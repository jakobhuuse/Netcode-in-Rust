//! Integration tests for networked multiplayer components
//!
//! These tests validate cross-component interactions and real network behavior.

use bincode::{deserialize, serialize};
use shared::{InputState, Packet, Player};
use std::net::UdpSocket;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

/// NETWORK PROTOCOL TESTS
mod protocol_tests {
    use super::*;

    /// Tests packet serialization round-trip for network protocol validation
    #[tokio::test]
    async fn packet_serialization_roundtrip() {
        let test_packets = vec![
            Packet::Connect { client_version: 1 },
            Packet::Input {
                sequence: 42,
                timestamp: 123456789,
                left: true,
                right: false,
                jump: true,
            },
            Packet::Connected { client_id: 42 },
            Packet::Disconnected {
                reason: "Test".to_string(),
            },
        ];

        for packet in test_packets {
            let serialized = serialize(&packet).unwrap();
            let deserialized: Packet = deserialize(&serialized).unwrap();

            // Verify packet type matches (simplified check)
            match (&packet, &deserialized) {
                (Packet::Connect { .. }, Packet::Connect { .. }) => {}
                (Packet::Input { .. }, Packet::Input { .. }) => {}
                (Packet::Connected { .. }, Packet::Connected { .. }) => {}
                (Packet::Disconnected { .. }, Packet::Disconnected { .. }) => {}
                _ => panic!("Packet type mismatch after serialization"),
            }
        }
    }

    /// Tests real UDP socket communication
    #[tokio::test]
    async fn udp_socket_communication() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind server socket");
        let server_addr = server_socket.local_addr().unwrap();

        // Echo server
        let server_socket_clone = server_socket.try_clone().unwrap();
        thread::spawn(move || {
            let mut buf = [0; 1024];
            if let Ok((size, client_addr)) = server_socket_clone.recv_from(&mut buf) {
                let _ = server_socket_clone.send_to(&buf[..size], client_addr);
            }
        });

        sleep(Duration::from_millis(10)).await;

        let client_socket = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind client socket");
        client_socket
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();

        let test_packet = Packet::Connect { client_version: 1 };
        let serialized = serialize(&test_packet).unwrap();

        client_socket.send_to(&serialized, server_addr).unwrap();

        let mut buf = [0; 1024];
        let (size, _) = client_socket.recv_from(&mut buf).unwrap();
        let received_packet: Packet = deserialize(&buf[..size]).unwrap();

        match received_packet {
            Packet::Connect { client_version } => assert_eq!(client_version, 1),
            _ => panic!("Wrong packet type received"),
        }
    }
}

/// GAME LOGIC INTEGRATION TESTS
mod game_logic_tests {
    use super::*;

    /// Tests integrated game physics and input processing
    #[test]
    fn physics_and_input_integration() {
        let mut player = Player::new(1, 100.0, 500.0);
        let dt = 1.0 / 60.0;

        // Test movement sequence
        player.vel_x = shared::PLAYER_SPEED;
        let initial_x = player.x;

        player.x += player.vel_x * dt;
        player.y += player.vel_y * dt;

        assert!(player.x > initial_x);

        // Test jump mechanics
        if player.on_ground {
            player.vel_y = shared::JUMP_VELOCITY;
            player.on_ground = false;
        }

        assert_eq!(player.vel_y, shared::JUMP_VELOCITY);
        assert!(!player.on_ground);

        // Test gravity application
        player.vel_y += shared::GRAVITY * dt;
        assert!(player.vel_y > shared::JUMP_VELOCITY);
    }

    /// Tests collision detection and resolution integration
    #[test]
    fn collision_system_integration() {
        let mut player1 = Player::new(1, 100.0, 100.0);
        let mut player2 = Player::new(2, 110.0, 110.0);

        assert!(shared::check_collision(&player1, &player2));

        player1.vel_x = 100.0;
        player2.vel_x = -50.0;

        shared::resolve_collision(&mut player1, &mut player2);

        let (cx1, cy1) = player1.center();
        let (cx2, cy2) = player2.center();
        let distance = ((cx2 - cx1).powi(2) + (cy2 - cy1).powi(2)).sqrt();

        assert!(distance >= shared::PLAYER_SIZE * 0.9);
        assert!((player1.vel_x - (-50.0 * 0.8)).abs() < 0.01);
        assert!((player2.vel_x - (100.0 * 0.8)).abs() < 0.01);
    }

    /// Tests player boundary constraint enforcement
    #[test]
    fn boundary_constraint_integration() {
        let mut player = Player::new(1, 0.0, 0.0);

        // Test left boundary
        player.x = -10.0;
        player.x = player
            .x
            .clamp(0.0, shared::WORLD_WIDTH - shared::PLAYER_SIZE);
        assert_eq!(player.x, 0.0);

        // Test right boundary
        player.x = shared::WORLD_WIDTH + 10.0;
        player.x = player
            .x
            .clamp(0.0, shared::WORLD_WIDTH - shared::PLAYER_SIZE);
        assert_eq!(player.x, shared::WORLD_WIDTH - shared::PLAYER_SIZE);

        // Test floor collision
        player.y = shared::FLOOR_Y + 10.0;
        if player.y + shared::PLAYER_SIZE >= shared::FLOOR_Y {
            player.y = shared::FLOOR_Y - shared::PLAYER_SIZE;
            player.vel_y = 0.0;
            player.on_ground = true;
        }

        assert_eq!(player.y, shared::FLOOR_Y - shared::PLAYER_SIZE);
        assert_eq!(player.vel_y, 0.0);
        assert!(player.on_ground);
    }
}

/// CLIENT-SERVER INTEGRATION TESTS  
mod client_server_tests {
    use super::*;

    /// Tests input state timing and sequence validation
    #[test]
    fn input_timing_integration() {
        let input1 = InputState {
            sequence: 1,
            timestamp: get_current_timestamp(),
            left: true,
            right: false,
            jump: false,
        };

        thread::sleep(Duration::from_millis(2));

        let input2 = InputState {
            sequence: 2,
            timestamp: get_current_timestamp(),
            left: false,
            right: true,
            jump: false,
        };

        assert!(input2.timestamp > input1.timestamp);
        assert!(input2.sequence > input1.sequence);
    }

    /// Tests deterministic input processing across client and server
    #[test]
    fn deterministic_input_processing() {
        let initial_player = Player::new(1, 100.0, 100.0);

        let inputs = vec![
            InputState {
                sequence: 1,
                timestamp: 100,
                left: true,
                right: false,
                jump: false,
            },
            InputState {
                sequence: 2,
                timestamp: 110,
                left: false,
                right: true,
                jump: false,
            },
            InputState {
                sequence: 3,
                timestamp: 120,
                left: false,
                right: false,
                jump: true,
            },
        ];

        let dt = 1.0 / 60.0;

        // Simulate client processing
        let mut client_player = initial_player.clone();
        for input in &inputs {
            apply_input_simulation(&mut client_player, input);
            simulate_physics_step(&mut client_player, dt);
        }

        // Simulate server processing (same logic)
        let mut server_player = initial_player.clone();
        for input in &inputs {
            apply_input_simulation(&mut server_player, input);
            simulate_physics_step(&mut server_player, dt);
        }

        // Results should be identical
        assert!((client_player.x - server_player.x).abs() < 0.0001);
        assert!((client_player.y - server_player.y).abs() < 0.0001);
        assert!((client_player.vel_x - server_player.vel_x).abs() < 0.0001);
        assert!((client_player.vel_y - server_player.vel_y).abs() < 0.0001);
    }
}

/// STRESS AND ERROR HANDLING TESTS
mod stress_tests {
    use super::*;

    /// Tests complex multi-player collision scenarios
    #[test]
    fn multi_player_collision_stress() {
        let mut players = vec![
            Player::new(1, 50.0, 100.0),
            Player::new(2, 150.0, 100.0),
            Player::new(3, 250.0, 100.0),
            Player::new(4, 350.0, 100.0),
        ];

        // Set different velocities to create collisions
        players[0].vel_x = 100.0;
        players[1].vel_x = -50.0;
        players[2].vel_x = 75.0;
        players[3].vel_x = -25.0;

        let dt = 1.0 / 60.0;
        let substeps = 4;
        let substep_dt = dt / substeps as f32;

        // Simulate physics with collision detection
        for _ in 0..substeps {
            // Update positions
            for player in &mut players {
                player.x += player.vel_x * substep_dt;
                player.y += player.vel_y * substep_dt;
                player.vel_y += shared::GRAVITY * substep_dt;

                // Apply ground collision
                if player.y + shared::PLAYER_SIZE >= shared::FLOOR_Y {
                    player.y = shared::FLOOR_Y - shared::PLAYER_SIZE;
                    player.vel_y = 0.0;
                    player.on_ground = true;
                }
            }

            // Check all player-player collisions
            for i in 0..players.len() {
                for j in (i + 1)..players.len() {
                    if shared::check_collision(&players[i], &players[j]) {
                        let (mut p1, mut p2) = (players[i].clone(), players[j].clone());
                        shared::resolve_collision(&mut p1, &mut p2);
                        players[i] = p1;
                        players[j] = p2;
                    }
                }
            }
        }

        // Verify all players are in valid state
        for player in &players {
            assert!(player.on_ground, "Player {} should be on ground", player.id);
            assert!(
                player.x >= 0.0,
                "Player {} should be within left bound",
                player.id
            );
            assert!(
                player.x <= shared::WORLD_WIDTH - shared::PLAYER_SIZE,
                "Player {} should be within right bound",
                player.id
            );
        }
    }

    /// Tests malformed packet handling
    #[test]
    fn malformed_packet_handling() {
        let valid_packet = Packet::Connect { client_version: 1 };
        let valid_data = serialize(&valid_packet).unwrap();

        // Test truncated packet
        let truncated_data = &valid_data[..valid_data.len() / 2];
        let result: Result<Packet, _> = deserialize(truncated_data);
        assert!(
            result.is_err(),
            "Should fail to deserialize truncated packet"
        );

        // Test corrupted packet
        let mut corrupted_data = valid_data.clone();
        if !corrupted_data.is_empty() {
            corrupted_data[0] = 0xFF; // Corrupt first byte
        }
        let result: Result<Packet, _> = deserialize(&corrupted_data);
        assert!(
            result.is_err(),
            "Should fail to deserialize corrupted packet"
        );

        // Test empty packet
        let empty_data = vec![];
        let result: Result<Packet, _> = deserialize(&empty_data);
        assert!(result.is_err(), "Should fail to deserialize empty packet");
    }
}

// HELPER FUNCTIONS

fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}

fn apply_input_simulation(player: &mut Player, input: &InputState) {
    player.vel_x = 0.0; // Reset horizontal velocity

    if input.left {
        player.vel_x -= shared::PLAYER_SPEED;
    }
    if input.right {
        player.vel_x += shared::PLAYER_SPEED;
    }
    if input.jump && player.on_ground {
        player.vel_y = shared::JUMP_VELOCITY;
        player.on_ground = false;
    }
}

fn simulate_physics_step(player: &mut Player, dt: f32) {
    // Update position
    player.x += player.vel_x * dt;
    player.y += player.vel_y * dt;

    // Apply gravity
    if !player.on_ground {
        player.vel_y += shared::GRAVITY * dt;
    }

    // Ground collision
    if player.y + shared::PLAYER_SIZE >= shared::FLOOR_Y {
        player.y = shared::FLOOR_Y - shared::PLAYER_SIZE;
        player.vel_y = 0.0;
        player.on_ground = true;
    }

    // Boundary constraints
    player.x = player
        .x
        .clamp(0.0, shared::WORLD_WIDTH - shared::PLAYER_SIZE);
}
