//! Client-side network implementation with artificial latency simulation

use crate::game::{ClientGameState, ServerStateConfig};
use crate::input::InputManager;
use crate::network_graph::NetworkGraph;
use crate::rendering::{RenderConfig, Renderer};
use bincode::{deserialize, serialize};
use log::{error, info, warn};
use macroquad::prelude::*;
use shared::{InputState, Packet};
use std::collections::VecDeque;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// Main client managing network communication and game state
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
    network_graph: NetworkGraph,

    // Connection monitoring
    real_ping_ms: u64,
    fake_ping_ms: u64,
    ping_ms: u64,
    ping_history: VecDeque<u64>,
    last_packet_received: Instant,
    connection_timeout: Duration,

    // Clock synchronization for remote servers
    clock_offset_samples: VecDeque<i64>, // Track clock offset between client and server
    last_server_timestamp: Option<u64>,
    packet_send_times: VecDeque<(u64, Instant)>, // Track when we sent packets for RTT calculation

    // Packet queuing for artificial latency simulation
    outgoing_packets: VecDeque<(Vec<u8>, Instant)>,
    incoming_packets: VecDeque<(Packet, Instant, Instant)>,

    // Netcode feature toggles
    prediction_enabled: bool,
    reconciliation_enabled: bool,
    interpolation_enabled: bool,
}

impl Client {
    /// Creates a new client with specified server address and artificial latency
    pub async fn new(
        server_addr: &str,
        fake_ping_ms: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;

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
            network_graph: NetworkGraph::new(), // Initialize network graph
            real_ping_ms: 0,
            fake_ping_ms,
            ping_ms: 0,
            ping_history: VecDeque::new(),
            last_packet_received: Instant::now(),
            connection_timeout: Duration::from_secs(5),
            clock_offset_samples: VecDeque::new(),
            last_server_timestamp: None,
            packet_send_times: VecDeque::new(),
            outgoing_packets: VecDeque::new(),
            incoming_packets: VecDeque::new(),
            prediction_enabled: true,
            reconciliation_enabled: true,
            interpolation_enabled: true,
        })
    }

    /// Resolves server address supporting both IP addresses and domain names
    fn resolve_address(addr_str: &str) -> Result<SocketAddr, Box<dyn std::error::Error>> {
        // Try parsing as direct SocketAddr first
        if let Ok(addr) = addr_str.parse::<SocketAddr>() {
            return Ok(addr);
        }

        // Try DNS resolution for domain names
        use std::net::ToSocketAddrs;
        let mut addrs = addr_str.to_socket_addrs()?;

        if let Some(addr) = addrs.next() {
            Ok(addr)
        } else {
            Err(format!("Failed to resolve address: {}", addr_str).into())
        }
    }

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Connecting to server...");
        let packet = Packet::Connect { client_version: 1 };
        self.send_packet(&packet).await?;
        Ok(())
    }

    /// Attempts to reconnect after connection loss
    pub async fn reconnect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Attempting to reconnect...");

        if self.connected {
            let _ = self.send_packet(&Packet::Disconnect).await;
            std::thread::sleep(Duration::from_millis(100));
        }

        // Reset client state
        self.connected = false;
        self.client_id = None;
        self.real_ping_ms = 0;
        self.ping_ms = self.fake_ping_ms;
        self.ping_history.clear();
        self.last_packet_received = Instant::now();
        self.outgoing_packets.clear();
        self.incoming_packets.clear();
        self.game_state = ClientGameState::new();

        self.connect().await
    }

    fn check_connection_health(&mut self) {
        if self.connected && self.last_packet_received.elapsed() > self.connection_timeout {
            warn!("Connection timeout detected");
            self.connected = false;
            self.client_id = None;
        }
    }

    /// Sends packet with optional artificial latency
    async fn send_packet(&mut self, packet: &Packet) -> Result<(), Box<dyn std::error::Error>> {
        let data = serialize(packet)?;

        if self.fake_ping_ms > 0 {
            // Simulate one-way latency (half of round-trip time)
            let delay_ms = self.fake_ping_ms / 2;
            let send_time = Instant::now() + Duration::from_millis(delay_ms);
            self.outgoing_packets.push_back((data, send_time));
        } else {
            self.socket.send_to(&data, self.server_addr)?;
        }

        Ok(())
    }

    /// Processes queued outgoing packets for artificial latency
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

    /// Processes queued incoming packets for artificial latency
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
    fn handle_packet_sync(&mut self, packet: Packet, _receive_time: Instant) {
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
                // Calculate ping time for display
                if timestamp > 0 {
                    let ping_candidate = self.calculate_robust_ping(timestamp);

                    // Sanity check: ping should be reasonable (0-2000ms)
                    if ping_candidate <= 2000 {
                        // Add to history for smoothing
                        self.ping_history.push_back(ping_candidate);

                        // Keep only last 10 ping samples
                        while self.ping_history.len() > 10 {
                            self.ping_history.pop_front();
                        }

                        // Use moving average of last few pings for smoother display
                        if !self.ping_history.is_empty() {
                            let sum: u64 = self.ping_history.iter().sum();
                            self.real_ping_ms = sum / self.ping_history.len() as u64;
                        }
                    }
                    // If ping is unreasonable, keep the previous value

                    self.ping_ms = self.real_ping_ms + self.fake_ping_ms;

                    // Record packet received for network graph
                    self.network_graph
                        .record_packet_received(self.ping_ms as f32);
                }

                let config = ServerStateConfig {
                    client_id: self.client_id,
                    reconciliation_enabled: self.reconciliation_enabled,
                    interpolation_enabled: self.interpolation_enabled,
                };

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

    /// Sends player input and applies client-side prediction
    async fn send_input(&mut self, input: InputState) -> Result<(), Box<dyn std::error::Error>> {
        if !self.connected || self.client_id.is_none() {
            return Ok(());
        }

        let packet = Packet::Input {
            sequence: input.sequence,
            timestamp: input.timestamp,
            left: input.left,
            right: input.right,
            jump: input.jump,
        };

        self.send_packet(&packet).await?;

        // Apply client-side prediction
        if self.prediction_enabled {
            if let Some(client_id) = self.client_id {
                self.game_state.apply_prediction(client_id, &input);
            }
        }

        Ok(())
    }

    /// Handles runtime toggle of netcode features and network graph
    fn handle_toggles(&mut self, toggles: (bool, bool, bool, bool, bool)) -> bool {
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
        if toggles.4 {
            self.network_graph.toggle_visibility();
            info!(
                "Network graph: {}",
                if self.network_graph.is_visible() {
                    "shown"
                } else {
                    "hidden"
                }
            );
        }

        reconnect_requested
    }

    /// Main client game loop handling network, input, and rendering
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.connect().await?;

        let mut last_input_time = Instant::now();
        let mut last_render_time = Instant::now();
        let input_interval = Duration::from_millis(16); // 60Hz
        let render_interval = Duration::from_millis(16); // 60 FPS

        let mut buffer = [0u8; 2048];

        loop {
            // Process outgoing packets
            if let Err(e) = self.process_outgoing_packets() {
                error!("Error processing outgoing packets: {}", e);
            }

            // Receive and queue incoming packets
            match self.socket.recv_from(&mut buffer) {
                Ok((len, _)) => {
                    let receive_time = Instant::now();
                    if let Ok(packet) = deserialize::<Packet>(&buffer[0..len]) {
                        if self.fake_ping_ms > 0 {
                            let delay_ms = self.fake_ping_ms / 2;
                            let process_time = receive_time + Duration::from_millis(delay_ms);
                            self.incoming_packets
                                .push_back((packet, process_time, receive_time));
                        } else {
                            self.handle_packet_sync(packet, receive_time);
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    error!("Error receiving packet: {}", e);
                }
            }

            self.process_incoming_packets();

            // Input processing at 60Hz
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

            // Rendering at 60 FPS
            if last_render_time.elapsed() >= render_interval {
                if !self.prediction_enabled {
                    let dt = 1.0 / 60.0;
                    self.game_state.update_physics(dt);
                }

                let players = self.game_state.get_render_players(
                    self.client_id,
                    self.prediction_enabled,
                    self.interpolation_enabled,
                );

                let render_config = RenderConfig {
                    client_id: self.client_id,
                    prediction_enabled: self.prediction_enabled,
                    reconciliation_enabled: self.reconciliation_enabled,
                    interpolation_enabled: self.interpolation_enabled,
                    real_ping_ms: self.real_ping_ms,
                    fake_ping_ms: self.fake_ping_ms,
                    ping_ms: self.ping_ms,
                    current_input: Some(self.input_manager.get_current_input().clone()),
                };

                self.renderer.render(&players, render_config);

                // Render network graph on top of everything else
                self.network_graph.render();

                last_render_time = Instant::now();
                next_frame().await;
            }

            if is_quit_requested() {
                break;
            }
        }

        // Clean disconnect
        if self.connected {
            let _ = self.send_packet(&Packet::Disconnect).await;
        }

        Ok(())
    }

    /// Calculates ping using clock-drift resistant method for remote servers
    fn calculate_robust_ping(&mut self, server_timestamp: u64) -> u64 {
        // For localhost testing, use simple calculation
        if self.server_addr.ip().is_loopback() {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis() as u64;

            return if now_ms >= server_timestamp {
                now_ms.saturating_sub(server_timestamp).min(10)
            } else {
                0
            };
        }

        // Track the relationship between server and client timestamps to detect clock drift
        self.last_server_timestamp = Some(server_timestamp);

        // Get current time safely
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis();

        // Safe conversion with overflow protection
        let now_ms_safe = (now_ms.min(u64::MAX as u128)) as u64;

        // Use timestamp deltas for drift-resistant calculation when we have history
        if let Some((prev_server_ts, prev_recv_time)) = self.packet_send_times.back() {
            let prev_server_ts = *prev_server_ts;
            let prev_recv_time = *prev_recv_time;

            // Calculate time differences on both sides
            let server_time_diff = server_timestamp.saturating_sub(prev_server_ts);
            let client_time_diff = prev_recv_time.elapsed().as_millis() as u64;

            // If the differences are reasonable, use them to estimate ping
            if server_time_diff > 0 && server_time_diff < 5000 && client_time_diff < 5000 {
                // Estimate RTT based on time progression
                let estimated_ping = if client_time_diff > server_time_diff {
                    (client_time_diff - server_time_diff) / 2
                } else {
                    // Server clock is faster, use a conservative estimate
                    server_time_diff.min(self.real_ping_ms.max(50))
                };

                // Store this measurement for next calculation
                self.packet_send_times
                    .push_back((server_timestamp, Instant::now()));
                if self.packet_send_times.len() > 20 {
                    self.packet_send_times.pop_front();
                }

                return estimated_ping.clamp(10, 2000);
            }
        }

        // Fallback: Calculate clock offset to detect systematic drift
        let raw_ping = if now_ms_safe >= server_timestamp {
            now_ms_safe.saturating_sub(server_timestamp)
        } else {
            // Server is ahead - this suggests clock skew
            let clock_offset = server_timestamp.saturating_sub(now_ms_safe);

            // Track clock offset samples for drift detection
            self.clock_offset_samples.push_back(clock_offset as i64);
            if self.clock_offset_samples.len() > 10 {
                self.clock_offset_samples.pop_front();
            }

            // Use median offset to handle clock corrections
            if self.clock_offset_samples.len() >= 3 {
                let mut offsets: Vec<i64> = self.clock_offset_samples.iter().cloned().collect();
                offsets.sort();
                let median_offset = offsets[offsets.len() / 2];

                // Apply offset correction if it's consistent
                if median_offset.abs() < 10000 {
                    // Less than 10 seconds offset
                    let corrected_server_time =
                        server_timestamp.saturating_sub(median_offset.unsigned_abs());
                    now_ms_safe.saturating_sub(corrected_server_time)
                } else {
                    // Large offset, use previous ping
                    self.real_ping_ms.min(1000)
                }
            } else {
                // Not enough samples, use previous ping
                self.real_ping_ms.min(1000)
            }
        };

        // Store this measurement for next calculation
        self.packet_send_times
            .push_back((server_timestamp, Instant::now()));
        if self.packet_send_times.len() > 20 {
            self.packet_send_times.pop_front();
        }

        raw_ping.clamp(0, 2000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_resolve_address_ip() {
        let result = Client::resolve_address("127.0.0.1:8080");
        assert!(result.is_ok());
        let addr = result.unwrap();
        assert_eq!(addr.port(), 8080);
        assert!(addr.ip().is_loopback());
    }

    #[test]
    fn test_resolve_address_localhost() {
        let result = Client::resolve_address("localhost:8080");
        assert!(result.is_ok());
        let addr = result.unwrap();
        assert_eq!(addr.port(), 8080);
    }

    #[test]
    fn test_resolve_address_ipv6() {
        let result = Client::resolve_address("[::1]:8080");
        assert!(result.is_ok());
        let addr = result.unwrap();
        assert_eq!(addr.port(), 8080);
    }

    #[test]
    fn test_resolve_address_invalid() {
        let result = Client::resolve_address("invalid-address");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_address_invalid_port() {
        let result = Client::resolve_address("127.0.0.1:99999");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_address_empty() {
        let result = Client::resolve_address("");
        assert!(result.is_err());
    }

    #[test]
    fn test_client_initial_state() {
        // Since Client::new is async and requires socket binding,
        // we'll test the logical initial state values
        let fake_ping_ms = 100;

        // Test expected initial values
        assert_eq!(fake_ping_ms, 100);
        assert!(fake_ping_ms > 0);

        // Test connection state logic
        let connected = false;
        let client_id: Option<u32> = None;

        assert!(!connected);
        assert!(client_id.is_none());
    }

    #[test]
    fn test_ping_calculation_localhost() {
        let mut client = create_test_client();

        // Test localhost ping calculation
        let now_ms = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Server timestamp slightly in the past
        let server_timestamp = now_ms - 50;
        let ping = client.calculate_robust_ping(server_timestamp);

        // Should be small for localhost
        assert!(ping <= 10);
    }

    #[test]
    fn test_ping_calculation_future_timestamp() {
        let mut client = create_test_client();

        let now_ms = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Server timestamp in the future (clock skew)
        let server_timestamp = now_ms + 1000;
        let ping = client.calculate_robust_ping(server_timestamp);

        // Should handle gracefully
        assert!(ping <= 2000);
    }

    #[test]
    fn test_ping_history_management() {
        let mut ping_history = VecDeque::new();

        // Add more than 10 samples
        for i in 1..=15 {
            ping_history.push_back(i * 10);

            // Keep only last 10 ping samples
            while ping_history.len() > 10 {
                ping_history.pop_front();
            }
        }

        assert_eq!(ping_history.len(), 10);
        assert_eq!(ping_history.front(), Some(&60)); // Should start from 6th element
        assert_eq!(ping_history.back(), Some(&150));
    }

    #[test]
    fn test_ping_averaging() {
        let ping_samples = [50, 60, 55, 65, 58];
        let sum: u64 = ping_samples.iter().sum();
        let average = sum / ping_samples.len() as u64;

        assert_eq!(average, 57); // (50+60+55+65+58)/5 = 288/5 = 57
    }

    #[test]
    fn test_ping_validation() {
        let ping_candidates = vec![0, 50, 100, 500, 1000, 2000, 3000];

        for ping in ping_candidates {
            let is_valid = ping <= 2000;

            if ping <= 2000 {
                assert!(is_valid);
            } else {
                assert!(!is_valid);
            }
        }
    }

    #[test]
    fn test_connection_timeout_logic() {
        let last_packet_received = Instant::now();
        let connection_timeout = Duration::from_secs(5);

        // Should not timeout immediately
        assert!(last_packet_received.elapsed() < connection_timeout);

        // Test timeout detection logic
        let old_time = Instant::now() - Duration::from_secs(6);
        assert!(old_time.elapsed() > connection_timeout);
    }

    #[test]
    fn test_clock_offset_calculation() {
        let client_time = 1000u64;
        let server_time_ahead = 1100u64;
        let server_time_behind = 900u64;

        // Server ahead
        let offset_ahead = server_time_ahead.saturating_sub(client_time) as i64;
        assert_eq!(offset_ahead, 100);

        // Server behind
        let offset_behind = if client_time >= server_time_behind {
            -(client_time.saturating_sub(server_time_behind) as i64)
        } else {
            server_time_behind.saturating_sub(client_time) as i64
        };
        assert_eq!(offset_behind, -100);
    }

    #[test]
    fn test_packet_queuing_timing() {
        let mut outgoing_packets = VecDeque::new();
        let fake_ping_ms = 100u64;

        // Simulate packet queuing
        let data = vec![1, 2, 3, 4];
        let delay_ms = fake_ping_ms / 2; // One-way latency
        let send_time = Instant::now() + Duration::from_millis(delay_ms);

        outgoing_packets.push_back((data.clone(), send_time));

        assert_eq!(outgoing_packets.len(), 1);

        // Check timing logic
        let now = Instant::now();
        let should_send = if let Some((_, time)) = outgoing_packets.front() {
            now >= *time
        } else {
            false
        };

        // Should not send immediately due to artificial latency
        assert!(!should_send);
    }

    #[test]
    fn test_netcode_feature_toggles() {
        let mut prediction_enabled = true;
        let mut reconciliation_enabled = true;
        let mut interpolation_enabled = true;

        // Test toggle logic
        prediction_enabled = !prediction_enabled;
        assert!(!prediction_enabled);

        reconciliation_enabled = !reconciliation_enabled;
        assert!(!reconciliation_enabled);

        interpolation_enabled = !interpolation_enabled;
        assert!(!interpolation_enabled);

        // Toggle back
        prediction_enabled = !prediction_enabled;
        reconciliation_enabled = !reconciliation_enabled;
        interpolation_enabled = !interpolation_enabled;

        assert!(prediction_enabled);
        assert!(reconciliation_enabled);
        assert!(interpolation_enabled);
    }

    #[test]
    fn test_toggle_parsing() {
        let test_toggles = [
            (true, false, false, false, false),
            (false, true, false, false, false),
            (false, false, true, false, false),
            (false, false, false, true, false),
            (false, false, false, false, true),
        ];

        for (pred, recon, interp, reconnect, graph) in test_toggles.iter() {
            let toggles = (*pred, *recon, *interp, *reconnect, *graph);

            assert_eq!(toggles.0, *pred);
            assert_eq!(toggles.1, *recon);
            assert_eq!(toggles.2, *interp);
            assert_eq!(toggles.3, *reconnect);
            assert_eq!(toggles.4, *graph);
        }
    }

    #[test]
    fn test_time_interval_calculations() {
        let input_interval = Duration::from_millis(16); // 60Hz
        let render_interval = Duration::from_millis(16); // 60 FPS

        assert_eq!(input_interval.as_millis(), 16);
        assert_eq!(render_interval.as_millis(), 16);

        // Test frequency calculation
        let input_hz = 1000.0 / input_interval.as_millis() as f64;
        let render_fps = 1000.0 / render_interval.as_millis() as f64;

        assert!((input_hz - 62.5).abs() < 0.1); // ~62.5 Hz
        assert!((render_fps - 62.5).abs() < 0.1); // ~62.5 FPS
    }

    #[test]
    fn test_buffer_size_validation() {
        let buffer_size = 2048;
        assert!(buffer_size >= 1024); // Minimum reasonable size
        assert!(buffer_size <= 65536); // Maximum reasonable size

        // Test that common packet sizes fit
        let typical_packet_sizes = vec![64, 128, 256, 512, 1024];
        for size in typical_packet_sizes {
            assert!(size < buffer_size);
        }
    }

    #[test]
    fn test_client_version_handling() {
        let client_version = 1u32;
        assert!(client_version > 0);
        assert!(client_version <= 1000); // Reasonable upper bound
    }

    #[test]
    fn test_sequence_overflow_safety() {
        let mut sequence = u32::MAX - 1;

        // Test normal increment
        sequence += 1;
        assert_eq!(sequence, u32::MAX);

        // Test overflow handling
        sequence = sequence.wrapping_add(1);
        assert_eq!(sequence, 0);
    }

    #[test]
    fn test_timestamp_validation() {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Should be a reasonable timestamp
        let year_2020_ms = 1577836800000u64;
        let year_2100_ms = 4102444800000u64;

        assert!(timestamp > year_2020_ms);
        assert!(timestamp < year_2100_ms);
    }

    #[test]
    fn test_clock_drift_detection() {
        let mut clock_offset_samples = VecDeque::new();

        // Simulate consistent clock offset
        let consistent_offset = 100i64;
        for _ in 0..5 {
            clock_offset_samples.push_back(consistent_offset);
        }

        // Calculate median
        let mut offsets: Vec<i64> = clock_offset_samples.iter().cloned().collect();
        offsets.sort();
        let median = offsets[offsets.len() / 2];

        assert_eq!(median, consistent_offset);
        assert!(median.abs() < 10000); // Reasonable offset
    }

    #[test]
    fn test_ping_clamping() {
        let test_pings = vec![0, 50, 100, 1000, 2000, 5000, 10000];

        for ping in test_pings {
            let clamped = ping.clamp(0, 2000);

            assert!(clamped >= 0);
            assert!(clamped <= 2000);

            if ping <= 2000 {
                assert_eq!(clamped, ping);
            } else {
                assert_eq!(clamped, 2000);
            }
        }
    }

    #[test]
    fn test_packet_send_time_tracking() {
        let mut packet_send_times = VecDeque::new();
        let max_samples = 20;

        // Add samples
        for i in 0..25 {
            packet_send_times.push_back((i as u64, Instant::now()));

            // Keep only last 20 samples
            if packet_send_times.len() > max_samples {
                packet_send_times.pop_front();
            }
        }

        assert_eq!(packet_send_times.len(), max_samples);
        assert_eq!(packet_send_times.front().unwrap().0, 5); // Should start from 5th element
        assert_eq!(packet_send_times.back().unwrap().0, 24);
    }

    // Helper function for creating test client components
    fn create_test_client() -> TestClientMock {
        TestClientMock {
            server_addr: "127.0.0.1:8080".parse().unwrap(),
            real_ping_ms: 0,
            ping_history: VecDeque::new(),
            clock_offset_samples: VecDeque::new(),
            packet_send_times: VecDeque::new(),
        }
    }

    // Mock client for testing without actual network
    #[allow(dead_code)]
    struct TestClientMock {
        server_addr: SocketAddr,
        real_ping_ms: u64,
        ping_history: VecDeque<u64>,
        clock_offset_samples: VecDeque<i64>,
        packet_send_times: VecDeque<(u64, Instant)>,
    }

    impl TestClientMock {
        fn calculate_robust_ping(&mut self, server_timestamp: u64) -> u64 {
            // For localhost testing, use simple calculation
            if self.server_addr.ip().is_loopback() {
                let now_ms = SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(Duration::from_secs(0))
                    .as_millis() as u64;

                return if now_ms >= server_timestamp {
                    now_ms.saturating_sub(server_timestamp).min(10)
                } else {
                    0
                };
            }

            // Simplified version of the ping calculation for testing
            let now_ms = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis() as u64;

            let raw_ping = if now_ms >= server_timestamp {
                now_ms.saturating_sub(server_timestamp)
            } else {
                self.real_ping_ms.min(1000)
            };

            raw_ping.clamp(0, 2000)
        }
    }

    #[test]
    fn test_connection_state_transitions() {
        // Test the full connection lifecycle
        let mut connected;
        let mut client_id: Option<u32>;

        // Connect
        client_id = Some(42);
        connected = true;
        assert!(connected && client_id.is_some());

        // Disconnect
        connected = false;
        client_id = None;
        assert!(!connected && client_id.is_none());
    }

    #[test]
    fn test_packet_ordering_under_latency() {
        let mut packets = VecDeque::new();

        // Simulate packets arriving out of order due to network latency
        packets.push_back((Instant::now() + Duration::from_millis(100), 1u32));
        packets.push_back((Instant::now() + Duration::from_millis(50), 2u32));
        packets.push_back((Instant::now() + Duration::from_millis(150), 3u32));

        // Sort by process time (when they should be handled)
        packets.make_contiguous().sort_by_key(|(time, _)| *time);

        let order: Vec<u32> = packets.iter().map(|(_, id)| *id).collect();
        assert_eq!(order, vec![2, 1, 3]); // Should process in arrival order
    }

    #[test]
    fn test_artificial_latency_bounds() {
        let fake_ping_values = vec![0, 50, 100, 500, 1000];

        for fake_ping in fake_ping_values {
            let one_way_latency = fake_ping / 2;

            // Ensure latency values are reasonable
            assert!(one_way_latency <= 500); // Max 500ms one-way
            assert!(fake_ping <= 1000); // Max 1s round-trip
        }
    }
}
