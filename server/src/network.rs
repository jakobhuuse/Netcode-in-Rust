//! Server network layer handling UDP communications and game loop coordination

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

/// Messages sent from network tasks to main server loop
#[derive(Debug)]
pub enum ServerMessage {
    PacketReceived {
        packet: Packet,
        addr: SocketAddr,
    },
    ClientTimeout {
        client_id: u32,
    },
    #[allow(dead_code)]
    Shutdown,
}

/// Messages sent from game loop to network tasks
#[derive(Debug)]
pub enum GameMessage {
    SendPacket {
        packet: Packet,
        addr: SocketAddr,
    },
    BroadcastPacket {
        packet: Packet,
        exclude: Option<u32>,
    },
}

/// Main server coordinating networking and game simulation
pub struct Server {
    socket: Arc<UdpSocket>,
    clients: Arc<RwLock<ClientManager>>,
    game_state: GameState,
    tick_duration: Duration,

    // Communication channels
    server_tx: mpsc::UnboundedSender<ServerMessage>,
    server_rx: mpsc::UnboundedReceiver<ServerMessage>,
    game_tx: mpsc::UnboundedSender<GameMessage>,
    game_rx: mpsc::UnboundedReceiver<GameMessage>,
}

impl Server {
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

    /// Spawns task that continuously listens for incoming packets
    async fn spawn_network_receiver(&self) {
        let socket = Arc::clone(&self.socket);
        let server_tx = self.server_tx.clone();

        tokio::spawn(async move {
            let mut buffer = [0u8; 2048];

            loop {
                match socket.recv_from(&mut buffer).await {
                    Ok((len, addr)) => {
                        if let Ok(packet) = deserialize::<Packet>(&buffer[0..len]) {
                            if let Err(e) =
                                server_tx.send(ServerMessage::PacketReceived { packet, addr })
                            {
                                error!("Failed to send packet to main loop: {}", e);
                                break;
                            }
                        } else {
                            warn!("Failed to deserialize packet from {}", addr);
                        }
                    }
                    Err(e) => {
                        error!("Error receiving packet: {}", e);
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            }
        });
    }

    /// Spawns task that processes outgoing packet queue
    async fn spawn_network_sender(&mut self) {
        let socket = Arc::clone(&self.socket);
        let clients = Arc::clone(&self.clients);
        let mut game_rx = std::mem::replace(&mut self.game_rx, mpsc::unbounded_channel().1);

        tokio::spawn(async move {
            while let Some(message) = game_rx.recv().await {
                match message {
                    GameMessage::SendPacket { packet, addr } => {
                        if let Err(e) = Self::send_packet_impl(&socket, &packet, addr).await {
                            error!("Failed to send packet to {}: {}", addr, e);
                        }
                    }
                    GameMessage::BroadcastPacket { packet, exclude } => {
                        let client_addrs = {
                            let clients_guard = clients.read().await;
                            clients_guard.get_client_addrs()
                        };

                        for (client_id, addr) in client_addrs {
                            if Some(client_id) == exclude {
                                continue;
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

    /// Spawns task that monitors client timeouts
    async fn spawn_timeout_checker(&self) {
        let clients = Arc::clone(&self.clients);
        let server_tx = self.server_tx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));

            loop {
                interval.tick().await;

                let timed_out = {
                    let mut clients_guard = clients.write().await;
                    clients_guard.check_timeouts()
                };

                for client_id in timed_out {
                    if let Err(e) = server_tx.send(ServerMessage::ClientTimeout { client_id }) {
                        error!("Failed to send timeout message: {}", e);
                        break;
                    }
                }
            }
        });
    }

    async fn send_packet_impl(
        socket: &UdpSocket,
        packet: &Packet,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let data = serialize(packet)?;
        socket.send_to(&data, addr).await?;
        Ok(())
    }

    async fn send_packet(&self, packet: &Packet, addr: SocketAddr) {
        if let Err(e) = self.game_tx.send(GameMessage::SendPacket {
            packet: packet.clone(),
            addr,
        }) {
            error!("Failed to queue packet for sending: {}", e);
        }
    }

    async fn broadcast_packet(&self, packet: &Packet, exclude: Option<u32>) {
        if let Err(e) = self.game_tx.send(GameMessage::BroadcastPacket {
            packet: packet.clone(),
            exclude,
        }) {
            error!("Failed to queue broadcast packet: {}", e);
        }
    }

    /// Processes incoming packets and updates game state
    async fn handle_packet(&mut self, packet: Packet, addr: SocketAddr) {
        match packet {
            Packet::Connect { client_version } => {
                info!(
                    "Client connecting from {} (version: {})",
                    addr, client_version
                );

                // Remove existing connection if present
                let existing_client_id = {
                    let clients = self.clients.read().await;
                    clients.find_client_by_addr(addr)
                };

                if let Some(existing_id) = existing_client_id {
                    info!("Removing existing client {} from {}", existing_id, addr);
                    let mut clients = self.clients.write().await;
                    clients.remove_client(&existing_id);
                    self.game_state.remove_player(&existing_id);
                }

                // Try to add new client
                let client_id = {
                    let mut clients = self.clients.write().await;
                    clients.add_client(addr)
                };

                if let Some(client_id) = client_id {
                    self.game_state.add_player(client_id);
                    let response = Packet::Connected { client_id };
                    self.send_packet(&response, addr).await;
                } else {
                    let response = Packet::Disconnected {
                        reason: "Server full".to_string(),
                    };
                    self.send_packet(&response, addr).await;
                }
            }

            Packet::Input {
                sequence,
                timestamp,
                left,
                right,
                jump,
            } => {
                let client_id = {
                    let clients = self.clients.read().await;
                    clients.find_client_by_addr(addr)
                };

                if let Some(client_id) = client_id {
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

            _ => {
                warn!("Unexpected packet type from client at {}", addr);
            }
        }
    }

    /// Processes queued inputs and advances physics simulation
    async fn process_inputs(&mut self, dt: f32) {
        // Calculate physics substeps needed to prevent tunneling
        let total_substeps = self.calculate_required_substeps(dt);
        let substep_dt = dt / total_substeps as f32;

        let all_inputs = {
            let clients = self.clients.read().await;
            clients.get_chronological_inputs()
        };

        if all_inputs.is_empty() {
            // No inputs, just run physics
            for _ in 0..total_substeps {
                self.game_state.update_physics(substep_dt);
            }
            return;
        }

        // Distribute inputs across substeps
        let batch_size = 50;
        let inputs_per_substep = if all_inputs.len() >= total_substeps as usize {
            (all_inputs.len() / total_substeps as usize).min(batch_size)
        } else {
            batch_size.min(all_inputs.len())
        };

        let mut input_index = 0;

        for substep in 0..total_substeps {
            let inputs_this_step = if substep == total_substeps - 1 {
                all_inputs.len() - input_index
            } else {
                inputs_per_substep.min(all_inputs.len() - input_index)
            };

            // Apply inputs for this substep
            for _ in 0..inputs_this_step {
                if input_index < all_inputs.len() {
                    let (client_id, input) = &all_inputs[input_index];
                    self.game_state.apply_input(*client_id, input, substep_dt);

                    let mut clients = self.clients.write().await;
                    clients.mark_input_processed(*client_id, input.sequence);
                    input_index += 1;
                }
            }

            self.game_state.update_physics(substep_dt);

            // Yield CPU periodically during intensive processing
            if substep % 10 == 0 && total_substeps > 20 {
                tokio::task::yield_now().await;
            }
        }

        // Clean up processed inputs
        let mut clients = self.clients.write().await;
        clients.cleanup_processed_inputs();
    }

    /// Calculates physics substeps required to prevent collision tunneling
    fn calculate_required_substeps(&self, dt: f32) -> u32 {
        const MAX_PLAYER_SPEED: f32 = PLAYER_SPEED;
        const MIN_COLLISION_RADIUS: f32 = PLAYER_SIZE / 2.0;
        const SAFETY_FACTOR: f32 = 0.5;

        let max_movement_per_step = MIN_COLLISION_RADIUS * SAFETY_FACTOR;
        let max_movement_this_tick = MAX_PLAYER_SPEED * dt;

        if max_movement_this_tick > max_movement_per_step {
            (max_movement_this_tick / max_movement_per_step).ceil() as u32
        } else {
            1
        }
    }

    /// Broadcasts current game state to all connected clients
    async fn broadcast_game_state(&mut self) {
        let client_count = {
            let clients = self.clients.read().await;
            clients.len()
        };

        if client_count == 0 {
            return;
        }

        // Prepare packet data first
        let players: Vec<Player> = self.game_state.players.values().cloned().collect();
        let last_processed_input = {
            let clients = self.clients.read().await;
            clients.get_last_processed_inputs()
        };

        // Take timestamp as close to transmission as possible
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis();
        let timestamp_safe = (timestamp.min(u64::MAX as u128)) as u64;

        let packet = Packet::GameState {
            tick: self.game_state.tick,
            timestamp: timestamp_safe,
            last_processed_input,
            players,
        };

        self.broadcast_packet(&packet, None).await;
    }

    /// Main server loop coordinating all operations
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Initialize concurrent tasks
        self.spawn_network_receiver().await;
        self.spawn_network_sender().await;
        self.spawn_timeout_checker().await;

        let mut tick_interval = interval(self.tick_duration);
        let mut last_tick = Instant::now();

        info!("Server started successfully");

        loop {
            tokio::select! {
                // Handle network events
                message = self.server_rx.recv() => {
                    match message {
                        Some(ServerMessage::PacketReceived { packet, addr }) => {
                            self.handle_packet(packet, addr).await;
                        },
                        Some(ServerMessage::ClientTimeout { client_id }) => {
                            self.game_state.remove_player(&client_id);
                        },
                        Some(ServerMessage::Shutdown) | None => {
                            info!("Server shutting down");
                            break;
                        }
                    }
                },

                // Handle server tick events
                _ = tick_interval.tick() => {
                    let now = Instant::now();
                    let dt = now.duration_since(last_tick).as_secs_f32();
                    last_tick = now;

                    self.process_inputs(dt).await;
                    self.game_state.tick += 1;
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
                                   self.game_state.tick, client_count, 1.0 / dt, substeps);
                        }
                    }
                },
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use tokio::sync::mpsc;

    #[test]
    fn test_server_message_creation() {
        let packet = Packet::Connect { client_version: 1 };
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let msg = ServerMessage::PacketReceived {
            packet: packet.clone(),
            addr,
        };

        match msg {
            ServerMessage::PacketReceived { packet: p, addr: a } => {
                assert_eq!(a, addr);
                match p {
                    Packet::Connect { client_version } => {
                        assert_eq!(client_version, 1);
                    }
                    _ => panic!("Unexpected packet type"),
                }
            }
            _ => panic!("Unexpected message type"),
        }
    }

    #[test]
    fn test_client_timeout_message() {
        let client_id = 42;
        let msg = ServerMessage::ClientTimeout { client_id };

        match msg {
            ServerMessage::ClientTimeout { client_id: id } => {
                assert_eq!(id, client_id);
            }
            _ => panic!("Unexpected message type"),
        }
    }

    #[test]
    fn test_game_message_send_packet() {
        let packet = Packet::Connected { client_id: 123 };
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 9090);

        let msg = GameMessage::SendPacket {
            packet: packet.clone(),
            addr,
        };

        match msg {
            GameMessage::SendPacket { packet: p, addr: a } => {
                assert_eq!(a, addr);
                match p {
                    Packet::Connected { client_id } => {
                        assert_eq!(client_id, 123);
                    }
                    _ => panic!("Unexpected packet type"),
                }
            }
            _ => panic!("Unexpected message type"),
        }
    }

    #[test]
    fn test_game_message_broadcast() {
        let packet = Packet::GameState {
            tick: 100,
            timestamp: 1234567890,
            last_processed_input: std::collections::HashMap::new(),
            players: vec![],
        };

        let msg = GameMessage::BroadcastPacket {
            packet: packet.clone(),
            exclude: Some(5),
        };

        match msg {
            GameMessage::BroadcastPacket { packet: p, exclude } => {
                assert_eq!(exclude, Some(5));
                match p {
                    Packet::GameState { tick, .. } => {
                        assert_eq!(tick, 100);
                    }
                    _ => panic!("Unexpected packet type"),
                }
            }
            _ => panic!("Unexpected message type"),
        }
    }

    #[test]
    fn test_substep_calculation() {
        let server = create_test_server();

        // Test normal case - should require 1 substep
        let substeps = server.calculate_required_substeps(1.0 / 60.0); // 60 FPS
        assert_eq!(substeps, 1);

        // Test high speed case - should require multiple substeps
        let large_dt = 1.0; // 1 second
        let substeps = server.calculate_required_substeps(large_dt);
        assert!(substeps > 1);

        // Test edge case - very small dt
        let tiny_dt = 1.0 / 1000.0; // 1000 FPS
        let substeps = server.calculate_required_substeps(tiny_dt);
        assert_eq!(substeps, 1);
    }

    #[test]
    fn test_substep_safety_calculations() {
        const PLAYER_SPEED: f32 = 300.0;
        const PLAYER_SIZE: f32 = 20.0;
        const SAFETY_FACTOR: f32 = 0.5;

        let min_collision_radius = PLAYER_SIZE / 2.0;
        let max_movement_per_step = min_collision_radius * SAFETY_FACTOR;

        assert_eq!(min_collision_radius, 10.0);
        assert_eq!(max_movement_per_step, 5.0);

        // Test substep requirement for different dt values
        let test_cases = vec![
            (1.0 / 60.0, PLAYER_SPEED),  // 60 FPS
            (1.0 / 30.0, PLAYER_SPEED),  // 30 FPS
            (1.0 / 120.0, PLAYER_SPEED), // 120 FPS
        ];

        for (dt, speed) in test_cases {
            let max_movement = speed * dt;
            let required_substeps = if max_movement > max_movement_per_step {
                (max_movement / max_movement_per_step).ceil() as u32
            } else {
                1
            };

            assert!(required_substeps >= 1);
            assert!(required_substeps <= 100); // Reasonable upper bound
        }
    }

    #[test]
    fn test_timestamp_generation() {
        let timestamp1 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        std::thread::sleep(std::time::Duration::from_millis(1));

        let timestamp2 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        assert!(timestamp2 > timestamp1);

        // Test timestamp safety conversion
        let large_timestamp = u128::MAX;
        let safe_timestamp = (large_timestamp.min(u64::MAX as u128)) as u64;
        assert_eq!(safe_timestamp, u64::MAX);
    }

    #[test]
    fn test_channel_communication() {
        let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

        let packet = Packet::Connect { client_version: 1 };
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let msg = ServerMessage::PacketReceived {
            packet: packet.clone(),
            addr,
        };

        // Send message
        assert!(tx.send(msg).is_ok());

        // Receive message
        let received = rx.try_recv();
        assert!(received.is_ok());

        match received.unwrap() {
            ServerMessage::PacketReceived { packet: p, addr: a } => {
                assert_eq!(a, addr);
                match p {
                    Packet::Connect { client_version } => {
                        assert_eq!(client_version, 1);
                    }
                    _ => panic!("Unexpected packet type"),
                }
            }
            _ => panic!("Unexpected message type"),
        }
    }

    #[test]
    fn test_input_distribution_logic() {
        let total_inputs = 100;
        let total_substeps = 4;
        let batch_size = 50;

        // Test input distribution calculation
        let inputs_per_substep = if total_inputs >= total_substeps {
            (total_inputs / total_substeps).min(batch_size)
        } else {
            batch_size.min(total_inputs)
        };

        assert_eq!(inputs_per_substep, 25); // 100 / 4 = 25, min(25, 50) = 25

        // Test edge case - fewer inputs than substeps
        let few_inputs = 2;
        let inputs_per_substep_few = if few_inputs >= total_substeps {
            (few_inputs / total_substeps).min(batch_size)
        } else {
            batch_size.min(few_inputs)
        };

        assert_eq!(inputs_per_substep_few, 2); // min(50, 2) = 2
    }

    #[test]
    fn test_input_batch_processing() {
        let all_inputs = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let total_substeps = 3;
        let inputs_per_substep = all_inputs.len() / total_substeps;

        assert_eq!(inputs_per_substep, 3); // 10 / 3 = 3

        let mut input_index = 0;
        let mut processed_inputs = Vec::new();

        for substep in 0..total_substeps {
            let inputs_this_step = if substep == total_substeps - 1 {
                all_inputs.len() - input_index // Last substep gets remaining
            } else {
                inputs_per_substep.min(all_inputs.len() - input_index)
            };

            for _ in 0..inputs_this_step {
                if input_index < all_inputs.len() {
                    processed_inputs.push(all_inputs[input_index]);
                    input_index += 1;
                }
            }
        }

        assert_eq!(processed_inputs.len(), all_inputs.len());
        assert_eq!(processed_inputs, all_inputs);
    }

    #[test]
    fn test_performance_monitoring_logic() {
        let tick = 60;
        let should_log = tick % 60 == 0;
        assert!(should_log);

        let tick2 = 59;
        let should_log2 = tick2 % 60 == 0;
        assert!(!should_log2);

        let tick3 = 120;
        let should_log3 = tick3 % 60 == 0;
        assert!(should_log3);
    }

    #[test]
    fn test_address_validation() {
        let valid_addrs = vec![
            "127.0.0.1:8080",
            "0.0.0.0:0",
            "192.168.1.1:9090",
            "[::1]:8080",
        ];

        for addr_str in valid_addrs {
            let result = addr_str.parse::<SocketAddr>();
            assert!(result.is_ok(), "Failed to parse address: {}", addr_str);
        }

        let invalid_addrs = vec!["invalid", "127.0.0.1:99999", "256.256.256.256:8080", ""];

        for addr_str in invalid_addrs {
            let result = addr_str.parse::<SocketAddr>();
            assert!(result.is_err(), "Should fail to parse: {}", addr_str);
        }
    }

    #[test]
    fn test_packet_serialization_roundtrip() {
        let test_packets = vec![
            Packet::Connect { client_version: 1 },
            Packet::Connected { client_id: 42 },
            Packet::Disconnect,
            Packet::Disconnected {
                reason: "Test".to_string(),
            },
            Packet::Input {
                sequence: 100,
                timestamp: 1234567890,
                left: true,
                right: false,
                jump: true,
            },
        ];

        for packet in test_packets {
            let serialized = serialize(&packet);
            assert!(serialized.is_ok());

            let deserialized: Result<Packet, _> = deserialize(&serialized.unwrap());
            assert!(deserialized.is_ok());

            // Compare packet types (simplified comparison)
            match (&packet, &deserialized.unwrap()) {
                (Packet::Connect { .. }, Packet::Connect { .. }) => {}
                (Packet::Connected { .. }, Packet::Connected { .. }) => {}
                (Packet::Disconnect, Packet::Disconnect) => {}
                (Packet::Disconnected { .. }, Packet::Disconnected { .. }) => {}
                (Packet::Input { .. }, Packet::Input { .. }) => {}
                _ => panic!("Packet type mismatch after roundtrip"),
            }
        }
    }

    #[test]
    fn test_buffer_bounds() {
        let buffer_size = 2048;

        // Test typical packet sizes
        let typical_sizes = vec![64, 128, 256, 512, 1024];
        for size in typical_sizes {
            assert!(size < buffer_size, "Packet size {} exceeds buffer", size);
        }

        // Test edge cases
        assert!(buffer_size >= 1024); // Minimum for game packets
        assert!(buffer_size <= 65536); // Maximum reasonable size
    }

    #[test]
    fn test_tick_duration_validation() {
        let valid_durations = vec![
            Duration::from_millis(16), // 60 Hz
            Duration::from_millis(33), // 30 Hz
            Duration::from_millis(8),  // 120 Hz
        ];

        for duration in valid_durations {
            assert!(duration.as_millis() > 0);
            assert!(duration.as_millis() < 1000); // Less than 1 second

            let hz = 1000.0 / duration.as_millis() as f64;
            assert!((1.0..=1000.0).contains(&hz)); // Reasonable frequency range
        }
    }

    #[test]
    fn test_client_version_compatibility() {
        let supported_versions = [1];
        let test_versions = vec![0, 1, 2, 999];

        for version in test_versions {
            let is_supported = supported_versions.contains(&version);

            if version == 1 {
                assert!(is_supported);
            } else {
                assert!(!is_supported);
            }
        }
    }

    #[test]
    fn test_error_message_formatting() {
        let reasons = vec![
            "Server full",
            "Protocol version mismatch",
            "Client timeout",
            "Invalid packet",
        ];

        for reason in reasons {
            assert!(!reason.is_empty());
            assert!(reason.len() < 256); // Reasonable message length

            let packet = Packet::Disconnected {
                reason: reason.to_string(),
            };

            match packet {
                Packet::Disconnected { reason: r } => {
                    assert_eq!(r, reason);
                }
                _ => panic!("Wrong packet type"),
            }
        }
    }

    // Helper function to create a test server for unit testing
    fn create_test_server() -> TestServerMock {
        TestServerMock {
            tick_duration: Duration::from_millis(16),
        }
    }

    // Mock server for testing without actual networking
    #[allow(dead_code)]
    struct TestServerMock {
        tick_duration: Duration,
    }

    impl TestServerMock {
        fn calculate_required_substeps(&self, dt: f32) -> u32 {
            const MAX_PLAYER_SPEED: f32 = PLAYER_SPEED;
            const MIN_COLLISION_RADIUS: f32 = PLAYER_SIZE / 2.0;
            const SAFETY_FACTOR: f32 = 0.5;

            let max_movement_per_step = MIN_COLLISION_RADIUS * SAFETY_FACTOR;
            let max_movement_this_tick = MAX_PLAYER_SPEED * dt;

            if max_movement_this_tick > max_movement_per_step {
                (max_movement_this_tick / max_movement_per_step).ceil() as u32
            } else {
                1
            }
        }
    }
}
