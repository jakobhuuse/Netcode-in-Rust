use std::net::SocketAddr;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::packets::InputState;

// Client representation
#[derive(Debug)]
pub struct Client {
    pub id: u32,
    pub addr: SocketAddr,
    pub last_seen: Instant,
    pub entity_id: u32,
    pub last_processed_input: u32,
    pub input_buffer: Vec<InputState>,
    pub sender: mpsc::Sender<Message>,
}

impl Client {
    pub fn new(
        id: u32, 
        addr: SocketAddr, 
        entity_id: u32,
        sender: mpsc::Sender<Message>
    ) -> Self {
        Client {
            id,
            addr,
            last_seen: Instant::now(),
            entity_id,
            last_processed_input: 0,
            input_buffer: Vec::new(),
            sender,
        }
    }
    
    // Update the client's last seen time
    pub fn refresh_last_seen(&mut self) {
        self.last_seen = Instant::now();
    }
    
    // Check if client has timed out
    pub fn is_timed_out(&self, timeout_duration: std::time::Duration) -> bool {
        Instant::now().duration_since(self.last_seen) > timeout_duration
    }
    
    // Add input to the client's input buffer
    pub fn add_input(&mut self, input: InputState) {
        self.refresh_last_seen();
        self.input_buffer.push(input);
    }
    
    // Process inputs and return the ones that were processed
    pub fn process_inputs(&mut self) -> u32 {
        if self.input_buffer.is_empty() {
            return self.last_processed_input;
        }
        
        // Sort inputs by sequence number
        self.input_buffer.sort_by_key(|input| input.sequence);
        
        // Get the highest sequence number
        let highest_seq = self.input_buffer.last().unwrap().sequence;
        
        // Update last processed input
        self.last_processed_input = highest_seq;
        
        // Clear input buffer
        self.input_buffer.clear();
        
        highest_seq
    }
}