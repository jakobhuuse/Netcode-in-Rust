//! Client connection management and input queuing

use log::info;
use shared::InputState;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Connected client with input state
#[derive(Debug)]
pub struct Client {
    pub id: u32,
    pub addr: SocketAddr,
    pub last_seen: Instant,
    pub last_processed_input: u32,
    pub pending_inputs: Vec<InputState>,
}

impl Client {
    pub fn new(id: u32, addr: SocketAddr) -> Self {
        Self {
            id,
            addr,
            last_seen: Instant::now(),
            last_processed_input: 0,
            pending_inputs: Vec::new(),
        }
    }

    /// Adds input and sorts by sequence to handle out-of-order packets
    pub fn add_input(&mut self, input: InputState) {
        self.last_seen = Instant::now();
        self.pending_inputs.push(input);
        self.pending_inputs.sort_by_key(|i| i.sequence);
    }

    pub fn is_timed_out(&self, timeout: Duration) -> bool {
        self.last_seen.elapsed() > timeout
    }
}

/// Manages all connected clients and their input processing
pub struct ClientManager {
    clients: HashMap<u32, Client>,
    next_client_id: u32,
    max_clients: usize,
}

impl ClientManager {
    pub fn new(max_clients: usize) -> Self {
        Self {
            clients: HashMap::new(),
            next_client_id: 1,
            max_clients,
        }
    }

    /// Attempts to add a new client, returns client ID if successful
    pub fn add_client(&mut self, addr: SocketAddr) -> Option<u32> {
        if self.clients.len() >= self.max_clients {
            return None;
        }

        let client_id = self.next_client_id;
        self.next_client_id += 1;

        let client = Client::new(client_id, addr);
        info!("Client {} connected from {}", client_id, addr);
        self.clients.insert(client_id, client);

        Some(client_id)
    }

    pub fn remove_client(&mut self, client_id: &u32) -> bool {
        if let Some(client) = self.clients.remove(client_id) {
            info!("Client {} disconnected", client.id);
            true
        } else {
            false
        }
    }

    pub fn find_client_by_addr(&self, addr: SocketAddr) -> Option<u32> {
        self.clients
            .iter()
            .find(|(_, client)| client.addr == addr)
            .map(|(id, _)| *id)
    }

    pub fn add_input(&mut self, client_id: u32, input: InputState) -> bool {
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.add_input(input);
            true
        } else {
            false
        }
    }

    /// Gets all unprocessed inputs sorted chronologically for deterministic processing
    pub fn get_chronological_inputs(&self) -> Vec<(u32, InputState)> {
        let mut all_inputs: Vec<(u32, InputState)> = Vec::new();

        for (client_id, client) in &self.clients {
            for input in &client.pending_inputs {
                if input.sequence > client.last_processed_input {
                    all_inputs.push((*client_id, input.clone()));
                }
            }
        }

        // Sort by timestamp for deterministic processing order
        all_inputs.sort_by_key(|(_, input)| input.timestamp);
        all_inputs
    }

    pub fn mark_input_processed(&mut self, client_id: u32, sequence: u32) {
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.last_processed_input = client.last_processed_input.max(sequence);
        }
    }

    /// Removes processed inputs to prevent memory growth
    pub fn cleanup_processed_inputs(&mut self) {
        for client in self.clients.values_mut() {
            client
                .pending_inputs
                .retain(|input| input.sequence > client.last_processed_input);
        }
    }

    /// Returns last processed input sequence for each client (used for reconciliation)
    pub fn get_last_processed_inputs(&self) -> HashMap<u32, u32> {
        self.clients
            .iter()
            .map(|(id, client)| (*id, client.last_processed_input))
            .collect()
    }

    /// Checks for and removes timed-out clients
    pub fn check_timeouts(&mut self) -> Vec<u32> {
        let timeout = Duration::from_secs(5);
        let timed_out: Vec<u32> = self
            .clients
            .iter()
            .filter(|(_, client)| client.is_timed_out(timeout))
            .map(|(id, _)| *id)
            .collect();

        for client_id in &timed_out {
            self.remove_client(client_id);
        }

        timed_out
    }

    pub fn get_client_addrs(&self) -> Vec<(u32, SocketAddr)> {
        self.clients
            .iter()
            .map(|(id, client)| (*id, client.addr))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.clients.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_addr() -> SocketAddr {
        "127.0.0.1:8080".parse().unwrap()
    }

    #[test]
    fn test_client_creation() {
        let addr = test_addr();
        let client = Client::new(1, addr);
        assert_eq!(client.id, 1);
        assert_eq!(client.addr, addr);
        assert_eq!(client.last_processed_input, 0);
        assert!(client.pending_inputs.is_empty());
    }

    #[test]
    fn test_add_client() {
        let mut manager = ClientManager::new(2);
        let addr = test_addr();

        let client_id = manager.add_client(addr).unwrap();
        assert_eq!(client_id, 1);
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn test_client_capacity() {
        let mut manager = ClientManager::new(1);
        let addr1 = "127.0.0.1:8080".parse().unwrap();
        let addr2 = "127.0.0.1:8081".parse().unwrap();

        assert!(manager.add_client(addr1).is_some());
        assert!(manager.add_client(addr2).is_none()); // Should be full
    }

    #[test]
    fn test_chronological_inputs() {
        let mut manager = ClientManager::new(2);
        let addr1 = "127.0.0.1:8080".parse().unwrap();
        let addr2 = "127.0.0.1:8081".parse().unwrap();

        let client_id1 = manager.add_client(addr1).unwrap();
        let client_id2 = manager.add_client(addr2).unwrap();

        let input1 = InputState {
            sequence: 1,
            timestamp: 100,
            left: true,
            right: false,
            jump: false,
        };

        let input2 = InputState {
            sequence: 1,
            timestamp: 50, // Earlier timestamp
            left: false,
            right: true,
            jump: false,
        };

        manager.add_input(client_id1, input1);
        manager.add_input(client_id2, input2);

        let chronological = manager.get_chronological_inputs();
        assert_eq!(chronological.len(), 2);
        // Should be sorted by timestamp (50, then 100)
        assert_eq!(chronological[0].1.timestamp, 50);
        assert_eq!(chronological[1].1.timestamp, 100);
    }
}
