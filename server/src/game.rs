use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use log::{info, warn, error};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;

use crate::entity::{Entity, EntityType};
use crate::client::Client;
use crate::packets::{InputState, Packet, NetworkEvent};
use crate::utils;

// Game state
pub struct GameState {
    pub clients: HashMap<u32, Client>,
    pub entities: HashMap<u32, Entity>,
    pub next_client_id: u32,
    pub next_entity_id: u32,
    // Map world boundaries
    pub width: f32,
    pub height: f32,
}

impl GameState {
    pub fn new(width: f32, height: f32) -> Self {
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
    pub fn add_client(&mut self, client_id: u32, addr: SocketAddr, sender: mpsc::Sender<Message>) -> u32 {
        // Create entity for this client
        let entity_id = self.next_entity_id;
        self.next_entity_id += 1;

        // Randomize starting position (away from edges)
        let position = (
            50.0 + (client_id as f32 * 50.0) % (self.width - 100.0),
            50.0 + (client_id as f32 * 30.0) % (self.height - 100.0),
        );

        // Generate a color based on client ID
        let color = utils::generate_color(client_id);

        // Create player entity
        let entity = Entity::new(
            entity_id,
            EntityType::Player,
            position,
            20.0,
            color
        );
        
        self.entities.insert(entity_id, entity);

        // Create client record
        let client = Client::new(client_id, addr, entity_id, sender);
        self.clients.insert(client_id, client);

        info!("Client {} connected from {}", client_id, addr);
        client_id
    }

    // Remove a client and their entity
    pub fn remove_client(&mut self, client_id: &u32) {
        if let Some(client) = self.clients.remove(client_id) {
            info!("Client {} disconnected", client.id);
            self.entities.remove(&client.entity_id);
        }
    }

    // Update client's input state
    pub fn update_client_input(&mut self, client_id: &u32, input: InputState) -> Result<(), String> {
        if let Some(client) = self.clients.get_mut(client_id) {
            client.add_input(input);
            Ok(())
        } else {
            Err(format!("Client {} not found", client_id))
        }
    }

    // Process inputs for all clients
    fn process_client_inputs(&mut self) {
        for client in self.clients.values_mut() {
            let last_input = client.process_inputs();
            
            // Get the client's entity
            if let Some(entity) = self.entities.get_mut(&client.entity_id) {
                // Process the most recent input from buffer
                if !client.input_buffer.is_empty() {
                    let input = client.input_buffer.last().unwrap();
                    
                    // Update velocity based on input
                    // Units per second
                    let speed = 200.0;
                    let input_vec = input.input_vector;
                    
                    // Normalize input vector
                    let (input_x, input_y) = utils::normalize_vector(input_vec.0, input_vec.1);
                    
                    entity.velocity = (input_x * speed, input_y * speed);
                }
            }
        }
    }

    // Update game physics 
    pub fn update(&mut self, dt: f32) {
        // Process inputs for each client
        self.process_client_inputs();
        
        // Update positions
        for entity in self.entities.values_mut() {
            entity.update_position(dt, self.width, self.height);
        }
        
        // Handle collisions between entities
        self.handle_collisions();
    }

    // Handle collisions between entities
    fn handle_collisions(&mut self) {
    let entity_ids: Vec<u32> = self.entities.keys().cloned().collect();
    let entity_count = entity_ids.len();
    
    // Store collision resolutions to apply later
    let mut collision_updates: Vec<(u32, Entity)> = Vec::new();
    
    for i in 0..entity_count {
        for j in (i+1)..entity_count {
            let id1 = entity_ids[i];
            let id2 = entity_ids[j];
            
            // Skip if either entity was removed
            if !self.entities.contains_key(&id1) || !self.entities.contains_key(&id2) {
                continue;
            }
            
            // Check for collision using immutable references
            let collision = {
                let e1 = self.entities.get(&id1).unwrap();
                let e2 = self.entities.get(&id2).unwrap();
                e1.check_collision(e2)
            };
            
            if collision {
                // Get copies of both entities
                if let (Some(e1), Some(e2)) = (self.entities.get(&id1), self.entities.get(&id2)) {
                    let mut e1_clone = e1.clone();
                    let mut e2_clone = e2.clone();
                    
                    // Resolve collision on the clones
                    e1_clone.resolve_collision(&mut e2_clone);
                    
                    // Store updates to apply after all collision checks
                    collision_updates.push((id1, e1_clone));
                    collision_updates.push((id2, e2_clone));
                }
            }
        }
    }
    
    // Apply all collision updates after collision detection is complete
    for (id, updated_entity) in collision_updates {
        if let Some(entity) = self.entities.get_mut(&id) {
            *entity = updated_entity;
        }
    }
}

    // Broadcast game state to all clients
    pub async fn broadcast_game_state(&mut self) {
        let timestamp = utils::get_timestamp();
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
    
    // Check for client timeouts
    pub fn check_timeouts(&mut self, timeout: Duration) -> Vec<u32> {
        let mut timed_out = Vec::new();
        
        for (client_id, client) in &self.clients {
            if client.is_timed_out(timeout) {
                timed_out.push(*client_id);
            }
        }
        
        timed_out
    }
}

// Game loop
pub async fn run_game_loop(
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
                    log::debug!("Game thread: New connection from client {}", client_id);
                    
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
                    log::debug!("Game thread: Client disconnected: {}", client_id);
                    
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
        tokio::time::sleep(tick_interval).await;
    }
}

// Check for client timeouts task
pub async fn timeout_checker(
    game_state: Arc<Mutex<GameState>>,
    check_interval: Duration,
    timeout_duration: Duration,
) {
    loop {
        tokio::time::sleep(check_interval).await;
        
        let mut state = game_state.lock().await;
        let timed_out = state.check_timeouts(timeout_duration);
        
        for client_id in timed_out {
            info!("Client {} timed out", client_id);
            state.remove_client(&client_id);
        }
    }
}