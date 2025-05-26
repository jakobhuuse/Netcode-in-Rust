//! Server network layer handling UDP communications and game loop coordination

use crate::client_manager::ClientManager;
use crate::game::GameState;
use bincode::{deserialize, serialize};
use log::{debug, error, info, warn};
use shared::{InputState, Packet, Player, PLAYER_SIZE, PLAYER_SPEED};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;

/// Messages sent from network tasks to main server loop
#[derive(Debug)]
pub enum ServerMessage {
    PacketReceived {
        packet: Packet,
        addr: SocketAddr,
    },
    ClientTimeout {
        client_id: u32,
    },
    #[allow(dead_code)]
    Shutdown,
}

/// Messages sent from game loop to network tasks
#[derive(Debug)]
pub enum GameMessage {
    SendPacket {
        packet: Packet,
        addr: SocketAddr,
    },
    BroadcastPacket {
        packet: Packet,
        exclude: Option<u32>,
    },
}

/// Main server coordinating networking and game simulation
pub struct Server {
    socket: Arc<UdpSocket>,
    clients: Arc<RwLock<ClientManager>>,
    game_state: GameState,
    tick_duration: Duration,

    // Communication channels
    server_tx: mpsc::UnboundedSender<ServerMessage>,
    server_rx: mpsc::UnboundedReceiver<ServerMessage>,
    game_tx: mpsc::UnboundedSender<GameMessage>,
    game_rx: mpsc::UnboundedReceiver<GameMessage>,
}

impl Server {
    pub async fn new(
        addr: &str,
        tick_duration: Duration,
        max_clients: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let socket = Arc::new(UdpSocket::bind(addr).await?);
        info!("Server listening on {}", addr);

        let (server_tx, server_rx) = mpsc::unbounded_channel();
        let (game_tx, game_rx) = mpsc::unbounded_channel();

        Ok(Server {
            socket,
            clients: Arc::new(RwLock::new(ClientManager::new(max_clients))),
            game_state: GameState::new(),
            tick_duration,
            server_tx,
            server_rx,
            game_tx,
            game_rx,
        })
    }

    /// Spawns task that continuously listens for incoming packets
    async fn spawn_network_receiver(&self) {
        let socket = Arc::clone(&self.socket);
        let server_tx = self.server_tx.clone();

        tokio::spawn(async move {
            let mut buffer = [0u8; 2048];

            loop {
                match socket.recv_from(&mut buffer).await {
                    Ok((len, addr)) => {
                        if let Ok(packet) = deserialize::<Packet>(&buffer[0..len]) {
                            if let Err(e) =
                                server_tx.send(ServerMessage::PacketReceived { packet, addr })
                            {
                                error!("Failed to send packet to main loop: {}", e);
                                break;
                            }
                        } else {
                            warn!("Failed to deserialize packet from {}", addr);
                        }
                    }
                    Err(e) => {
                        error!("Error receiving packet: {}", e);
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            }
        });
    }

    /// Spawns task that processes outgoing packet queue
    async fn spawn_network_sender(&mut self) {
        let socket = Arc::clone(&self.socket);
        let clients = Arc::clone(&self.clients);
        let mut game_rx = std::mem::replace(&mut self.game_rx, mpsc::unbounded_channel().1);

        tokio::spawn(async move {
            while let Some(message) = game_rx.recv().await {
                match message {
                    GameMessage::SendPacket { packet, addr } => {
                        if let Err(e) = Self::send_packet_impl(&socket, &packet, addr).await {
                            error!("Failed to send packet to {}: {}", addr, e);
                        }
                    }
                    GameMessage::BroadcastPacket { packet, exclude } => {
                        let client_addrs = {
                            let clients_guard = clients.read().await;
                            clients_guard.get_client_addrs()
                        };

                        for (client_id, addr) in client_addrs {
                            if Some(client_id) == exclude {
                                continue;
                            }

                            if let Err(e) = Self::send_packet_impl(&socket, &packet, addr).await {
                                error!("Failed to send to client {}: {}", client_id, e);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Spawns task that monitors client timeouts
    async fn spawn_timeout_checker(&self) {
        let clients = Arc::clone(&self.clients);
        let server_tx = self.server_tx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));

            loop {
                interval.tick().await;

                let timed_out = {
                    let mut clients_guard = clients.write().await;
                    clients_guard.check_timeouts()
                };

                for client_id in timed_out {
                    if let Err(e) = server_tx.send(ServerMessage::ClientTimeout { client_id }) {
                        error!("Failed to send timeout message: {}", e);
                        break;
                    }
                }
            }
        });
    }

    async fn send_packet_impl(
        socket: &UdpSocket,
        packet: &Packet,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let data = serialize(packet)?;
        socket.send_to(&data, addr).await?;
        Ok(())
    }

    async fn send_packet(&self, packet: &Packet, addr: SocketAddr) {
        if let Err(e) = self.game_tx.send(GameMessage::SendPacket {
            packet: packet.clone(),
            addr,
        }) {
            error!("Failed to queue packet for sending: {}", e);
        }
    }

    async fn broadcast_packet(&self, packet: &Packet, exclude: Option<u32>) {
        if let Err(e) = self.game_tx.send(GameMessage::BroadcastPacket {
            packet: packet.clone(),
            exclude,
        }) {
            error!("Failed to queue broadcast packet: {}", e);
        }
    }

    /// Processes incoming packets and updates game state
    async fn handle_packet(&mut self, packet: Packet, addr: SocketAddr) {
        match packet {
            Packet::Connect { client_version } => {
                info!(
                    "Client connecting from {} (version: {})",
                    addr, client_version
                );

                // Remove existing connection if present
                let existing_client_id = {
                    let clients = self.clients.read().await;
                    clients.find_client_by_addr(addr)
                };

                if let Some(existing_id) = existing_client_id {
                    info!("Removing existing client {} from {}", existing_id, addr);
                    let mut clients = self.clients.write().await;
                    clients.remove_client(&existing_id);
                    self.game_state.remove_player(&existing_id);
                }

                // Try to add new client
                let client_id = {
                    let mut clients = self.clients.write().await;
                    clients.add_client(addr)
                };

                if let Some(client_id) = client_id {
                    self.game_state.add_player(client_id);
                    let response = Packet::Connected { client_id };
                    self.send_packet(&response, addr).await;
                } else {
                    let response = Packet::Disconnected {
                        reason: "Server full".to_string(),
                    };
                    self.send_packet(&response, addr).await;
                }
            }

            Packet::Input {
                sequence,
                timestamp,
                left,
                right,
                jump,
            } => {
                let client_id = {
                    let clients = self.clients.read().await;
                    clients.find_client_by_addr(addr)
                };

                if let Some(client_id) = client_id {
                    let input = InputState {
                        sequence,
                        timestamp,
                        left,
                        right,
                        jump,
                    };

                    let mut clients = self.clients.write().await;
                    clients.add_input(client_id, input);
                }
            }

            Packet::Disconnect => {
                let client_id = {
                    let clients = self.clients.read().await;
                    clients.find_client_by_addr(addr)
                };

                if let Some(client_id) = client_id {
                    let mut clients = self.clients.write().await;
                    clients.remove_client(&client_id);
                    self.game_state.remove_player(&client_id);
                }
            }

            _ => {
                warn!("Unexpected packet type from client at {}", addr);
            }
        }
    }

    /// Processes queued inputs and advances physics simulation
    async fn process_inputs(&mut self, dt: f32) {
        // Calculate physics substeps needed to prevent tunneling
        let total_substeps = self.calculate_required_substeps(dt);
        let substep_dt = dt / total_substeps as f32;

        let all_inputs = {
            let clients = self.clients.read().await;
            clients.get_chronological_inputs()
        };

        if all_inputs.is_empty() {
            // No inputs, just run physics
            for _ in 0..total_substeps {
                self.game_state.update_physics(substep_dt);
            }
            return;
        }

        // Distribute inputs across substeps
        let batch_size = 50;
        let inputs_per_substep = if all_inputs.len() >= total_substeps as usize {
            (all_inputs.len() / total_substeps as usize).min(batch_size)
        } else {
            batch_size.min(all_inputs.len())
        };

        let mut input_index = 0;

        for substep in 0..total_substeps {
            let inputs_this_step = if substep == total_substeps - 1 {
                all_inputs.len() - input_index
            } else {
                inputs_per_substep.min(all_inputs.len() - input_index)
            };

            // Apply inputs for this substep
            for _ in 0..inputs_this_step {
                if input_index < all_inputs.len() {
                    let (client_id, input) = &all_inputs[input_index];
                    self.game_state.apply_input(*client_id, input, substep_dt);

                    let mut clients = self.clients.write().await;
                    clients.mark_input_processed(*client_id, input.sequence);
                    input_index += 1;
                }
            }

            self.game_state.update_physics(substep_dt);

            // Yield CPU periodically during intensive processing
            if substep % 10 == 0 && total_substeps > 20 {
                tokio::task::yield_now().await;
            }
        }

        // Clean up processed inputs
        let mut clients = self.clients.write().await;
        clients.cleanup_processed_inputs();
    }

    /// Calculates physics substeps required to prevent collision tunneling
    fn calculate_required_substeps(&self, dt: f32) -> u32 {
        const MAX_PLAYER_SPEED: f32 = PLAYER_SPEED;
        const MIN_COLLISION_RADIUS: f32 = PLAYER_SIZE / 2.0;
        const SAFETY_FACTOR: f32 = 0.5;

        let max_movement_per_step = MIN_COLLISION_RADIUS * SAFETY_FACTOR;
        let max_movement_this_tick = MAX_PLAYER_SPEED * dt;

        if max_movement_this_tick > max_movement_per_step {
            (max_movement_this_tick / max_movement_per_step).ceil() as u32
        } else {
            1
        }
    }

    /// Broadcasts current game state to all connected clients
    async fn broadcast_game_state(&mut self) {
        let client_count = {
            let clients = self.clients.read().await;
            clients.len()
        };

        if client_count == 0 {
            return;
        }

        // Prepare packet data first
        let players: Vec<Player> = self.game_state.players.values().cloned().collect();
        let last_processed_input = {
            let clients = self.clients.read().await;
            clients.get_last_processed_inputs()
        };

        // Take timestamp as close to transmission as possible
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis();
        let timestamp_safe = (timestamp.min(u64::MAX as u128)) as u64;

        let packet = Packet::GameState {
            tick: self.game_state.tick,
            timestamp: timestamp_safe,
            last_processed_input,
            players,
        };

        self.broadcast_packet(&packet, None).await;
    }

    /// Main server loop coordinating all operations
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Initialize concurrent tasks
        self.spawn_network_receiver().await;
        self.spawn_network_sender().await;
        self.spawn_timeout_checker().await;

        let mut tick_interval = interval(self.tick_duration);
        let mut last_tick = Instant::now();

        info!("Server started successfully");

        loop {
            tokio::select! {
                // Handle network events
                message = self.server_rx.recv() => {
                    match message {
                        Some(ServerMessage::PacketReceived { packet, addr }) => {
                            self.handle_packet(packet, addr).await;
                        },
                        Some(ServerMessage::ClientTimeout { client_id }) => {
                            self.game_state.remove_player(&client_id);
                        },
                        Some(ServerMessage::Shutdown) | None => {
                            info!("Server shutting down");
                            break;
                        }
                    }
                },

                // Handle server tick events
                _ = tick_interval.tick() => {
                    let now = Instant::now();
                    let dt = now.duration_since(last_tick).as_secs_f32();
                    last_tick = now;

                    self.process_inputs(dt).await;
                    self.game_state.tick += 1;
                    self.broadcast_game_state().await;

                    // Periodic performance monitoring
                    if self.game_state.tick % 60 == 0 {
                        let client_count = {
                            let clients = self.clients.read().await;
                            clients.len()
                        };

                        if client_count > 0 {
                            let substeps = self.calculate_required_substeps(dt);
                            debug!("Tick {}: {} clients, {:.1}Hz, {} physics substeps",
                                   self.game_state.tick, client_count, 1.0 / dt, substeps);
                        }
                    }
                },
            }
        }

        Ok(())
    }
}
