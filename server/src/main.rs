use bincode::{deserialize, serialize};
use clap::Parser;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::Mutex;
use tokio::time::sleep;

// Command line arguments
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Server IP address to bind to
    #[clap(short = 'H', long, default_value = "0.0.0.0")]
    host: String,

    /// Server port to listen on
    #[clap(short, long, default_value = "8080")]
    port: u16,

    /// Tick rate (updates per second)
    #[clap(short, long, default_value = "60")]
    tick_rate: u32,
}

// Main entry point
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();
    
    // Print a message about setting RUST_LOG if not set
    if std::env::var("RUST_LOG").is_err() {
        eprintln!("Warning: RUST_LOG environment variable not set. Set it to display logs!");
        eprintln!("Recommended: RUST_LOG=info cargo run");
    }

    // Parse command line arguments
    let args = Args::parse();
    let addr = format!("{}:{}", args.host, args.port);
    let tick_interval = Duration::from_secs_f32(1.0 / args.tick_rate as f32);

    info!("Starting game server on {}", addr);
    info!("Tick rate: {} Hz ({:?} per tick)", args.tick_rate, tick_interval);

    // Create UDP socket
    let socket = UdpSocket::bind(&addr).await?;
    let socket_arc = Arc::new(socket);
    info!("UDP socket bound successfully");

    // Create shared game state
    let game_state = Arc::new(Mutex::new(GameState::new()));

    // Channel for communication between network and game threads
    let (net_tx, game_rx) = mpsc::channel::<NetworkEvent>(100);

    // Spawn game loop task
    let game_state_clone_loop = game_state.clone();
    let socket_clone_loop = socket_arc.clone();
    tokio::spawn(async move {
        run_game_loop(game_state_clone_loop, socket_clone_loop, game_rx, tick_interval).await;
    });

    // Network handling loop
    let mut buf = [0u8; 2048];
    loop {
        match socket_arc.recv_from(&mut buf).await {
            Ok((len, client_addr)) => {
                // Log the received data with more details
                info!("Received {} bytes from {}", len, client_addr);
                
                // Try to display the raw message (may not be valid UTF-8)
                match std::str::from_utf8(&buf[..len]) {
                    Ok(s) if s.len() < 100 => info!("Raw message: {:?}", s),
                    Ok(_) => info!("Raw message: long UTF-8 data (binary)"),
                    Err(_) => info!("Raw message: non-UTF8 binary data"),
                }
                
                // Process the data
                let data = &buf[..len];
                
                // Pass the socket_arc to handle_incoming_packet
                if let Err(e) = handle_incoming_packet(data, client_addr, &game_state, &net_tx, &socket_arc).await {
                    warn!("Error handling packet from {}: {}", client_addr, e);
                    
                    // Send a simple error response for non-binary clients like netcat
                    let error_msg = format!("Error: {}. Note: This server expects binary data.", e);
                    let _ = socket_arc.send_to(error_msg.as_bytes(), client_addr).await;
                }
            }
            Err(e) => {
                error!("Error receiving data: {}", e);
            }
        }
    }
}

// Packet types for client-server communication
#[derive(Debug, Serialize, Deserialize, Clone)]
enum Packet {
    // Connection management
    ConnectionRequest { client_version: u32 },
    ConnectionAccepted { client_id: u32 },
    Heartbeat { timestamp: u64 },
    Disconnect { reason: String },

    // Game state
    PlayerInput {
        sequence: u32,
        timestamp: u64,
        input_vector: (f32, f32),
    },
    GameState {
        timestamp: u64,
        last_processed_input: u32,
        entities: Vec<Entity>,
    },
}

// Entity representation
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Entity {
    id: u32,
    entity_type: EntityType,
    position: (f32, f32),
    velocity: (f32, f32),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum EntityType {
    Player,
}

// Input state from client
#[derive(Debug, Clone)]
struct InputState {
    sequence: u32,
    timestamp: u64,
    input_vector: (f32, f32),
}

// Client representation
#[derive(Debug)]
struct Client {
    id: u32,
    addr: SocketAddr,
    last_seen: Instant,
    entity_id: u32,
    last_processed_input: u32,
    input_buffer: Vec<InputState>,
}

// Game state
struct GameState {
    clients: HashMap<SocketAddr, Client>,
    entities: HashMap<u32, Entity>,
    next_client_id: u32,
    next_entity_id: u32,
}

// Events from network thread to game thread
enum NetworkEvent {
    NewConnection { addr: SocketAddr },
    ClientDisconnect { addr: SocketAddr },
    PlayerInput { 
        addr: SocketAddr,
        sequence: u32,
        timestamp: u64,
        input_vector: (f32, f32),
    },
}

impl GameState {
    fn new() -> Self {
        GameState {
            clients: HashMap::new(),
            entities: HashMap::new(),
            next_client_id: 1,
            next_entity_id: 1,
        }
    }

    // Register a new client
    fn add_client(&mut self, addr: SocketAddr) -> u32 {
        // Check if client already exists
        if let Some(client) = self.clients.get(&addr) {
            info!("Client reconnecting from {}, reusing ID {}", addr, client.id);
            return client.id;
        }
        
        let client_id = self.next_client_id;
        self.next_client_id += 1;

        // Create entity for this client
        let entity_id = self.next_entity_id;
        self.next_entity_id += 1;

        // Randomize starting position
        let position = (
            200.0 + (client_id as f32 * 50.0) % 400.0,
            200.0 + (client_id as f32 * 30.0) % 300.0,
        );

        // Create player entity
        let entity = Entity {
            id: entity_id,
            entity_type: EntityType::Player,
            position,
            velocity: (0.0, 0.0),
        };
        self.entities.insert(entity_id, entity);

        // Create client record
        let client = Client {
            id: client_id,
            addr,
            last_seen: Instant::now(),
            entity_id,
            last_processed_input: 0,
            input_buffer: Vec::new(),
        };
        self.clients.insert(addr, client);

        info!("Client {} connected from {}", client_id, addr);
        client_id
    }

    // Remove a client and their entity
    fn remove_client(&mut self, addr: &SocketAddr) {
        if let Some(client) = self.clients.remove(addr) {
            info!("Client {} disconnected", client.id);
            self.entities.remove(&client.entity_id);
        }
    }

    // Update client's input state
    fn update_client_input(&mut self, addr: &SocketAddr, input: InputState) -> Result<(), String> {
        if let Some(client) = self.clients.get_mut(addr) {
            // Update last seen time
            client.last_seen = Instant::now();
            
            // Store input in buffer (will be processed during physics update)
            client.input_buffer.push(input);
            Ok(())
        } else {
            Err("Client not found".to_string())
        }
    }

    // Update game physics
    fn update(&mut self, dt: f32) {
        // Process inputs for each client
        for client in self.clients.values_mut() {
            // Sort inputs by sequence number
            client.input_buffer.sort_by_key(|input| input.sequence);
            
            // Process all inputs in the buffer
            while let Some(input) = client.input_buffer.first() {
                // Only process new inputs
                if input.sequence <= client.last_processed_input {
                    client.input_buffer.remove(0);
                    continue;
                }
                
                // Get the client's entity
                if let Some(entity) = self.entities.get_mut(&client.entity_id) {
                    // Update velocity based on input
                    // units per second
                    let speed = 200.0;
                    let input_vec = input.input_vector;
                    
                    // Normalize input vector if needed
                    let magnitude = (input_vec.0.powi(2) + input_vec.1.powi(2)).sqrt();
                    let (input_x, input_y) = if magnitude > 0.0 {
                        (input_vec.0 / magnitude, input_vec.1 / magnitude)
                    } else {
                        (0.0, 0.0)
                    };
                    
                    entity.velocity = (input_x * speed, input_y * speed);
                    
                    // Update last processed input
                    client.last_processed_input = input.sequence;
                }
                
                // Remove processed input
                client.input_buffer.remove(0);
            }
        }
        
        // Update positions based on velocities
        for entity in self.entities.values_mut() {
            entity.position.0 += entity.velocity.0 * dt;
            entity.position.1 += entity.velocity.1 * dt;
            
            // Boundary constraints
            entity.position.0 = entity.position.0.max(0.0).min(800.0);
            entity.position.1 = entity.position.1.max(0.0).min(600.0);
        }
    }

    // Get the current timestamp
    fn get_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis() as u64
    }
}

// Handle incoming packets
async fn handle_incoming_packet(
    data: &[u8],
    addr: SocketAddr,
    game_state: &Arc<Mutex<GameState>>,
    net_tx: &Sender<NetworkEvent>,
    socket: &Arc<UdpSocket>,
) -> Result<(), String> {
    // First check if this is a text command for debugging
    if let Ok(text) = std::str::from_utf8(data) {
        if text.starts_with("debug:") {
            let command = &text[6..];
            info!("Processing debug command: {}", command);
            
            match command.trim() {
                "ping" => {
                    info!("Received ping, sending pong response");
                    socket.send_to(b"pong", addr).await
                        .map_err(|e| format!("Failed to send pong: {}", e))?;
                    return Ok(());
                }
                "status" => {
                    let state = game_state.lock().await;
                    let client_count = state.clients.len();
                    let entity_count = state.entities.len();
                    let status = format!("Server status: {} clients, {} entities", 
                                       client_count, entity_count);
                    info!("{}", status);
                    socket.send_to(status.as_bytes(), addr).await
                        .map_err(|e| format!("Failed to send status: {}", e))?;
                    return Ok(());
                }
                _ => {
                    // Unknown debug command
                    let response = format!("Unknown debug command: {}", command.trim());
                    socket.send_to(response.as_bytes(), addr).await
                        .map_err(|e| format!("Failed to send response: {}", e))?;
                    return Ok(());
                }
            }
        } else if text.trim().is_empty() || text.trim().eq("hello") {
            // Special case for common test messages
            let msg = "Hello! This is a binary protocol server. Use 'debug:ping' or 'debug:status' for text commands.";
            socket.send_to(msg.as_bytes(), addr).await
                .map_err(|e| format!("Failed to send hello response: {}", e))?;
            return Ok(());
        }
        
        // If not a recognized text command, continue with binary deserialization
    }
    
    // Parse packet as binary
    let packet: Packet = match deserialize(data) {
        Ok(p) => p,
        Err(e) => {
            let error_msg = format!("Failed to deserialize packet: {}", e);
            warn!("{}", error_msg);
            
            // Try to send a helpful response
            let response = "Error: Invalid binary packet format. This server expects bincode-serialized data.";
            socket.send_to(response.as_bytes(), addr).await
                .map_err(|e| format!("Failed to send error response: {}", e))?;
            
            return Err(error_msg);
        }
    };
    
    debug!("Deserialized packet from {}: {:?}", addr, packet);

    match packet {
        Packet::ConnectionRequest { client_version } => {
            debug!("Connection request from {} (version: {})", addr, client_version);
            
            let client_id = {
                let mut state = game_state.lock().await;
                state.add_client(addr)
            };
            
            let response = Packet::ConnectionAccepted { client_id };
            send_packet(&response, addr, socket).await?;
            
            net_tx.send(NetworkEvent::NewConnection { addr }).await
                .map_err(|_| "Failed to send network event".to_string())?;
        }
        
        Packet::PlayerInput { sequence, timestamp, input_vector } => {
            debug!("PlayerInput from {}: seq {}, ts {}, vec {:?}", addr, sequence, timestamp, input_vector);
            net_tx.send(NetworkEvent::PlayerInput {
                addr,
                sequence,
                timestamp,
                input_vector,
            }).await.map_err(|e| format!("Failed to send PlayerInput to game thread: {}", e))?;
        }
        
        Packet::Heartbeat { timestamp } => {
            debug!("Heartbeat from {}: ts {}", addr, timestamp);
            let mut state = game_state.lock().await;
            if let Some(client) = state.clients.get_mut(&addr) {
                client.last_seen = Instant::now();
                // Send heartbeat acknowledgment
                let ack_response = Packet::Heartbeat { timestamp: GameState::get_timestamp() };
                send_packet(&ack_response, addr, socket).await?;
            } else {
                warn!("Heartbeat from unknown client: {}", addr);
                
                // Auto-register new client on heartbeat
                let client_id = state.add_client(addr);
                let response = Packet::ConnectionAccepted { client_id };
                send_packet(&response, addr, socket).await?;
                
                net_tx.send(NetworkEvent::NewConnection { addr }).await
                    .map_err(|_| "Failed to send network event".to_string())?;
            }
        }
        
        Packet::Disconnect { reason } => {
            info!("Disconnect request from {}: {}", addr, reason);
            let mut state = game_state.lock().await;
            state.remove_client(&addr);
            
            net_tx.send(NetworkEvent::ClientDisconnect { addr }).await
                .map_err(|e| format!("Failed to send ClientDisconnect to game thread: {}", e))?;
            
            // Acknowledge disconnect
            let ack_response = Packet::Disconnect { reason: "Goodbye".to_string() };
            send_packet(&ack_response, addr, socket).await?;
        }
        
        Packet::ConnectionAccepted { .. } | Packet::GameState { .. } => {
            warn!("Received server-sent type packet {:?} from client {}", packet, addr);
            let err_msg = format!("Client {} sent unexpected server-type packet", addr);
            let response = Packet::Disconnect { reason: err_msg.clone() };
            send_packet(&response, addr, socket).await?;
            return Err(err_msg);
        }
    }
    
    Ok(())
}

// Send a packet to a client
async fn send_packet(
    packet: &Packet,
    addr: SocketAddr,
    socket_to_use: &Arc<UdpSocket>,
) -> Result<(), String> {
    // Serialize packet
    let data = serialize(packet)
        .map_err(|e| format!("Failed to serialize packet: {}", e))?;
    
    // Send data
    socket_to_use.send_to(&data, addr).await
        .map_err(|e| format!("Failed to send packet: {}", e))?;
    
    debug!("Sent packet {:?} to {}", packet, addr);
    Ok(())
}

// Game loop
async fn run_game_loop(
    game_state: Arc<Mutex<GameState>>,
    socket: Arc<UdpSocket>,
    mut network_rx: Receiver<NetworkEvent>,
    tick_interval: Duration,
) {
    let mut last_tick = Instant::now();
    let mut tick_count = 0u64;
    
    loop {
        // Process network events
        while let Ok(event) = network_rx.try_recv() {
            match event {
                NetworkEvent::NewConnection { addr } => {
                    debug!("Game thread: New connection from {}", addr);
                }
                
                NetworkEvent::ClientDisconnect { addr } => {
                    debug!("Game thread: Client disconnected: {}", addr);
                }
                
                NetworkEvent::PlayerInput { addr, sequence, timestamp, input_vector } => {
                    let mut state = game_state.lock().await;
                    let input = InputState {
                        sequence,
                        timestamp,
                        input_vector,
                    };
                    
                    if let Err(e) = state.update_client_input(&addr, input) {
                        warn!("Error updating client input: {}", e);
                    }
                }
            }
        }
        
        // Calculate delta time
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;
        
        // Update game state
        {
            let mut state = game_state.lock().await;
            state.update(dt);
            
            // Send game state to all clients
            broadcast_game_state(&state, &socket).await;
            
            // Check for disconnected clients (timeout)
            check_client_timeouts(&mut state).await;
            
            // Log status periodically
            tick_count += 1;
            if tick_count % 300 == 0 {
                info!("Server status: {} clients, {} entities", 
                    state.clients.len(), state.entities.len());
            }
        }
        
        // Sleep until next tick
        sleep(tick_interval).await;
    }
}

// Send game state to all clients
async fn broadcast_game_state(state: &GameState, socket: &Arc<UdpSocket>) {
    let timestamp = GameState::get_timestamp();
    
    // Convert entities to list
    let entities: Vec<Entity> = state.entities.values().cloned().collect();
    
    // Send game state to each client
    for (addr, client) in &state.clients {
        let packet = Packet::GameState {
            timestamp,
            last_processed_input: client.last_processed_input,
            entities: entities.clone(),
        };
        
        // Serialize and send
        if let Ok(data) = serialize(&packet) {
            if let Err(e) = socket.send_to(&data, addr).await {
                warn!("Failed to send game state to {}: {}", addr, e);
            }
        }
    }
}

// Check for client timeouts
async fn check_client_timeouts(state: &mut GameState) {
    let timeout = Duration::from_secs(5);
    let now = Instant::now();
    
    // Collect addresses of timed-out clients
    let timed_out: Vec<SocketAddr> = state.clients
        .iter()
        .filter(|(_, client)| now.duration_since(client.last_seen) > timeout)
        .map(|(addr, _)| *addr)
        .collect();
    
    // Remove timed-out clients
    for addr in timed_out {
        info!("Client {} timed out", addr);
        state.remove_client(&addr);
    }
}