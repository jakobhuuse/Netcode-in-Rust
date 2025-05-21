use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::entity::Entity;

// Packet types for client-server communication
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Packet {
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

// Input state from client
#[derive(Debug, Clone)]
pub struct InputState {
    pub client_id: u32,
    pub sequence: u32,
    pub timestamp: u64,
    pub input_vector: (f32, f32),
}

// Events from network thread to game thread
pub enum NetworkEvent {
    NewConnection { client_id: u32, sender: tokio::sync::mpsc::Sender<tokio_tungstenite::tungstenite::Message> },
    ClientDisconnect { client_id: u32 },
    PlayerInput { 
        client_id: u32,
        sequence: u32,
        timestamp: u64,
        input_vector: (f32, f32),
    },
}