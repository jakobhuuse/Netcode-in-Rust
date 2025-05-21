use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::time::sleep;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use clap::Parser;
use log::{debug, error, info, warn};

// Command line arguments
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Server IP address to bind to
    #[clap(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    /// WebSocket port to listen on
    #[clap(short = 'p', long, default_value = "8080")]
    port: u16,

    /// Tick rate (updates per second)
    #[clap(short = 't', long, default_value = "60")]
    tick_rate: u32,
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
        last_processed_input: HashMap<u32, u32>, // client_id -> sequence
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
    radius: f32,
    color: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum EntityType {
    Player,
}

// Input state from client
#[derive(Debug, Clone)]
struct InputState {
    client_id: u32,
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
    sender: mpsc::Sender<Message>,
}

// Game state
struct GameState {
    clients: HashMap<u32, Client>,
    entities: HashMap<u32, Entity>,
    next_client_id: u32,
    next_entity_id: u32,
    // Map world boundaries
    width: f32,
    height: f32,
}

// Events from network thread to game thread
enum NetworkEvent {
    NewConnection { client_id: u32, sender: mpsc::Sender<Message> },
    ClientDisconnect { client_id: u32 },
    PlayerInput { 
        client_id: u32,
        sequence: u32,
        timestamp: u64,
        input_vector: (f32, f32),
    },
}

impl GameState {
    fn new(width: f32, height: f32) -> Self {
        GameState {
            clients: HashMap::new(),
            entities: HashMap::new(),
            next_client_id: 1,
            next_entity_id: 1,
            width,
            height,
        }
    }

    // Register a new client
    fn add_client(&mut self, client_id: u32, addr: SocketAddr, sender: mpsc::Sender<Message>) -> u32 {
        // Create entity for this client
        let entity_id = self.next_entity_id;
        self.next_entity_id += 1;

        // Randomize starting position (away from edges)
        let position = (
            50.0 + (client_id as f32 * 50.0) % (self.width - 100.0),
            50.0 + (client_id as f32 * 30.0) % (self.height - 100.0),
        );

        // Generate a color based on client ID
        let colors = ["blue", "red", "green", "purple", "orange", "cyan", "magenta", "yellow"];
        let color = colors[(client_id as usize - 1) % colors.len()].to_string();

        // Create player entity
        let entity = Entity {
            id: entity_id,
            entity_type: EntityType::Player,
            position,
            velocity: (0.0, 0.0),
            radius: 20.0,
            color,
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
            sender,
        };
        self.clients.insert(client_id, client);

        info!("Client {} connected from {}", client_id, addr);
        client_id
    }

    // Remove a client and their entity
    fn remove_client(&mut self, client_id: &u32) {
        if let Some(client) = self.clients.remove(client_id) {
            info!("Client {} disconnected", client.id);
            self.entities.remove(&client.entity_id);
        }
    }

    // Update client's input state
    fn update_client_input(&mut self, client_id: &u32, input: InputState) -> Result<(), String> {
        if let Some(client) = self.clients.get_mut(client_id) {
            // Update last seen time
            client.last_seen = Instant::now();
            
            // Store input in buffer (will be processed during physics update)
            client.input_buffer.push(input);
            Ok(())
        } else {
            Err(format!("Client {} not found", client_id))
        }
    }

    // Update game physics - core of the rollback system
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
        
        // Update positions based on velocities and handle collisions
        self.update_positions_and_handle_collisions(dt);
    }

    // Update positions and handle collisions
    fn update_positions_and_handle_collisions(&mut self, dt: f32) {
        // First update positions
        for entity in self.entities.values_mut() {
            entity.position.0 += entity.velocity.0 * dt;
            entity.position.1 += entity.velocity.1 * dt;
            
            // Boundary constraints
            entity.position.0 = entity.position.0.max(entity.radius).min(self.width - entity.radius);
            entity.position.1 = entity.position.1.max(entity.radius).min(self.height - entity.radius);
        }

        // Handle collisions between entities (simple bounce)
        let entity_ids: Vec<u32> = self.entities.keys().cloned().collect();
        let entity_count = entity_ids.len();
        
        for i in 0..entity_count {
            for j in (i+1)..entity_count {
                let id1 = entity_ids[i];
                let id2 = entity_ids[j];
                
                // Skip if either entity was removed
                if !self.entities.contains_key(&id1) || !self.entities.contains_key(&id2) {
                    continue;
                }
                
                // Get positions and radii
                let (pos1, r1) = {
                    let e = self.entities.get(&id1).unwrap();
                    (e.position, e.radius)
                };
                
                let (pos2, r2) = {
                    let e = self.entities.get(&id2).unwrap();
                    (e.position, e.radius)
                };
                
                // Calculate distance between entities
                let dx = pos2.0 - pos1.0;
                let dy = pos2.1 - pos1.1;
                let distance = (dx * dx + dy * dy).sqrt();
                
                // Check for collision
                if distance < r1 + r2 {
                    // Calculate unit vector between entities
                    let nx = dx / distance;
                    let ny = dy / distance;
                    
                    // Calculate overlap
                    let overlap = r1 + r2 - distance;
                    
                    // Resolve overlap
                    {
                        let e1 = self.entities.get_mut(&id1).unwrap();
                        e1.position.0 -= nx * overlap * 0.5;
                        e1.position.1 -= ny * overlap * 0.5;
                        
                        // Reflect velocity (simple bounce)
                        let dot = e1.velocity.0 * nx + e1.velocity.1 * ny;
                        e1.velocity.0 -= 2.0 * dot * nx;
                        e1.velocity.1 -= 2.0 * dot * ny;
                    }
                    
                    {
                        let e2 = self.entities.get_mut(&id2).unwrap();
                        e2.position.0 += nx * overlap * 0.5;
                        e2.position.1 += ny * overlap * 0.5;
                        
                        // Reflect velocity (simple bounce)
                        let dot = e2.velocity.0 * nx + e2.velocity.1 * ny;
                        e2.velocity.0 -= 2.0 * dot * nx;
                        e2.velocity.1 -= 2.0 * dot * ny;
                    }
                }
            }
        }
    }

    // Get the current timestamp
    fn get_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis() as u64
    }

    // Broadcast game state to all clients
    async fn broadcast_game_state(&mut self) {
        let timestamp = Self::get_timestamp();
        let entities: Vec<Entity> = self.entities.values().cloned().collect();
        
        // Create a map of client_id -> last_processed_input
        let mut last_processed_inputs = HashMap::new();
        for (client_id, client) in &self.clients {
            last_processed_inputs.insert(*client_id, client.last_processed_input);
        }
        
        // Create the game state packet
        let packet = Packet::GameState {
            timestamp,
            last_processed_input: last_processed_inputs,
            entities,
        };
        
        // Serialize to JSON
        let json = serde_json::to_string(&packet).unwrap_or_else(|e| {
            error!("Failed to serialize game state: {}", e);
            "{}".to_string()
        });
        
        // Send to all clients
        let mut disconnected_clients = Vec::new();
        
        for (client_id, client) in &mut self.clients {
            match client.sender.send(Message::Text(json.clone())).await {
                Ok(_) => {},
                Err(e) => {
                    warn!("Failed to send game state to client {}: {}", client_id, e);
                    disconnected_clients.push(*client_id);
                }
            }
        }
        
        // Remove disconnected clients
        for client_id in disconnected_clients {
            self.remove_client(&client_id);
        }
    }
}

// Handle WebSocket connection
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    event_tx: mpsc::Sender<NetworkEvent>,
) {
    info!("New WebSocket connection from {}", addr);
    
    // Accept WebSocket connection
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            error!("Error during WebSocket handshake: {}", e);
            return;
        }
    };
    
    info!("WebSocket connection established: {}", addr);
    
    // Split WebSocket stream
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    
    // Create channel for sending messages to client
    let (client_tx, mut client_rx) = mpsc::channel(100);
    
    // Assign a client ID (this will be set properly when client connects)
    let client_id = rand::random::<u32>() % 1000 + 1;
    
    // Send the connection event to the game loop
    match event_tx.send(NetworkEvent::NewConnection { 
        client_id, 
        sender: client_tx.clone() 
    }).await {
        Ok(_) => {},
        Err(e) => {
            error!("Failed to send connection event: {}", e);
            return;
        }
    }
    
    // Task for forwarding messages from game to client
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = client_rx.recv().await {
            if let Err(e) = ws_sender.send(msg).await {
                error!("Error sending WebSocket message: {}", e);
                break;
            }
        }
    });
    
    // Main receive loop for client messages
    while let Some(result) = ws_receiver.next().await {
        match result {
            Ok(msg) => {
                match msg {
                    Message::Text(text) => {
                        // Parse JSON message
                        match serde_json::from_str::<Packet>(&text) {
                            Ok(packet) => {
                                process_packet(packet, client_id, &event_tx).await;
                            },
                            Err(e) => {
                                warn!("Failed to parse JSON from {}: {}", addr, e);
                            }
                        }
                    },
                    Message::Binary(data) => {
                        // Parse binary message
                        match bincode::deserialize::<Packet>(&data) {
                            Ok(packet) => {
                                process_packet(packet, client_id, &event_tx).await;
                            },
                            Err(e) => {
                                warn!("Failed to deserialize binary data from {}: {}", addr, e);
                            }
                        }
                    },
                    Message::Close(_) => {
                        info!("WebSocket closed by client: {}", addr);
                        break;
                    },
                    _ => {} // Ignore other message types (ping/pong, etc.)
                }
            },
            Err(e) => {
                warn!("WebSocket error from {}: {}", addr, e);
                break;
            }
        }
    }
    
    // Handle disconnection
    info!("WebSocket client {} disconnected", addr);
    if let Err(e) = event_tx.send(NetworkEvent::ClientDisconnect { client_id }).await {
        error!("Failed to send disconnect event: {}", e);
    }
    
    // Cancel sender task
    sender_task.abort();
}

// Process a received packet
async fn process_packet(
    packet: Packet,
    client_id: u32,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    match packet {
        Packet::ConnectionRequest { client_version } => {
            debug!("Connection request from client {} (version: {})", client_id, client_version);
            
            // Connection is already established at this point, so we can just acknowledge
            // The actual client creation happens in the game loop
        },
        
        Packet::PlayerInput { sequence, timestamp, input_vector } => {
            debug!("PlayerInput from client {}: seq {}, ts {}, vec {:?}", 
                   client_id, sequence, timestamp, input_vector);
            
            // Forward input to game loop
            if let Err(e) = event_tx.send(NetworkEvent::PlayerInput {
                client_id,
                sequence,
                timestamp,
                input_vector,
            }).await {
                error!("Failed to send PlayerInput to game thread: {}", e);
            }
        },
        
        Packet::Heartbeat { timestamp } => {
            debug!("Heartbeat from client {}: ts {}", client_id, timestamp);
            // Heartbeats are handled automatically by WebSockets
        },
        
        Packet::Disconnect { reason } => {
            info!("Disconnect request from client {}: {}", client_id, reason);
            
            // Forward disconnect to game loop
            if let Err(e) = event_tx.send(NetworkEvent::ClientDisconnect { client_id }).await {
                error!("Failed to send ClientDisconnect to game thread: {}", e);
            }
        },
        
        Packet::ConnectionAccepted { .. } | Packet::GameState { .. } => {
            warn!("Received server-sent type packet from client {}", client_id);
        }
    }
}

// Game loop
async fn run_game_loop(
    game_state: Arc<Mutex<GameState>>,
    mut network_rx: mpsc::Receiver<NetworkEvent>,
    tick_interval: Duration,
) {
    let mut last_tick = Instant::now();
    let mut tick_count = 0u64;
    
    loop {
        // Process network events
        while let Ok(event) = network_rx.try_recv() {
            match event {
                NetworkEvent::NewConnection { client_id, sender } => {
                    debug!("Game thread: New connection from client {}", client_id);
                    
                    let mut state = game_state.lock().await;
                    // Generate a fake address since we don't have real socket addresses with WebSockets
                    let fake_addr = format!("ws-client-{}", client_id).parse::<SocketAddr>().unwrap_or_else(|_| {
                        "127.0.0.1:0".parse().unwrap()
                    });
                    
                    // Add the client to game state
                    state.add_client(client_id, fake_addr, sender.clone());
                    
                    // Send connection accepted message
                    let response = Packet::ConnectionAccepted { client_id };
                    let json = serde_json::to_string(&response).unwrap();
                    
                    // Ignore send errors - client might have disconnected
                    let _ = sender.send(Message::Text(json)).await;
                },
                
                NetworkEvent::ClientDisconnect { client_id } => {
                    debug!("Game thread: Client disconnected: {}", client_id);
                    
                    let mut state = game_state.lock().await;
                    state.remove_client(&client_id);
                },
                
                NetworkEvent::PlayerInput { client_id, sequence, timestamp, input_vector } => {
                    let mut state = game_state.lock().await;
                    let input = InputState {
                        client_id,
                        sequence,
                        timestamp,
                        input_vector,
                    };
                    
                    if let Err(e) = state.update_client_input(&client_id, input) {
                        warn!("Error updating client input: {}", e);
                    }
                }
            }
        }
        
        // Calculate delta time
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;
        
        // Cap delta time to avoid huge jumps
        let capped_dt = dt.min(0.1);
        
        // Update game state
        {
            let mut state = game_state.lock().await;
            state.update(capped_dt);
            
            // Send game state to all clients
            state.broadcast_game_state().await;
            
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

// Check for client timeouts
async fn check_client_timeouts(state: &mut GameState) {
    let timeout = Duration::from_secs(5);
    let now = Instant::now();
    
    // Collect IDs of timed-out clients
    let timed_out: Vec<u32> = state.clients
        .iter()
        .filter(|(_, client)| now.duration_since(client.last_seen) > timeout)
        .map(|(id, _)| *id)
        .collect();
    
    // Remove timed-out clients
    for id in timed_out {
        info!("Client {} timed out", id);
        state.remove_client(&id);
    }
}

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
    let ws_addr = format!("{}:{}", args.host, args.port);
    let tick_interval = Duration::from_secs_f32(1.0 / args.tick_rate as f32);

    info!("Starting game server on WebSocket: {}", ws_addr);
    info!("Tick rate: {} Hz ({:?} per tick)", args.tick_rate, tick_interval);

    // Create WebSocket listener
    let listener = TcpListener::bind(&ws_addr).await?;
    info!("WebSocket server listening on {}", ws_addr);

    // Create shared game state (800x600 game world)
    let game_state = Arc::new(Mutex::new(GameState::new(800.0, 600.0)));

    // Channel for communication between network and game threads
    let (event_tx, event_rx) = mpsc::channel::<NetworkEvent>(100);

    // Spawn game loop task
    let game_state_clone = game_state.clone();
    tokio::spawn(async move {
        run_game_loop(game_state_clone, event_rx, tick_interval).await;
    });

    // Spawn task to check for timeouts
    let game_state_timeouts = game_state.clone();
    tokio::spawn(async move {
        let check_interval = Duration::from_secs(1);
        loop {
            sleep(check_interval).await;
            let mut state = game_state_timeouts.lock().await;
            check_client_timeouts(&mut state).await;
        }
    });

    // Main WebSocket listener loop
    info!("WebSocket server started");
    while let Ok((stream, addr)) = listener.accept().await {
        let event_tx_clone = event_tx.clone();
        
        tokio::spawn(async move {
            handle_connection(stream, addr, event_tx_clone).await;
        });
    }

    Ok(())
}