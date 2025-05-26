//! Shared data structures and utilities for networked multiplayer game

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Physics constants
pub const GRAVITY: f32 = 980.0; // pixels/second²
pub const PLAYER_SPEED: f32 = 300.0; // pixels/second
pub const JUMP_VELOCITY: f32 = -400.0; // pixels/second (negative = upward)
pub const FLOOR_Y: f32 = 550.0; // pixels from top
pub const WORLD_WIDTH: f32 = 800.0; // pixels
pub const WORLD_HEIGHT: f32 = 600.0; // pixels
pub const PLAYER_SIZE: f32 = 32.0; // pixels

/// Network packet types for client-server communication
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Packet {
    // Client → Server
    Connect {
        client_version: u32,
    },
    Input {
        sequence: u32,
        timestamp: u64,
        left: bool,
        right: bool,
        jump: bool,
    },
    Disconnect,

    // Server → Client
    Connected {
        client_id: u32,
    },
    GameState {
        tick: u32,
        timestamp: u64,
        last_processed_input: HashMap<u32, u32>,
        players: Vec<Player>,
    },
    Disconnected {
        reason: String,
    },
}

/// Player entity with position, velocity, and state
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Player {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub vel_x: f32,
    pub vel_y: f32,
    pub on_ground: bool,
}

impl Player {
    pub fn new(id: u32, x: f32, y: f32) -> Self {
        Self {
            id,
            x,
            y,
            vel_x: 0.0,
            vel_y: 0.0,
            on_ground: true,
        }
    }

    /// Returns (left, top, right, bottom) coordinates
    pub fn get_bounds(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.x + PLAYER_SIZE, self.y + PLAYER_SIZE)
    }

    /// Returns (center_x, center_y) coordinates
    pub fn center(&self) -> (f32, f32) {
        (self.x + PLAYER_SIZE / 2.0, self.y + PLAYER_SIZE / 2.0)
    }
}

/// AABB collision detection between two players
pub fn check_collision(player1: &Player, player2: &Player) -> bool {
    let (x1, y1, x2, y2) = player1.get_bounds();
    let (x3, y3, x4, y4) = player2.get_bounds();

    // No collision if any edge of one box is beyond the corresponding edge of the other
    !(x2 <= x3 || x4 <= x1 || y2 <= y3 || y4 <= y1)
}

/// Resolves collision between two players using physics-based separation and momentum exchange
pub fn resolve_collision(player1: &mut Player, player2: &mut Player) {
    if !check_collision(player1, player2) {
        return;
    }

    let (cx1, cy1) = player1.center();
    let (cx2, cy2) = player2.center();

    // Calculate direction vector from player1 to player2
    let dx = cx2 - cx1;
    let dy = cy2 - cy1;
    let distance = (dx * dx + dy * dy).sqrt();

    // Handle edge case where players are at exactly the same position
    if distance < 0.001 {
        player1.x -= PLAYER_SIZE / 2.0;
        player2.x += PLAYER_SIZE / 2.0;
        return;
    }

    // Normalize direction vector
    let nx = dx / distance;
    let ny = dy / distance;

    // Calculate overlap and separate players
    let overlap = PLAYER_SIZE - distance;

    if overlap > 0.0 {
        let separation = overlap / 2.0;
        player1.x -= nx * separation;
        player1.y -= ny * separation;
        player2.x += nx * separation;
        player2.y += ny * separation;

        // Clamp positions to world boundaries
        player1.x = player1.x.clamp(0.0, WORLD_WIDTH - PLAYER_SIZE);
        player1.y = player1.y.clamp(0.0, FLOOR_Y - PLAYER_SIZE);
        player2.x = player2.x.clamp(0.0, WORLD_WIDTH - PLAYER_SIZE);
        player2.y = player2.y.clamp(0.0, FLOOR_Y - PLAYER_SIZE);

        // Exchange velocities with damping for realistic collision response
        let temp_vx = player1.vel_x;
        let temp_vy = player1.vel_y;
        player1.vel_x = player2.vel_x * 0.8;
        player1.vel_y = player2.vel_y * 0.8;
        player2.vel_x = temp_vx * 0.8;
        player2.vel_y = temp_vy * 0.8;
    }
}

/// Input state for deterministic networked gameplay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputState {
    pub sequence: u32,  // For reliable ordering
    pub timestamp: u64, // For lag compensation
    pub left: bool,
    pub right: bool,
    pub jump: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_approx_eq::assert_approx_eq;

    #[test]
    fn test_player_creation() {
        let player = Player::new(1, 100.0, 200.0);
        assert_eq!(player.id, 1);
        assert_eq!(player.x, 100.0);
        assert_eq!(player.y, 200.0);
        assert_eq!(player.vel_x, 0.0);
        assert_eq!(player.vel_y, 0.0);
        assert!(player.on_ground);
    }

    #[test]
    fn test_collision_detection_no_collision() {
        let player1 = Player::new(1, 0.0, 0.0);
        let player2 = Player::new(2, 100.0, 100.0);
        assert!(!check_collision(&player1, &player2));
    }

    #[test]
    fn test_collision_detection_overlap() {
        let player1 = Player::new(1, 0.0, 0.0);
        let player2 = Player::new(2, 16.0, 16.0);
        assert!(check_collision(&player1, &player2));
    }

    #[test]
    fn test_collision_resolution() {
        let mut player1 = Player::new(1, 10.0, 10.0);
        let mut player2 = Player::new(2, 20.0, 20.0);

        player1.vel_x = 100.0;
        player1.vel_y = -50.0;
        player2.vel_x = -75.0;
        player2.vel_y = 25.0;

        assert!(check_collision(&player1, &player2));
        resolve_collision(&mut player1, &mut player2);

        let (cx1, cy1) = player1.center();
        let (cx2, cy2) = player2.center();
        let distance = ((cx2 - cx1).powi(2) + (cy2 - cy1).powi(2)).sqrt();

        assert!(distance >= PLAYER_SIZE * 0.9);
        assert_approx_eq!(player1.vel_x, -75.0 * 0.8, 0.01);
        assert_approx_eq!(player2.vel_x, 100.0 * 0.8, 0.01);
    }

    #[test]
    fn test_collision_detection_edge_cases() {
        // Test exact touching boundaries (should not collide)
        let player1 = Player::new(1, 0.0, 0.0);
        let player2 = Player::new(2, PLAYER_SIZE, 0.0);
        assert!(!check_collision(&player1, &player2), "Players touching edge should not collide");

        // Test 1-pixel overlap (should collide)
        let player1 = Player::new(1, 0.0, 0.0);
        let player2 = Player::new(2, PLAYER_SIZE - 1.0, 0.0);
        assert!(check_collision(&player1, &player2), "Players with 1px overlap should collide");

        // Test diagonal overlap
        let player1 = Player::new(1, 100.0, 100.0);
        let player2 = Player::new(2, 116.0, 116.0);
        assert!(check_collision(&player1, &player2), "Diagonal overlap should be detected");

        // Test complete overlap (one inside another)
        let player1 = Player::new(1, 100.0, 100.0);
        let player2 = Player::new(2, 105.0, 105.0);
        assert!(check_collision(&player1, &player2), "Complete overlap should be detected");
    }

    #[test]
    fn test_collision_resolution_zero_distance_edge_case() {
        let mut player1 = Player::new(1, 100.0, 100.0);
        let mut player2 = Player::new(2, 100.0, 100.0); // Exact same position

        player1.vel_x = 50.0;
        player2.vel_x = -30.0;

        resolve_collision(&mut player1, &mut player2);

        // Players should be separated
        assert_ne!(player1.x, player2.x, "Players at same position should be separated");
        assert!((player1.x - player2.x).abs() >= PLAYER_SIZE / 2.0, "Separation should be at least half player size");
        
        // Check they're still within world bounds
        assert!(player1.x >= 0.0 && player1.x <= WORLD_WIDTH - PLAYER_SIZE);
        assert!(player2.x >= 0.0 && player2.x <= WORLD_WIDTH - PLAYER_SIZE);
    }

    #[test]
    fn test_collision_resolution_boundary_constraints() {
        // Test collision near left boundary
        let mut player1 = Player::new(1, 0.0, 100.0);
        let mut player2 = Player::new(2, 16.0, 100.0);

        player1.vel_x = 100.0;
        player2.vel_x = -100.0;

        resolve_collision(&mut player1, &mut player2);

        assert!(player1.x >= 0.0, "Player1 should not go past left boundary");
        assert!(player2.x >= 0.0, "Player2 should not go past left boundary");

        // Test collision near right boundary
        let mut player1 = Player::new(1, WORLD_WIDTH - PLAYER_SIZE, 100.0);
        let mut player2 = Player::new(2, WORLD_WIDTH - PLAYER_SIZE - 16.0, 100.0);

        player1.vel_x = -100.0;
        player2.vel_x = 100.0;

        resolve_collision(&mut player1, &mut player2);

        assert!(player1.x <= WORLD_WIDTH - PLAYER_SIZE, "Player1 should not go past right boundary");
        assert!(player2.x <= WORLD_WIDTH - PLAYER_SIZE, "Player2 should not go past right boundary");
    }

    #[test]
    fn test_collision_resolution_momentum_conservation() {
        let mut player1 = Player::new(1, 100.0, 100.0);
        let mut player2 = Player::new(2, 116.0, 100.0);

        player1.vel_x = 200.0;
        player1.vel_y = 100.0;
        player2.vel_x = -150.0;
        player2.vel_y = -75.0;

        let initial_momentum_x = player1.vel_x + player2.vel_x;
        let initial_momentum_y = player1.vel_y + player2.vel_y;

        resolve_collision(&mut player1, &mut player2);

        // Check momentum is approximately conserved (with damping factor 0.8)
        let final_momentum_x = player1.vel_x + player2.vel_x;
        let final_momentum_y = player1.vel_y + player2.vel_y;
        
        assert_approx_eq!(final_momentum_x, initial_momentum_x * 0.8, 0.01);
        assert_approx_eq!(final_momentum_y, initial_momentum_y * 0.8, 0.01);
    }

    #[test]
    fn test_player_bounds_calculation() {
        let player = Player::new(1, 150.0, 200.0);
        let (left, top, right, bottom) = player.get_bounds();
        
        assert_eq!(left, 150.0);
        assert_eq!(top, 200.0);
        assert_eq!(right, 150.0 + PLAYER_SIZE);
        assert_eq!(bottom, 200.0 + PLAYER_SIZE);
    }

    #[test]
    fn test_player_center_calculation() {
        let player = Player::new(1, 100.0, 200.0);
        let (cx, cy) = player.center();
        
        assert_eq!(cx, 100.0 + PLAYER_SIZE / 2.0);
        assert_eq!(cy, 200.0 + PLAYER_SIZE / 2.0);
    }

    #[test]
    fn test_input_state_validation() {
        let input = InputState {
            sequence: u32::MAX,
            timestamp: u64::MAX,
            left: true,
            right: true, // Contradictory input - should be allowed
            jump: true,
        };

        // Input state should handle extreme values gracefully
        assert_eq!(input.sequence, u32::MAX);
        assert_eq!(input.timestamp, u64::MAX);
        assert!(input.left && input.right); // Both directions can be pressed
    }

    #[test]
    fn test_packet_serialization_all_variants() {
        // Test Connect packet
        let connect = Packet::Connect { client_version: 42 };
        let serialized = bincode::serialize(&connect).unwrap();
        let deserialized: Packet = bincode::deserialize(&serialized).unwrap();
        match deserialized {
            Packet::Connect { client_version } => assert_eq!(client_version, 42),
            _ => panic!("Wrong packet type"),
        }

        // Test Input packet with extreme values
        let input = Packet::Input {
            sequence: u32::MAX,
            timestamp: u64::MAX,
            left: true,
            right: false,
            jump: true,
        };
        let serialized = bincode::serialize(&input).unwrap();
        let deserialized: Packet = bincode::deserialize(&serialized).unwrap();
        match deserialized {
            Packet::Input { sequence, timestamp, left, right, jump } => {
                assert_eq!(sequence, u32::MAX);
                assert_eq!(timestamp, u64::MAX);
                assert!(left);
                assert!(!right);
                assert!(jump);
            },
            _ => panic!("Wrong packet type"),
        }

        // Test GameState packet with multiple players
        let mut players = Vec::new();
        for i in 0..10 {
            let mut player = Player::new(i, i as f32 * 50.0, 100.0);
            player.vel_x = i as f32 * 10.0;
            player.vel_y = -(i as f32) * 5.0;
            player.on_ground = i % 2 == 0;
            players.push(player);
        }

        let mut last_processed = HashMap::new();
        for i in 0..10 {
            last_processed.insert(i, i * 100);
        }

        let game_state = Packet::GameState {
            tick: 12345,
            timestamp: 9876543210,
            last_processed_input: last_processed,
            players: players.clone(),
        };

        let serialized = bincode::serialize(&game_state).unwrap();
        let deserialized: Packet = bincode::deserialize(&serialized).unwrap();
        
        match deserialized {
            Packet::GameState { tick, timestamp, last_processed_input, players: deserialized_players } => {
                assert_eq!(tick, 12345);
                assert_eq!(timestamp, 9876543210);
                assert_eq!(last_processed_input.len(), 10);
                assert_eq!(deserialized_players.len(), 10);
                
                for (original, deserialized) in players.iter().zip(deserialized_players.iter()) {
                    assert_eq!(original.id, deserialized.id);
                    assert_approx_eq!(original.x, deserialized.x, 0.001);
                    assert_approx_eq!(original.y, deserialized.y, 0.001);
                    assert_eq!(original.on_ground, deserialized.on_ground);
                }
            },
            _ => panic!("Wrong packet type"),
        }
    }

    #[test]
    fn test_physics_constants_validity() {
        // Ensure physics constants are reasonable
        assert!(GRAVITY > 0.0, "Gravity should be positive (downward)");
        assert!(PLAYER_SPEED > 0.0, "Player speed should be positive");
        assert!(JUMP_VELOCITY < 0.0, "Jump velocity should be negative (upward)");
        assert!(FLOOR_Y > PLAYER_SIZE, "Floor should be below player size");
        assert!(WORLD_WIDTH > PLAYER_SIZE * 2.0, "World should fit at least 2 players");
        assert!(PLAYER_SIZE > 0.0, "Player size should be positive");
        
        // Test that a player can jump and come back down
        let jump_height = (JUMP_VELOCITY * JUMP_VELOCITY) / (2.0 * GRAVITY);
        assert!(jump_height > PLAYER_SIZE, "Jump height should be meaningful");
        assert!(jump_height < WORLD_HEIGHT / 2.0, "Jump shouldn't be too high");
    }

    #[test]
    fn test_collision_with_multiple_players() {
        // Test collision chain: Player1 -> Player2 -> Player3
        let mut player1 = Player::new(1, 100.0, 100.0);
        let mut player2 = Player::new(2, 116.0, 100.0);
        let mut player3 = Player::new(3, 132.0, 100.0);

        player1.vel_x = 300.0;
        player2.vel_x = 0.0;
        player3.vel_x = 0.0;

        // Resolve collision between 1 and 2
        resolve_collision(&mut player1, &mut player2);
        
        let vel2_after_first = player2.vel_x;
        assert!(vel2_after_first > 0.0, "Player2 should gain velocity from collision");

        // Now resolve collision between 2 and 3
        resolve_collision(&mut player2, &mut player3);
        
        assert!(player3.vel_x > 0.0, "Player3 should gain velocity from chain collision");
        assert!(player2.vel_x < vel2_after_first, "Player2 should lose some velocity in second collision");
    }

    #[test]
    fn test_collision_resolution_no_collision() {
        let mut player1 = Player::new(1, 0.0, 0.0);
        let mut player2 = Player::new(2, 100.0, 100.0);

        let original_state1 = player1.clone();
        let original_state2 = player2.clone();

        resolve_collision(&mut player1, &mut player2);

        // Players should remain unchanged if no collision
        assert_eq!(player1.x, original_state1.x);
        assert_eq!(player1.y, original_state1.y);
        assert_eq!(player1.vel_x, original_state1.vel_x);
        assert_eq!(player1.vel_y, original_state1.vel_y);
        
        assert_eq!(player2.x, original_state2.x);
        assert_eq!(player2.y, original_state2.y);
        assert_eq!(player2.vel_x, original_state2.vel_x);
        assert_eq!(player2.vel_y, original_state2.vel_y);
    }
}
