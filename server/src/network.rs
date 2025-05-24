use crate::client_manager::ClientManager;
use crate::game::GameState;
use bincode::{deserialize, serialize};
use log::{debug, error, info, warn};
use shared::{InputState, Packet, Player};
use std::net::SocketAddr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::time::interval;

pub struct Server {
    socket: UdpSocket,
    clients: ClientManager,
    game_state: GameState,
    tick_duration: Duration,
}

impl Server {
    pub async fn new(
        addr: &str,
        tick_duration: Duration,
        max_clients: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind(addr).await?;
        info!("Server listening on {}", addr);

        Ok(Server {
            socket,
            clients: ClientManager::new(max_clients),
            game_state: GameState::new(),
            tick_duration,
        })
    }

    async fn send_packet(
        &self,
        packet: &Packet,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let data = serialize(packet)?;
        self.socket.send_to(&data, addr).await?;
        Ok(())
    }

    async fn handle_packet(&mut self, packet: Packet, addr: SocketAddr) {
        match packet {
            Packet::Connect { client_version } => {
                info!(
                    "Client connecting from {} (version: {})",
                    addr, client_version
                );

                if let Some(client_id) = self.clients.add_client(addr) {
                    self.game_state.add_player(client_id);

                    let response = Packet::Connected { client_id };
                    if let Err(e) = self.send_packet(&response, addr).await {
                        error!("Failed to send Connected packet: {}", e);
                        self.clients.remove_client(&client_id);
                        self.game_state.remove_player(&client_id);
                    }
                } else {
                    let response = Packet::Disconnected {
                        reason: "Server full".to_string(),
                    };
                    if let Err(e) = self.send_packet(&response, addr).await {
                        error!("Failed to send server full message: {}", e);
                    }
                }
            }

            Packet::Input {
                sequence,
                timestamp,
                left,
                right,
                jump,
            } => {
                if let Some(client_id) = self.clients.find_client_by_addr(addr) {
                    let input = InputState {
                        sequence,
                        timestamp,
                        left,
                        right,
                        jump,
                    };

                    self.clients.add_input(client_id, input);
                }
            }

            Packet::Disconnect => {
                if let Some(client_id) = self.clients.find_client_by_addr(addr) {
                    self.clients.remove_client(&client_id);
                    self.game_state.remove_player(&client_id);
                }
            }

            _ => {
                warn!("Unexpected packet type from client at {}", addr);
            }
        }
    }

    fn process_inputs(&mut self, dt: f32) {
        self.clients.process_inputs(|client_id, input, dt| {
            self.game_state.apply_input(client_id, input, dt);
        });
    }

    async fn broadcast_game_state(&mut self) {
        if self.clients.is_empty() {
            return;
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis() as u64;

        let players: Vec<Player> = self.game_state.players.values().cloned().collect();
        let last_processed_input = self.clients.get_last_processed_inputs();

        let packet = Packet::GameState {
            tick: self.game_state.tick,
            timestamp,
            last_processed_input,
            players,
        };

        let mut failed_clients = Vec::new();
        for (client_id, addr) in self.clients.get_client_addrs() {
            if let Err(e) = self.send_packet(&packet, addr).await {
                error!("Failed to send to client {}: {}", client_id, e);
                failed_clients.push(client_id);
            }
        }

        for client_id in failed_clients {
            self.clients.remove_client(&client_id);
            self.game_state.remove_player(&client_id);
        }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut tick_interval = interval(self.tick_duration);
        let mut last_tick = Instant::now();
        let mut buffer = [0u8; 2048];

        info!("Server started successfully");

        loop {
            tokio::select! {
                result = self.socket.recv_from(&mut buffer) => {
                    match result {
                        Ok((len, addr)) => {
                            if let Ok(packet) = deserialize::<Packet>(&buffer[0..len]) {
                                self.handle_packet(packet, addr).await;
                            } else {
                                warn!("Failed to deserialize packet from {}", addr);
                            }
                        },
                        Err(e) => error!("Error receiving packet: {}", e),
                    }
                },

                _ = tick_interval.tick() => {
                    let now = Instant::now();
                    let dt = now.duration_since(last_tick).as_secs_f32();
                    last_tick = now;

                    self.process_inputs(dt);
                    self.game_state.step(dt);

                    self.broadcast_game_state().await;

                    let timed_out = self.clients.check_timeouts();
                    for client_id in timed_out {
                        self.game_state.remove_player(&client_id);
                    }

                    if self.game_state.tick % 300 == 0 && !self.clients.is_empty() {
                        debug!("Tick {}: {} clients", self.game_state.tick, self.clients.len());
                    }
                },
            }
        }
    }
}
