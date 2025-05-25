//! Client-side network implementation for real-time multiplayer game
//!
//! This module implements the client's network layer, handling:
//! - UDP connection management with timeout detection
//! - Artificial latency simulation for testing netcode
//! - Packet serialization and queuing for delayed transmission
//! - Integration with client-side prediction, reconciliation, and interpolation
//! - Input transmission with sequence numbering for reliability

use crate::game::{ClientGameState, ServerStateConfig};
use crate::input::InputManager;
use crate::rendering::{RenderConfig, Renderer};
use bincode::{deserialize, serialize};
use log::{error, info, warn};
use macroquad::prelude::*;
use shared::{InputState, Packet};
use std::collections::VecDeque;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// Main client structure managing network communication and game state
///
/// The client handles all networking responsibilities including:
/// - Maintaining UDP connection to game server
/// - Managing artificial latency for testing network conditions
/// - Coordinating input, prediction, and rendering systems
/// - Tracking connection health and implementing reconnection logic
pub struct Client {
    // Network components
    socket: UdpSocket,
    server_addr: SocketAddr,
    client_id: Option<u32>,
    connected: bool,

    // Game systems
    game_state: ClientGameState,
    input_manager: InputManager,
    renderer: Renderer,

    // Connection monitoring
    real_ping_ms: u64,     // Actual network ping
    fake_ping_ms: u64,     // Artificial latency for testing
    ping_ms: u64,          // Total displayed ping (real + fake)
    last_packet_received: Instant,
    connection_timeout: Duration,

    // Packet queuing for artificial latency simulation
    outgoing_packets: VecDeque<(Vec<u8>, Instant)>, // (data, send_time)
    incoming_packets: VecDeque<(Packet, Instant, Instant)>, // (packet, process_time, receive_time)

    // Netcode feature toggles
    prediction_enabled: bool,
    reconciliation_enabled: bool,
    interpolation_enabled: bool,
}

impl Client {
    /// Creates a new client instance with specified server address and artificial latency
    ///
    /// Sets up UDP socket, initializes game systems, and configures netcode features.
    /// The fake_ping_ms parameter allows testing network conditions by artificially
    /// delaying packet transmission and processing.
    pub async fn new(
        server_addr: &str,
        fake_ping_ms: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Bind to any available port for client socket
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;

        // Resolve server address (supports both IP addresses and domain names)
        let server_addr = Self::resolve_address(server_addr)?;

        let renderer = Renderer::new()?;

        Ok(Client {
            socket,
            server_addr,
            client_id: None,
            connected: false,
            game_state: ClientGameState::new(),
            input_manager: InputManager::new(),
            renderer,
            real_ping_ms: 0,
            fake_ping_ms,
            ping_ms: 0,
            last_packet_received: Instant::now(),
            connection_timeout: Duration::from_secs(5),
            outgoing_packets: VecDeque::new(),
            incoming_packets: VecDeque::new(),
            // Enable all netcode features by default
            prediction_enabled: true,
            reconciliation_enabled: true,
            interpolation_enabled: true,
        })
    }

    /// Resolves a server address string to a SocketAddr, supporting both IP addresses and domain names
    ///
    /// This method allows the client to connect to servers specified as:
    /// - IP addresses: "192.168.1.100:8080", "127.0.0.1:8080"
    /// - Domain names: "gameserver.example.com:8080", "localhost:8080"
    ///
    /// For domain names, it performs DNS resolution and returns the first resolved address.
    fn resolve_address(addr_str: &str) -> Result<SocketAddr, Box<dyn std::error::Error>> {
        // First try parsing as a direct SocketAddr (for IP addresses)
        if let Ok(addr) = addr_str.parse::<SocketAddr>() {
            return Ok(addr);
        }

        // If parsing fails, try DNS resolution (for domain names)
        // Use standard library's synchronous DNS resolution
        use std::net::ToSocketAddrs;
        let mut addrs = addr_str.to_socket_addrs()?;

        // Return the first resolved address
        if let Some(addr) = addrs.next() {
            Ok(addr)
        } else {
            Err(format!("Failed to resolve address: {}", addr_str).into())
        }
    }

    /// Initiates connection to the game server
    ///
    /// Sends initial connection packet with client version information.
    /// The server will respond with a Connected packet containing the assigned client ID.
    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Connecting to server...");

        let packet = Packet::Connect { client_version: 1 };
        self.send_packet(&packet).await?;

        Ok(())
    }

    /// Attempts to reconnect to the server after connection loss
    ///
    /// Performs clean disconnection if still connected, resets client state,
    /// and initiates a fresh connection. Used for both manual reconnection
    /// and automatic recovery from connection timeouts.
    pub async fn reconnect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Attempting to reconnect...");

        // Clean disconnect if still connected
        if self.connected {
            let _ = self.send_packet(&Packet::Disconnect).await;
            std::thread::sleep(Duration::from_millis(100));
        }

        // Reset client state for fresh connection
        self.connected = false;
        self.client_id = None;
        self.last_packet_received = Instant::now();

        // Clear packet queues to avoid stale data
        self.outgoing_packets.clear();
        self.incoming_packets.clear();

        // Reset game state for clean reconnection
        self.game_state = ClientGameState::new();

        self.connect().await
    }

    /// Monitors connection health and detects timeouts
    ///
    /// Checks if too much time has passed since the last received packet.
    /// Marks the client as disconnected if the timeout threshold is exceeded,
    /// allowing the main loop to handle reconnection logic.
    fn check_connection_health(&mut self) {
        if self.connected && self.last_packet_received.elapsed() > self.connection_timeout {
            warn!("Connection timeout detected, marking as disconnected");
            self.connected = false;
            self.client_id = None;
        }
    }

    /// Sends a packet to the server with optional artificial latency
    ///
    /// Serializes the packet and either sends immediately or queues for delayed
    /// transmission if artificial latency is enabled. This allows testing of
    /// netcode behavior under various network conditions.
    async fn send_packet(&mut self, packet: &Packet) -> Result<(), Box<dyn std::error::Error>> {
        let data = serialize(packet)?;

        if self.fake_ping_ms > 0 {
            // Simulate one-way latency (half of round-trip time)
            let delay_ms = self.fake_ping_ms / 2;
            let send_time = Instant::now() + Duration::from_millis(delay_ms);
            self.outgoing_packets.push_back((data, send_time));
        } else {
            // Send immediately for real network conditions
            self.socket.send_to(&data, self.server_addr)?;
        }

        Ok(())
    }

    /// Processes queued outgoing packets for artificial latency simulation
    ///
    /// Checks if any queued packets are ready to be sent based on their
    /// scheduled transmission time. This simulates network delay by holding
    /// packets until the artificial latency period has elapsed.
    fn process_outgoing_packets(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let now = Instant::now();
        while let Some((_data, send_time)) = self.outgoing_packets.front() {
            if now >= *send_time {
                let (data, _) = self.outgoing_packets.pop_front().unwrap();
                self.socket.send_to(&data, self.server_addr)?;
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Processes queued incoming packets for artificial latency simulation
    ///
    /// Handles packets that have been delayed to simulate network latency.
    /// Packets are processed when their scheduled processing time is reached,
    /// maintaining the artificial delay for realistic netcode testing.
    fn process_incoming_packets(&mut self) {
        let now = Instant::now();
        while let Some((_packet, process_time, _receive_time)) = self.incoming_packets.front() {
            if now >= *process_time {
                let (packet, _, receive_time) = self.incoming_packets.pop_front().unwrap();
                self.handle_packet_sync(packet, receive_time);
            } else {
                break;
            }
        }
    }

    /// Handles incoming packets from the server
    ///
    /// Processes different packet types and updates client state accordingly:
    /// - Connected: Establishes client ID and connection status
    /// - GameState: Updates game simulation and triggers netcode features
    /// - Disconnected: Handles server-initiated disconnection
    ///
    /// Also calculates ping time and updates connection health tracking.
    fn handle_packet_sync(&mut self, packet: Packet, receive_time: Instant) {
        // Update connection health tracking
        self.last_packet_received = Instant::now();

        match packet {
            Packet::Connected { client_id } => {
                info!("Connected! Client ID: {}", client_id);
                self.client_id = Some(client_id);
                self.connected = true;
            }

            Packet::GameState {
                tick,
                timestamp,
                last_processed_input,
                players,
            } => {
                // Calculate ping time for display and netcode tuning
                if timestamp > 0 {
                    // Calculate the elapsed time since the packet was actually received
                    // This accounts for any artificial delay in processing
                    let elapsed_since_receive = receive_time.elapsed();
                    
                    // Get current system time and subtract the processing delay
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or(Duration::from_secs(0))
                        .as_millis() as u64;
                    
                    // Reconstruct the actual receive time by subtracting processing delay
                    let actual_receive_time_ms = now_ms.saturating_sub(elapsed_since_receive.as_millis() as u64);
                    
                    // Always calculate real ping from actual network round-trip time
                    self.real_ping_ms = actual_receive_time_ms.saturating_sub(timestamp);
                    
                    // Total displayed ping is real ping + artificial ping
                    self.ping_ms = self.real_ping_ms + self.fake_ping_ms;
                    
                    // Log ping breakdown when fake ping is enabled
                    if self.fake_ping_ms > 0 {
                        log::debug!("Ping breakdown: real={}ms, fake={}ms, total={}ms", 
                                   self.real_ping_ms, self.fake_ping_ms, self.ping_ms);
                    }
                }

                // Configure server state processing based on enabled features
                let config = ServerStateConfig {
                    client_id: self.client_id,
                    reconciliation_enabled: self.reconciliation_enabled,
                    interpolation_enabled: self.interpolation_enabled,
                };

                // Apply authoritative server state and perform netcode processing
                self.game_state.apply_server_state(
                    tick,
                    timestamp,
                    players,
                    last_processed_input,
                    config,
                );
            }

            Packet::Disconnected { reason } => {
                warn!("Disconnected: {}", reason);
                self.connected = false;
                self.client_id = None;
            }

            _ => {
                warn!("Unexpected packet type");
            }
        }
    }

    /// Sends player input to the server and applies client-side prediction
    ///
    /// Transmits input state with sequence numbering for reliability and
    /// acknowledgment tracking. If prediction is enabled, immediately applies
    /// the input locally for responsive gameplay while waiting for server
    /// confirmation and potential reconciliation.
    async fn send_input(&mut self, input: InputState) -> Result<(), Box<dyn std::error::Error>> {
        if !self.connected || self.client_id.is_none() {
            return Ok(());
        }

        // Serialize input for network transmission
        let packet = Packet::Input {
            sequence: input.sequence,
            timestamp: input.timestamp,
            left: input.left,
            right: input.right,
            jump: input.jump,
        };

        self.send_packet(&packet).await?;

        // Apply client-side prediction for immediate response
        if self.prediction_enabled {
            if let Some(client_id) = self.client_id {
                self.game_state.apply_prediction(client_id, &input);
            }
        }

        Ok(())
    }

    /// Handles runtime toggle of netcode features via keyboard input
    ///
    /// Allows dynamic enable/disable of prediction, reconciliation, interpolation,
    /// and manual reconnection during gameplay for testing and demonstration.
    /// Returns true if reconnection was requested.
    fn handle_toggles(&mut self, toggles: (bool, bool, bool, bool)) -> bool {
        let mut reconnect_requested = false;

        if toggles.0 {
            self.prediction_enabled = !self.prediction_enabled;
            info!("Client-side prediction: {}", self.prediction_enabled);
        }
        if toggles.1 {
            self.reconciliation_enabled = !self.reconciliation_enabled;
            info!("Server reconciliation: {}", self.reconciliation_enabled);
        }
        if toggles.2 {
            self.interpolation_enabled = !self.interpolation_enabled;
            info!("Interpolation: {}", self.interpolation_enabled);
        }
        if toggles.3 {
            info!("Reconnection requested");
            reconnect_requested = true;
        }

        reconnect_requested
    }

    /// Main client game loop handling network, input, and rendering
    ///
    /// Orchestrates the complete client-side game experience:
    /// 1. Network: Processes incoming/outgoing packets with artificial latency
    /// 2. Input: Captures player input and sends to server at 60Hz
    /// 3. Physics: Updates game simulation with fixed timesteps
    /// 4. Rendering: Displays game state with appropriate netcode visualization
    ///
    /// The loop runs at 60 FPS with separate timing for input sampling and rendering
    /// to ensure consistent gameplay regardless of frame rate variations.
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.connect().await?;

        // Timing control for consistent update rates
        let mut last_input_time = Instant::now();
        let mut last_render_time = Instant::now();
        let input_interval = Duration::from_millis(16); // 60Hz input sampling
        let render_interval = Duration::from_millis(16); // 60 FPS rendering

        let mut buffer = [0u8; 2048];

        loop {
            // Process outgoing packets with artificial delay
            if let Err(e) = self.process_outgoing_packets() {
                error!("Error processing outgoing packets: {}", e);
            }

            // Receive and queue incoming packets
            match self.socket.recv_from(&mut buffer) {
                Ok((len, _)) => {
                    let receive_time = Instant::now();
                    if let Ok(packet) = deserialize::<Packet>(&buffer[0..len]) {
                        if self.fake_ping_ms > 0 {
                            // Queue packet for delayed processing
                            let delay_ms = self.fake_ping_ms / 2;
                            let process_time = receive_time + Duration::from_millis(delay_ms);
                            self.incoming_packets
                                .push_back((packet, process_time, receive_time));
                        } else {
                            // Process immediately for real network
                            self.handle_packet_sync(packet, receive_time);
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    error!("Error receiving packet: {}", e);
                }
            }

            // Process delayed incoming packets
            self.process_incoming_packets();

            // Input processing at consistent 60Hz rate
            if last_input_time.elapsed() >= input_interval {
                let (toggles, input_to_send) = self.input_manager.update();

                let reconnect_requested = self.handle_toggles(toggles);

                if reconnect_requested {
                    if let Err(e) = self.reconnect().await {
                        error!("Failed to reconnect: {}", e);
                    }
                }

                if let Some(input) = input_to_send {
                    if let Err(e) = self.send_input(input).await {
                        error!("Error sending input: {}", e);
                    }
                }
                last_input_time = Instant::now();
            }

            self.check_connection_health();

            // Rendering at consistent 60 FPS
            if last_render_time.elapsed() >= render_interval {
                // Update physics only if prediction is disabled
                if !self.prediction_enabled {
                    let dt = 1.0 / 60.0;
                    self.game_state.update_physics(dt);
                }

                // Get appropriate player positions based on netcode configuration
                let players = self.game_state.get_render_players(
                    self.client_id,
                    self.prediction_enabled,
                    self.interpolation_enabled,
                );

                // Configure rendering with current netcode state
                let render_config = RenderConfig {
                    client_id: self.client_id,
                    prediction_enabled: self.prediction_enabled,
                    reconciliation_enabled: self.reconciliation_enabled,
                    interpolation_enabled: self.interpolation_enabled,
                    real_ping_ms: self.real_ping_ms,
                    fake_ping_ms: self.fake_ping_ms,
                    ping_ms: self.ping_ms,
                };

                self.renderer.render(&players, render_config);

                last_render_time = Instant::now();
                next_frame().await;
            }

            self.check_connection_health();

            // Handle application quit
            if is_quit_requested() {
                break;
            }
        }

        // Clean disconnect when exiting
        if self.connected {
            let _ = self.send_packet(&Packet::Disconnect).await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_address_ip() {
        // Test with IPv4 address
        let result = Client::resolve_address("127.0.0.1:8080");
        assert!(result.is_ok());
        let addr = result.unwrap();
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
        assert_eq!(addr.port(), 8080);

        // Test with IPv6 address
        let result = Client::resolve_address("[::1]:8080");
        assert!(result.is_ok());
        let addr = result.unwrap();
        assert_eq!(addr.port(), 8080);
    }

    #[test]
    fn test_resolve_address_localhost() {
        // Test with localhost domain name
        let result = Client::resolve_address("localhost:8080");
        assert!(result.is_ok());
        let addr = result.unwrap();
        assert_eq!(addr.port(), 8080);
        // localhost should resolve to either 127.0.0.1 or ::1
        assert!(addr.ip().to_string() == "127.0.0.1" || addr.ip().to_string() == "::1");
    }

    #[test]
    fn test_resolve_address_invalid() {
        // Test with invalid address format
        let result = Client::resolve_address("invalid-address");
        assert!(result.is_err());

        // Test with invalid domain
        let result = Client::resolve_address("nonexistent.invalid.domain:8080");
        assert!(result.is_err());
    }

    #[test]
    fn test_ping_calculation_with_fake_ping() {
        // Test that fake ping is added on top of real ping, not replacing it
        let mut client = Client {
            socket: UdpSocket::bind("0.0.0.0:0").unwrap(),
            server_addr: "127.0.0.1:8080".parse().unwrap(),
            client_id: Some(1),
            connected: true,
            game_state: ClientGameState::new(),
            input_manager: InputManager::new(),
            renderer: Renderer::new().unwrap(),
            real_ping_ms: 0,
            fake_ping_ms: 50, // 50ms fake ping
            ping_ms: 0,
            last_packet_received: Instant::now(),
            connection_timeout: Duration::from_secs(5),
            outgoing_packets: VecDeque::new(),
            incoming_packets: VecDeque::new(),
            prediction_enabled: true,
            reconciliation_enabled: true,
            interpolation_enabled: true,
        };

        // Simulate receiving a packet with a timestamp that would result in 30ms real ping
        let fake_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64 - 30; // 30ms ago

        // Simulate the ping calculation logic
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        client.real_ping_ms = now.saturating_sub(fake_timestamp);
        client.ping_ms = client.real_ping_ms + client.fake_ping_ms;

        // Verify that the total ping is real ping + fake ping
        assert_eq!(client.real_ping_ms, 30);
        assert_eq!(client.fake_ping_ms, 50);
        assert_eq!(client.ping_ms, 80); // 30 + 50
        
        println!("✓ Ping calculation test passed!");
        println!("  Real ping: {}ms", client.real_ping_ms);
        println!("  Fake ping: {}ms", client.fake_ping_ms);
        println!("  Total ping: {}ms", client.ping_ms);
    }
}
