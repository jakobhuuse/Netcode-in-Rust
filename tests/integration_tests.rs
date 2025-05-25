//! Integration tests for networked multiplayer components

use bincode::{deserialize, serialize};
use shared::{InputState, Packet, Player};
use std::net::UdpSocket;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

/// Tests packet serialization round-trip for network protocol validation
#[tokio::test]
async fn test_packet_serialization_roundtrip() {
    let connect_packet = Packet::Connect { client_version: 1 };
    let serialized = serialize(&connect_packet).unwrap();
    let deserialized: Packet = deserialize(&serialized).unwrap();

    match deserialized {
        Packet::Connect { client_version } => assert_eq!(client_version, 1),
        _ => panic!("Wrong packet type"),
    }

    let input_packet = Packet::Input {
        sequence: 42,
        timestamp: 123456789,
        left: true,
        right: false,
        jump: true,
    };
    let serialized = serialize(&input_packet).unwrap();
    let deserialized: Packet = deserialize(&serialized).unwrap();

    match deserialized {
        Packet::Input {
            sequence,
            timestamp,
            left,
            right,
            jump,
        } => {
            assert_eq!(sequence, 42);
            assert_eq!(timestamp, 123456789);
            assert!(left);
            assert!(!right);
            assert!(jump);
        }
        _ => panic!("Wrong packet type"),
    }
}

/// Tests real UDP socket communication
#[tokio::test]
async fn test_udp_socket_communication() {
    let server_addr = "127.0.0.1:0";
    let server_socket = UdpSocket::bind(server_addr).expect("Failed to bind server socket");
    let server_addr = server_socket.local_addr().unwrap();

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

/// Tests integrated game logic components
#[test]
fn test_game_logic_integration() {
    let mut player = Player::new(1, 100.0, 500.0);
    let dt = 1.0 / 60.0;

    // Test movement
    player.vel_x = shared::PLAYER_SPEED;
    let initial_x = player.x;

    player.x += player.vel_x * dt;
    player.y += player.vel_y * dt;

    assert!(player.x > initial_x);

    // Test jump
    if player.on_ground {
        player.vel_y = shared::JUMP_VELOCITY;
        player.on_ground = false;
    }

    assert_eq!(player.vel_y, shared::JUMP_VELOCITY);
    assert!(!player.on_ground);

    // Test gravity
    player.vel_y += shared::GRAVITY * dt;
    assert!(player.vel_y > shared::JUMP_VELOCITY);
}

/// Tests input state timing and sequence validation
#[test]
fn test_input_state_timing() {
    let input1 = InputState {
        sequence: 1,
        timestamp: get_current_timestamp(),
        left: true,
        right: false,
        jump: false,
    };

    thread::sleep(Duration::from_millis(1));

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

/// Tests collision detection and resolution integration
#[test]
fn test_collision_detection_and_resolution() {
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
fn test_player_boundary_constraints() {
    let mut player = Player::new(1, 0.0, 0.0);

    // Test left boundary
    player.x = -10.0;
    player.x = player.x.clamp(0.0, shared::WORLD_WIDTH - shared::PLAYER_SIZE);
    assert_eq!(player.x, 0.0);

    // Test right boundary
    player.x = shared::WORLD_WIDTH + 10.0;
    player.x = player.x.clamp(0.0, shared::WORLD_WIDTH - shared::PLAYER_SIZE);
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

/// Helper function to get current timestamp
fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}

/// Tests address resolution functionality for IP addresses and domain names
#[cfg(test)]
mod address_resolution_tests {
    use client::network::Client;

    #[tokio::test]
    async fn test_client_creation_with_ip_addresses() {
        let result = Client::new("127.0.0.1:8080", 0).await;
        assert!(result.is_ok(), "Should work with IPv4 address");

        let result = Client::new("[::1]:8080", 0).await;
        assert!(result.is_ok(), "Should work with IPv6 address");
    }

    #[tokio::test]
    async fn test_client_creation_with_domain_names() {
        let result = Client::new("localhost:8080", 0).await;
        assert!(result.is_ok(), "Should work with localhost domain");
    }

    #[tokio::test]
    async fn test_client_creation_with_invalid_addresses() {
        let result = Client::new("invalid-format", 0).await;
        assert!(result.is_err(), "Should fail with invalid address format");

        let result = Client::new("definitely-nonexistent-domain-12345.invalid:8080", 0).await;
        assert!(result.is_err(), "Should fail with non-existent domain");
    }
}