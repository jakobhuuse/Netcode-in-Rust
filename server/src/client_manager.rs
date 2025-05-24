//! Client connection management and input queuing for the multiplayer server
//!
//! This module handles the server-side management of connected clients, including:
//! - Client connection lifecycle (connect, disconnect, timeout)
//! - Input buffering and chronological ordering for deterministic simulation
//! - Connection health monitoring and automatic cleanup
//! - Client capacity management and address tracking
//!
//! The client manager ensures reliable input processing and maintains
//! authoritative control over which clients are allowed to participate.

use log::info;
use shared::InputState;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Represents a connected client and their input state
///
/// Each client maintains:
/// - Connection metadata (ID, address, last activity)
/// - Input acknowledgment tracking for reliable delivery
/// - Buffered inputs waiting for processing in chronological order
#[derive(Debug)]
pub struct Client {
    /// Unique client identifier assigned by the server
    pub id: u32,
    /// Network address for sending responses
    pub addr: SocketAddr,
    /// Last time we received any packet from this client
    pub last_seen: Instant,
    /// Highest input sequence number we've processed
    pub last_processed_input: u32,
    /// Buffered inputs waiting to be processed
    pub pending_inputs: Vec<InputState>,
}

impl Client {
    /// Creates a new client with the given ID and network address
    ///
    /// Initializes the client with default values and marks them as
    /// recently active. The client starts with no processed inputs
    /// and an empty input buffer.
    pub fn new(id: u32, addr: SocketAddr) -> Self {
        Self {
            id,
            addr,
            last_seen: Instant::now(),
            last_processed_input: 0,
            pending_inputs: Vec::new(),
        }
    }

    /// Adds a new input to the client's pending queue
    ///
    /// Updates the client's last seen time and inserts the input into
    /// the buffer in sequence order. This ensures inputs are processed
    /// in the correct chronological order even if packets arrive out of order.
    pub fn add_input(&mut self, input: InputState) {
        self.last_seen = Instant::now();
        self.pending_inputs.push(input);
        // Sort by sequence to handle out-of-order packet delivery
        self.pending_inputs.sort_by_key(|i| i.sequence);
    }

    /// Checks if the client has exceeded the connection timeout
    ///
    /// Returns true if no packets have been received from this client
    /// within the specified timeout duration, indicating a likely disconnect.
    pub fn is_timed_out(&self, timeout: Duration) -> bool {
        self.last_seen.elapsed() > timeout
    }
}

/// Manages all connected clients and their input processing
///
/// The ClientManager provides centralized control over client connections,
/// enforces server capacity limits, and ensures deterministic input processing
/// by maintaining chronological order across all clients. This is crucial for
/// authoritative server simulation in multiplayer games.
pub struct ClientManager {
    /// Connected clients indexed by their unique ID
    clients: HashMap<u32, Client>,
    /// Next available client ID for new connections
    next_client_id: u32,
    /// Maximum number of concurrent clients allowed
    max_clients: usize,
}

impl ClientManager {
    /// Creates a new client manager with the specified capacity limit
    ///
    /// Initializes an empty client roster with the given maximum client limit.
    /// Client IDs start from 1 and increment for each new connection.
    pub fn new(max_clients: usize) -> Self {
        Self {
            clients: HashMap::new(),
            next_client_id: 1,
            max_clients,
        }
    }

    /// Attempts to add a new client connection
    ///
    /// Returns Some(client_id) if successful, None if server is at capacity.
    /// Each client gets a unique ID and is associated with their network address
    /// for response routing. Logs the new connection for server monitoring.
    pub fn add_client(&mut self, addr: SocketAddr) -> Option<u32> {
        // Enforce server capacity limits
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

    /// Removes a client from the server
    ///
    /// Cleans up client state and logs the disconnection. Returns true if
    /// the client was found and removed, false if they were already gone.
    /// This handles both explicit disconnections and timeout cleanup.
    pub fn remove_client(&mut self, client_id: &u32) -> bool {
        if let Some(client) = self.clients.remove(client_id) {
            info!("Client {} disconnected", client.id);
            true
        } else {
            false
        }
    }

    /// Finds a client ID by their network address
    ///
    /// Used to associate incoming packets with existing client connections.
    /// Returns None if no client is connected from the given address.
    pub fn find_client_by_addr(&self, addr: SocketAddr) -> Option<u32> {
        self.clients
            .iter()
            .find(|(_, client)| client.addr == addr)
            .map(|(id, _)| *id)
    }

    /// Adds an input to a specific client's pending queue
    ///
    /// Updates the client's activity timestamp and buffers the input for
    /// processing. Returns false if the client ID is invalid.
    pub fn add_input(&mut self, client_id: u32, input: InputState) -> bool {
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.add_input(input);
            true
        } else {
            false
        }
    }

    /// Gets all unprocessed inputs sorted chronologically
    ///
    /// Collects inputs from all clients that haven't been processed yet,
    /// sorts them by timestamp to ensure deterministic processing order.
    /// This is critical for maintaining consistent game state across
    /// server simulation and client prediction/reconciliation.
    pub fn get_chronological_inputs(&self) -> Vec<(u32, InputState)> {
        let mut all_inputs: Vec<(u32, InputState)> = Vec::new();

        // Collect unprocessed inputs from all clients
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

    /// Marks an input sequence as processed for a specific client
    ///
    /// Updates the client's last processed input sequence number to support
    /// client-side reconciliation. Clients can use this information to clean
    /// up their prediction history and detect when rollback is needed.
    pub fn mark_input_processed(&mut self, client_id: u32, sequence: u32) {
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.last_processed_input = client.last_processed_input.max(sequence);
        }
    }

    /// Removes inputs that have been processed from all client buffers
    ///
    /// Cleans up memory by removing inputs that have already been applied
    /// to the game state. This prevents unbounded memory growth while
    /// maintaining inputs needed for reconciliation.
    pub fn cleanup_processed_inputs(&mut self) {
        for client in self.clients.values_mut() {
            client
                .pending_inputs
                .retain(|input| input.sequence > client.last_processed_input);
        }
    }

    /// Gets the last processed input sequence for each client
    ///
    /// Returns a map of client IDs to their highest processed input sequence.
    /// This information is sent to clients to enable reconciliation by
    /// letting them know which inputs have been authoritatively processed.
    pub fn get_last_processed_inputs(&self) -> HashMap<u32, u32> {
        self.clients
            .iter()
            .map(|(id, client)| (*id, client.last_processed_input))
            .collect()
    }

    /// Checks for and removes timed-out clients
    ///
    /// Automatically disconnects clients that haven't sent packets within
    /// the timeout threshold. Returns a list of removed client IDs for
    /// cleanup in other game systems. This ensures server resources are
    /// freed when clients disconnect unexpectedly.
    pub fn check_timeouts(&mut self) -> Vec<u32> {
        let timeout = Duration::from_secs(5);
        let timed_out: Vec<u32> = self
            .clients
            .iter()
            .filter(|(_, client)| client.is_timed_out(timeout))
            .map(|(id, _)| *id)
            .collect();

        // Remove timed-out clients
        for client_id in &timed_out {
            self.remove_client(client_id);
        }

        timed_out
    }

    /// Gets all client IDs and their network addresses
    ///
    /// Used for broadcasting game state updates to all connected clients.
    /// Returns a vector of (client_id, address) pairs for efficient
    /// packet distribution during the server's main game loop.
    pub fn get_client_addrs(&self) -> Vec<(u32, SocketAddr)> {
        self.clients
            .iter()
            .map(|(id, client)| (*id, client.addr))
            .collect()
    }

    /// Returns the number of currently connected clients
    pub fn len(&self) -> usize {
        self.clients.len()
    }

    /// Returns true if no clients are currently connected
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
}

/// Comprehensive test suite for client manager functionality
///
/// Tests cover client lifecycle management, input processing, timeout handling,
/// and capacity enforcement. These tests ensure reliable multiplayer server
/// operation and proper client state management.
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_addr() -> SocketAddr {
        "127.0.0.1:8080".parse().unwrap()
    }

    fn test_addr2() -> SocketAddr {
        "127.0.0.1:8081".parse().unwrap()
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
    fn test_client_add_input() {
        let addr = test_addr();
        let mut client = Client::new(1, addr);

        let input1 = InputState {
            sequence: 2,
            timestamp: 100,
            left: true,
            right: false,
            jump: false,
        };

        let input2 = InputState {
            sequence: 1,
            timestamp: 50,
            left: false,
            right: true,
            jump: false,
        };

        client.add_input(input1);
        client.add_input(input2);

        assert_eq!(client.pending_inputs.len(), 2);
        assert_eq!(client.pending_inputs[0].sequence, 1);
        assert_eq!(client.pending_inputs[1].sequence, 2);
    }

    #[test]
    fn test_client_timeout() {
        let addr = test_addr();
        let mut client = Client::new(1, addr);

        assert!(!client.is_timed_out(Duration::from_secs(1)));

        client.last_seen = std::time::Instant::now() - Duration::from_secs(2);

        assert!(client.is_timed_out(Duration::from_secs(1)));
    }

    #[test]
    fn test_client_manager_creation() {
        let manager = ClientManager::new(5);
        assert_eq!(manager.max_clients, 5);
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_add_client() {
        let mut manager = ClientManager::new(2);
        let addr = test_addr();

        let client_id = manager.add_client(addr).unwrap();
        assert_eq!(client_id, 1);
        assert_eq!(manager.len(), 1);
        assert!(!manager.is_empty());
    }

    #[test]
    fn test_add_multiple_clients() {
        let mut manager = ClientManager::new(3);
        let addr1 = test_addr();
        let addr2 = test_addr2();

        let client_id1 = manager.add_client(addr1).unwrap();
        let client_id2 = manager.add_client(addr2).unwrap();

        assert_eq!(client_id1, 1);
        assert_eq!(client_id2, 2);
        assert_eq!(manager.len(), 2);
    }

    #[test]
    fn test_add_client_max_capacity() {
        let mut manager = ClientManager::new(1);
        let addr1 = test_addr();
        let addr2 = test_addr2();

        let client_id1 = manager.add_client(addr1);
        assert!(client_id1.is_some());
        assert_eq!(manager.len(), 1);

        let client_id2 = manager.add_client(addr2);
        assert!(client_id2.is_none());
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn test_remove_client() {
        let mut manager = ClientManager::new(2);
        let addr = test_addr();

        let client_id = manager.add_client(addr).unwrap();
        assert_eq!(manager.len(), 1);

        let removed = manager.remove_client(&client_id);
        assert!(removed);
        assert_eq!(manager.len(), 0);
        assert!(manager.is_empty());
    }

    #[test]
    fn test_remove_nonexistent_client() {
        let mut manager = ClientManager::new(2);

        let removed = manager.remove_client(&999);
        assert!(!removed);
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_find_client_by_addr() {
        let mut manager = ClientManager::new(2);
        let addr1 = test_addr();
        let addr2 = test_addr2();

        let client_id1 = manager.add_client(addr1).unwrap();
        let _client_id2 = manager.add_client(addr2).unwrap();

        let found_id = manager.find_client_by_addr(addr1);
        assert_eq!(found_id, Some(client_id1));

        let unknown_addr: SocketAddr = "192.168.1.1:9999".parse().unwrap();
        let not_found = manager.find_client_by_addr(unknown_addr);
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_add_input_to_client() {
        let mut manager = ClientManager::new(2);
        let addr = test_addr();

        let client_id = manager.add_client(addr).unwrap();

        let input = InputState {
            sequence: 1,
            timestamp: 100,
            left: true,
            right: false,
            jump: false,
        };

        let success = manager.add_input(client_id, input);
        assert!(success);
    }

    #[test]
    fn test_add_input_to_nonexistent_client() {
        let mut manager = ClientManager::new(2);

        let input = InputState {
            sequence: 1,
            timestamp: 100,
            left: true,
            right: false,
            jump: false,
        };

        let success = manager.add_input(999, input);
        assert!(!success);
    }

    #[test]
    fn test_get_chronological_inputs() {
        let mut manager = ClientManager::new(3);
        let addr1 = test_addr();
        let addr2 = test_addr2();

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
            timestamp: 50,
            left: false,
            right: true,
            jump: false,
        };

        let input3 = InputState {
            sequence: 2,
            timestamp: 200,
            left: false,
            right: false,
            jump: true,
        };

        manager.add_input(client_id1, input1);
        manager.add_input(client_id2, input2);
        manager.add_input(client_id1, input3);

        let chronological_inputs = manager.get_chronological_inputs();

        assert_eq!(chronological_inputs.len(), 3);
        assert_eq!(chronological_inputs[0].1.timestamp, 50);
        assert_eq!(chronological_inputs[1].1.timestamp, 100);
        assert_eq!(chronological_inputs[2].1.timestamp, 200);
    }
}
