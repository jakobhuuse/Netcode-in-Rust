//! # Server Network Layer
//!
//! This module implements the core networking layer for the authoritative game server.
//! It handles UDP communications, packet processing, and coordinates between the game
//! simulation and client connections.
//!
//! ## Architecture Overview
//!
//! The server uses an event-driven architecture with multiple concurrent tasks:
//! - **Network Receiver**: Continuously listens for incoming packets from clients
//! - **Network Sender**: Processes outgoing packet queue and handles broadcasts
//! - **Timeout Checker**: Monitors client connection health and removes inactive clients
//! - **Main Game Loop**: Processes inputs, runs physics simulation, and broadcasts state
//!
//! ## Concurrency Model
//!
//! The server is designed for high-performance concurrent operation:
//! - Each network task runs independently without blocking the game loop
//! - Client management uses async read-write locks for safe concurrent access
//! - Input processing is batched and distributed across physics substeps for stability
//! - Adaptive physics stepping prevents tunneling issues with fast-moving objects
//!
//! ## Packet Flow
//!
//! 1. **Incoming**: UDP packets → deserialize → queue → main loop → process
//! 2. **Outgoing**: game events → queue → network sender → serialize → UDP send
//! 3. **Broadcast**: game state → replicate to all connected clients (except sender)

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

/// Messages sent from network tasks to the main server loop.
/// These represent events that require processing by the game simulation.
#[derive(Debug)]
pub enum ServerMessage {
    /// A packet was received from a client and needs processing
    PacketReceived { packet: Packet, addr: SocketAddr },
    /// A client has timed out and should be removed from the game
    ClientTimeout { client_id: u32 },
    /// Server shutdown signal (currently unused but reserved for graceful shutdown)
    #[allow(dead_code)]
    Shutdown,
}

/// Messages sent from the main game loop to network tasks.
/// These represent outgoing network operations that need to be performed.
#[derive(Debug)]
pub enum GameMessage {
    /// Send a packet to a specific client
    SendPacket { packet: Packet, addr: SocketAddr },
    /// Broadcast a packet to all clients, optionally excluding one
    BroadcastPacket {
        packet: Packet,
        exclude: Option<u32>,
    },
}

/// The main server struct that orchestrates all networking and game simulation.
///
/// This struct manages the authoritative game state and coordinates communication
/// between multiple concurrent tasks:
/// - Network receiver task for incoming packets
/// - Network sender task for outgoing packets and broadcasts  
/// - Timeout checker task for connection health monitoring
/// - Main game loop for physics simulation and state synchronization
///
/// ## Concurrency Design
///
/// The server uses message-passing channels to communicate between tasks, avoiding
/// the need for complex locking mechanisms in the hot path. The game state and client
/// manager are protected by async RwLocks to allow concurrent reads while ensuring
/// exclusive access for writes.
pub struct Server {
    /// Shared UDP socket for all network communication
    socket: Arc<UdpSocket>,
    /// Thread-safe client connection manager
    clients: Arc<RwLock<ClientManager>>,
    /// Authoritative game state (physics, player positions, etc.)
    game_state: GameState,
    /// Target duration between server ticks (typically 16.67ms for 60Hz)
    tick_duration: Duration,

    /// Channel for sending messages to the main server loop
    server_tx: mpsc::UnboundedSender<ServerMessage>,
    /// Channel for receiving messages in the main server loop
    server_rx: mpsc::UnboundedReceiver<ServerMessage>,
    /// Channel for sending packets from game loop to network sender
    game_tx: mpsc::UnboundedSender<GameMessage>,
    /// Channel for receiving packets in the network sender task
    game_rx: mpsc::UnboundedReceiver<GameMessage>,
}

impl Server {
    /// Creates a new server instance bound to the specified address.
    ///
    /// # Arguments
    /// * `addr` - The address to bind the UDP socket to (e.g., "127.0.0.1:8080")
    /// * `tick_duration` - Target time between server simulation ticks
    /// * `max_clients` - Maximum number of concurrent client connections
    ///
    /// # Returns
    /// A Result containing the initialized Server or an error if binding fails
    ///
    /// # Example
    /// ```rust
    /// # use std::time::Duration;
    /// # tokio_test::block_on(async {
    /// let server = server::network::Server::new("127.0.0.1:8080", Duration::from_millis(16), 32).await;
    /// # });
    /// ```
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

    /// Spawns the network receiver task that continuously listens for incoming packets.
    ///
    /// This task runs independently and deserializes incoming UDP packets, forwarding
    /// valid packets to the main server loop via the ServerMessage channel. Invalid
    /// packets are logged and discarded.
    ///
    /// ## Error Handling
    /// - Network errors trigger a brief pause before retrying
    /// - Deserialization errors are logged but don't crash the task
    /// - Channel send failures indicate main loop termination and cause task exit
    async fn spawn_network_receiver(&self) {
        let socket = Arc::clone(&self.socket);
        let server_tx = self.server_tx.clone();

        tokio::spawn(async move {
            let mut buffer = [0u8; 2048]; // 2KB buffer sufficient for game packets

            loop {
                match socket.recv_from(&mut buffer).await {
                    Ok((len, addr)) => {
                        // Attempt to deserialize the received packet
                        if let Ok(packet) = deserialize::<Packet>(&buffer[0..len]) {
                            // Forward valid packet to main loop for processing
                            if let Err(e) =
                                server_tx.send(ServerMessage::PacketReceived { packet, addr })
                            {
                                error!("Failed to send packet to main loop: {}", e);
                                break; // Main loop has terminated
                            }
                        } else {
                            warn!("Failed to deserialize packet from {}", addr);
                        }
                    }
                    Err(e) => {
                        error!("Error receiving packet: {}", e);
                        // Brief pause to prevent busy-waiting on persistent errors
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            }
        });
    }

    /// Spawns the network sender task that processes outgoing packet queue.
    ///
    /// This task handles two types of outgoing messages:
    /// 1. **DirectSend**: Packets targeted at specific clients
    /// 2. **Broadcast**: Packets sent to all clients (with optional exclusions)
    ///
    /// The sender task runs independently from the main game loop to prevent
    /// network I/O from blocking game simulation timing.
    async fn spawn_network_sender(&mut self) {
        let socket = Arc::clone(&self.socket);
        let clients = Arc::clone(&self.clients);
        let mut game_rx = std::mem::replace(&mut self.game_rx, mpsc::unbounded_channel().1);

        tokio::spawn(async move {
            while let Some(message) = game_rx.recv().await {
                match message {
                    // Send packet to a specific client
                    GameMessage::SendPacket { packet, addr } => {
                        if let Err(e) = Self::send_packet_impl(&socket, &packet, addr).await {
                            error!("Failed to send packet to {}: {}", addr, e);
                        }
                    }
                    // Broadcast packet to all connected clients
                    GameMessage::BroadcastPacket { packet, exclude } => {
                        // Get snapshot of all client addresses
                        let client_addrs = {
                            let clients_guard = clients.read().await;
                            clients_guard.get_client_addrs()
                        };

                        // Send to each client (except excluded one)
                        for (client_id, addr) in client_addrs {
                            if Some(client_id) == exclude {
                                continue; // Skip excluded client (usually the sender)
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

    /// Spawns the client timeout monitoring task.
    ///
    /// This task periodically checks for clients that haven't sent packets within
    /// the timeout window and notifies the main loop to remove them. This prevents
    /// zombie connections from accumulating and consuming server resources.
    ///
    /// Runs every second to balance responsiveness with system overhead.
    async fn spawn_timeout_checker(&self) {
        let clients = Arc::clone(&self.clients);
        let server_tx = self.server_tx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));

            loop {
                interval.tick().await;

                // Check for timed out clients
                let timed_out = {
                    let mut clients_guard = clients.write().await;
                    clients_guard.check_timeouts()
                };

                // Notify main loop about each timed out client
                for client_id in timed_out {
                    if let Err(e) = server_tx.send(ServerMessage::ClientTimeout { client_id }) {
                        error!("Failed to send timeout message: {}", e);
                        break; // Main loop has terminated
                    }
                }
            }
        });
    }

    /// Low-level packet sending implementation.
    ///
    /// Serializes the packet using bincode and sends it via UDP. This is used by
    /// both direct sends and broadcasts.
    ///
    /// # Arguments
    /// * `socket` - The UDP socket to send through
    /// * `packet` - The packet to serialize and send
    /// * `addr` - The destination address
    ///
    /// # Returns
    /// Result indicating success or failure with error details
    async fn send_packet_impl(
        socket: &UdpSocket,
        packet: &Packet,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let data = serialize(packet)?; // Serialize packet to binary format
        socket.send_to(&data, addr).await?; // Send via UDP
        Ok(())
    }

    /// Queues a packet for sending to a specific client.
    ///
    /// This method doesn't block - it adds the packet to the outgoing queue
    /// where the network sender task will process it asynchronously.
    async fn send_packet(&self, packet: &Packet, addr: SocketAddr) {
        if let Err(e) = self.game_tx.send(GameMessage::SendPacket {
            packet: packet.clone(),
            addr,
        }) {
            error!("Failed to queue packet for sending: {}", e);
        }
    }

    /// Queues a packet for broadcasting to all connected clients.
    ///
    /// # Arguments
    /// * `packet` - The packet to broadcast
    /// * `exclude` - Optional client ID to exclude from the broadcast (e.g., the sender)
    ///
    /// This is commonly used for game state updates where all clients need to see
    /// the current state, but the originating client might be excluded to avoid
    /// redundant information.
    async fn broadcast_packet(&self, packet: &Packet, exclude: Option<u32>) {
        if let Err(e) = self.game_tx.send(GameMessage::BroadcastPacket {
            packet: packet.clone(),
            exclude,
        }) {
            error!("Failed to queue broadcast packet: {}", e);
        }
    }

    /// Processes incoming packets and updates game state accordingly.
    ///
    /// This method handles all packet types that clients can send:
    /// - **Connect**: New client connection requests
    /// - **Input**: Player input for game simulation
    /// - **Disconnect**: Graceful client disconnection
    ///
    /// ## Connection Management
    /// - Duplicate connections from the same address are handled by removing the old connection
    /// - Server capacity limits are enforced during connection attempts
    /// - All connections are tracked with unique client IDs
    async fn handle_packet(&mut self, packet: Packet, addr: SocketAddr) {
        match packet {
            // Handle new client connection
            Packet::Connect { client_version } => {
                info!(
                    "Client connecting from {} (version: {})",
                    addr, client_version
                );

                // Check if this address already has a connection
                let existing_client_id = {
                    let clients = self.clients.read().await;
                    clients.find_client_by_addr(addr)
                };

                // Remove existing connection if present (reconnection scenario)
                if let Some(existing_id) = existing_client_id {
                    info!("Removing existing client {} from {}", existing_id, addr);
                    let mut clients = self.clients.write().await;
                    clients.remove_client(&existing_id);
                    self.game_state.remove_player(&existing_id);
                }

                // Attempt to add new client
                let client_id = {
                    let mut clients = self.clients.write().await;
                    clients.add_client(addr)
                };

                if let Some(client_id) = client_id {
                    // Successfully added client
                    self.game_state.add_player(client_id);

                    let response = Packet::Connected { client_id };
                    self.send_packet(&response, addr).await;
                } else {
                    // Server is full
                    let response = Packet::Disconnected {
                        reason: "Server full".to_string(),
                    };
                    self.send_packet(&response, addr).await;
                }
            }

            // Handle player input
            Packet::Input {
                sequence,
                timestamp,
                left,
                right,
                jump,
            } => {
                // Find client ID for this address
                let client_id = {
                    let clients = self.clients.read().await;
                    clients.find_client_by_addr(addr)
                };

                if let Some(client_id) = client_id {
                    // Create input state and queue it for processing
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

            // Handle client disconnection
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

            // Handle unexpected packet types
            _ => {
                warn!("Unexpected packet type from client at {}", addr);
            }
        }
    }

    /// Processes all queued client inputs and advances the physics simulation.
    ///
    /// This method implements several critical netcode concepts:
    ///
    /// ## Adaptive Physics Stepping
    /// - Calculates required substeps to prevent collision tunneling
    /// - Distributes input processing across substeps for temporal accuracy
    /// - Yields CPU periodically during intensive processing
    ///
    /// ## Input Processing Order
    /// - Processes inputs in chronological order across all clients
    /// - Ensures deterministic simulation regardless of packet arrival order
    /// - Batches inputs efficiently to balance accuracy with performance
    ///
    /// ## Concurrency Considerations
    /// - Periodically yields during long processing to maintain responsiveness
    /// - Cleans up processed inputs to prevent memory growth
    async fn process_inputs(&mut self, dt: f32) {
        // Calculate physics substeps needed to prevent tunneling
        let total_substeps = self.calculate_required_substeps(dt);
        let substep_dt = dt / total_substeps as f32;

        // Get all inputs sorted chronologically across all clients
        let all_inputs = {
            let clients = self.clients.read().await;
            clients.get_chronological_inputs()
        };

        // If no inputs, just run physics
        if all_inputs.is_empty() {
            for _ in 0..total_substeps {
                self.game_state.update_physics(substep_dt);
            }
            return;
        }

        // Distribute input processing across physics substeps
        let batch_size = 50; // Maximum inputs per substep
        let inputs_per_substep = if all_inputs.len() >= total_substeps as usize {
            (all_inputs.len() / total_substeps as usize).min(batch_size)
        } else {
            batch_size.min(all_inputs.len())
        };

        let mut input_index = 0;

        // Process each physics substep
        for substep in 0..total_substeps {
            // Calculate inputs for this substep
            let inputs_this_step = if substep == total_substeps - 1 {
                // Last substep processes any remaining inputs
                all_inputs.len() - input_index
            } else {
                inputs_per_substep.min(all_inputs.len() - input_index)
            };

            // Apply inputs for this substep
            for _ in 0..inputs_this_step {
                if input_index < all_inputs.len() {
                    let (client_id, input) = &all_inputs[input_index];
                    self.game_state.apply_input(*client_id, input, substep_dt);

                    // Mark input as processed
                    let mut clients = self.clients.write().await;
                    clients.mark_input_processed(*client_id, input.sequence);
                    input_index += 1;
                }
            }

            // Run physics simulation for this substep
            self.game_state.update_physics(substep_dt);

            // Yield CPU periodically during intensive processing
            if substep % 10 == 0 && total_substeps > 20 {
                tokio::task::yield_now().await;
            }
        }

        // Clean up processed inputs to prevent memory growth
        let mut clients = self.clients.write().await;
        clients.cleanup_processed_inputs();
    }

    /// Calculates the number of physics substeps required to prevent tunneling.
    ///
    /// This implements a critical netcode stability feature. Fast-moving objects
    /// can "tunnel" through collision boundaries if the physics timestep is too large
    /// relative to their movement speed.
    ///
    /// ## Algorithm
    /// - Uses the maximum possible player speed and minimum collision radius
    /// - Ensures no object moves more than half its collision radius per substep
    /// - Includes a safety factor for additional stability margin
    ///
    /// # Arguments
    /// * `dt` - The frame delta time in seconds
    ///
    /// # Returns
    /// The number of physics substeps needed (minimum 1)
    fn calculate_required_substeps(&self, dt: f32) -> u32 {
        // Physics stability constants
        const MAX_PLAYER_SPEED: f32 = PLAYER_SPEED;
        const MIN_COLLISION_RADIUS: f32 = PLAYER_SIZE / 2.0;
        const SAFETY_FACTOR: f32 = 0.5; // Additional stability margin

        // Maximum safe movement per physics step
        let max_movement_per_step = MIN_COLLISION_RADIUS * SAFETY_FACTOR;
        // Total movement this frame at maximum speed
        let max_movement_this_tick = MAX_PLAYER_SPEED * dt;

        if max_movement_this_tick > max_movement_per_step {
            // Need multiple substeps to prevent tunneling
            (max_movement_this_tick / max_movement_per_step).ceil() as u32
        } else {
            // Single step is sufficient
            1
        }
    }

    /// Broadcasts the current game state to all connected clients.
    ///
    /// This is the primary mechanism for keeping clients synchronized with the
    /// authoritative server state. The broadcast includes:
    /// - Current server tick number for temporal synchronization
    /// - Timestamp for latency calculations
    /// - Last processed input sequence per client (for acknowledgment)
    /// - Complete player state for all players in the game
    ///
    /// ## Optimization Notes
    /// - Early returns if no clients are connected
    /// - Snapshot approach prevents holding locks during broadcast
    /// - Single packet construction reused for all recipients
    async fn broadcast_game_state(&mut self) {
        // Early return if no clients to avoid unnecessary work
        let client_count = {
            let clients = self.clients.read().await;
            clients.len()
        };

        if client_count == 0 {
            return;
        }

        // Generate timestamp for client latency calculations
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis() as u64;

        // Create snapshot of current game state
        let players: Vec<Player> = self.game_state.players.values().cloned().collect();
        let last_processed_input = {
            let clients = self.clients.read().await;
            clients.get_last_processed_inputs()
        };

        // Construct game state packet
        let packet = Packet::GameState {
            tick: self.game_state.tick,
            timestamp,
            last_processed_input,
            players,
        };

        // Broadcast to all clients
        self.broadcast_packet(&packet, None).await;
    }

    /// Starts the main server loop that coordinates all operations.
    ///
    /// This is the heart of the server that brings together all concurrent tasks:
    ///
    /// ## Initialization
    /// - Spawns network receiver, sender, and timeout checker tasks
    /// - Sets up precise timing for the server tick rate
    /// - Initializes performance monitoring
    ///
    /// ## Main Loop
    /// The loop uses `tokio::select!` to handle events concurrently:
    /// - **Network Events**: Process incoming packets and client timeouts
    /// - **Tick Events**: Run physics simulation and broadcast game state
    ///
    /// ## Performance Monitoring
    /// - Logs periodic statistics (every 60 ticks)
    /// - Reports client count, tick rate, and physics complexity
    /// - Helps identify performance issues in production
    ///
    /// # Returns
    /// Result indicating successful completion or error details
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Initialize all concurrent tasks
        self.spawn_network_receiver().await;
        self.spawn_network_sender().await;
        self.spawn_timeout_checker().await;

        // Set up precise server timing
        let mut tick_interval = interval(self.tick_duration);
        let mut last_tick = Instant::now();

        info!("Server started successfully with improved concurrency");

        loop {
            tokio::select! {
                // Handle network and client management events
                message = self.server_rx.recv() => {
                    match message {
                        Some(ServerMessage::PacketReceived { packet, addr }) => {
                            self.handle_packet(packet, addr).await;
                        },
                        Some(ServerMessage::ClientTimeout { client_id }) => {
                            // Remove timed out client from game state
                            self.game_state.remove_player(&client_id);
                        },
                        Some(ServerMessage::Shutdown) | None => {
                            info!("Server shutting down");
                            break;
                        }
                    }
                },

                // Handle server tick events (physics + state broadcast)
                _ = tick_interval.tick() => {
                    let now = Instant::now();
                    let dt = now.duration_since(last_tick).as_secs_f32();
                    last_tick = now;

                    // Process all queued inputs and run physics
                    self.process_inputs(dt).await;
                    self.game_state.tick += 1;

                    // Broadcast updated state to all clients
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
                                   self.game_state.tick,
                                   client_count,
                                   1.0 / dt,
                                   substeps);
                        }
                    }
                },
            }
        }

        Ok(())
    }
}
