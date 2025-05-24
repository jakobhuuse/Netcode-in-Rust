use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const GRAVITY: f32 = 980.0;
pub const PLAYER_SPEED: f32 = 300.0;
pub const JUMP_VELOCITY: f32 = -400.0;
pub const FLOOR_Y: f32 = 550.0;
pub const WORLD_WIDTH: f32 = 800.0;
pub const WORLD_HEIGHT: f32 = 600.0;
pub const PLAYER_SIZE: f32 = 32.0;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Packet {
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

    pub fn get_bounds(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.x + PLAYER_SIZE, self.y + PLAYER_SIZE)
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + PLAYER_SIZE / 2.0, self.y + PLAYER_SIZE / 2.0)
    }
}

pub fn check_collision(player1: &Player, player2: &Player) -> bool {
    let (x1, y1, x2, y2) = player1.get_bounds();
    let (x3, y3, x4, y4) = player2.get_bounds();

    !(x2 <= x3 || x4 <= x1 || y2 <= y3 || y4 <= y1)
}

pub fn resolve_collision(player1: &mut Player, player2: &mut Player) {
    if !check_collision(player1, player2) {
        return;
    }

    let (cx1, cy1) = player1.center();
    let (cx2, cy2) = player2.center();

    let dx = cx2 - cx1;
    let dy = cy2 - cy1;
    let distance = (dx * dx + dy * dy).sqrt();

    if distance < 0.001 {
        player1.x -= PLAYER_SIZE / 2.0;
        player2.x += PLAYER_SIZE / 2.0;
        return;
    }

    let nx = dx / distance;
    let ny = dy / distance;

    let overlap = PLAYER_SIZE - distance;

    if overlap > 0.0 {
        let separation = overlap / 2.0;
        player1.x -= nx * separation;
        player1.y -= ny * separation;
        player2.x += nx * separation;
        player2.y += ny * separation;

        player1.x = player1.x.clamp(0.0, WORLD_WIDTH - PLAYER_SIZE);
        player1.y = player1.y.clamp(0.0, FLOOR_Y - PLAYER_SIZE);
        player2.x = player2.x.clamp(0.0, WORLD_WIDTH - PLAYER_SIZE);
        player2.y = player2.y.clamp(0.0, FLOOR_Y - PLAYER_SIZE);

        let temp_vx = player1.vel_x;
        let temp_vy = player1.vel_y;
        player1.vel_x = player2.vel_x * 0.8;
        player1.vel_y = player2.vel_y * 0.8;
        player2.vel_x = temp_vx * 0.8;
        player2.vel_y = temp_vy * 0.8;
    }
}

#[derive(Debug, Clone)]
pub struct InputState {
    pub sequence: u32,
    pub timestamp: u64,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_approx_eq::assert_approx_eq;
    use std::collections::HashMap;

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
    fn test_player_bounds() {
        let player = Player::new(1, 50.0, 75.0);
        let (x1, y1, x2, y2) = player.get_bounds();
        assert_eq!(x1, 50.0);
        assert_eq!(y1, 75.0);
        assert_eq!(x2, 50.0 + PLAYER_SIZE);
        assert_eq!(y2, 75.0 + PLAYER_SIZE);
    }

    #[test]
    fn test_player_center() {
        let player = Player::new(1, 100.0, 200.0);
        let (cx, cy) = player.center();
        assert_eq!(cx, 100.0 + PLAYER_SIZE / 2.0);
        assert_eq!(cy, 200.0 + PLAYER_SIZE / 2.0);
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
    fn test_collision_detection_exact_touch() {
        let player1 = Player::new(1, 0.0, 0.0);
        let player2 = Player::new(2, PLAYER_SIZE, 0.0);
        assert!(!check_collision(&player1, &player2));
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
        assert_approx_eq!(player1.vel_y, 25.0 * 0.8, 0.01);
        assert_approx_eq!(player2.vel_x, 100.0 * 0.8, 0.01);
        assert_approx_eq!(player2.vel_y, -50.0 * 0.8, 0.01);
    }

    #[test]
    fn test_collision_resolution_same_position() {
        let mut player1 = Player::new(1, 10.0, 10.0);
        let mut player2 = Player::new(2, 10.0, 10.0);

        resolve_collision(&mut player1, &mut player2);

        assert!(!check_collision(&player1, &player2));
        assert_ne!(player1.x, player2.x);
    }

    #[test]
    fn test_packet_serialization_connect() {
        let packet = Packet::Connect { client_version: 42 };
        let serialized = bincode::serialize(&packet).unwrap();
        let deserialized: Packet = bincode::deserialize(&serialized).unwrap();

        match deserialized {
            Packet::Connect { client_version } => assert_eq!(client_version, 42),
            _ => panic!("Wrong packet type after deserialization"),
        }
    }

    #[test]
    fn test_packet_serialization_input() {
        let packet = Packet::Input {
            sequence: 123,
            timestamp: 456789,
            left: true,
            right: false,
            jump: true,
        };

        let serialized = bincode::serialize(&packet).unwrap();
        let deserialized: Packet = bincode::deserialize(&serialized).unwrap();

        match deserialized {
            Packet::Input {
                sequence,
                timestamp,
                left,
                right,
                jump,
            } => {
                assert_eq!(sequence, 123);
                assert_eq!(timestamp, 456789);
                assert!(left);
                assert!(!right);
                assert!(jump);
            }
            _ => panic!("Wrong packet type after deserialization"),
        }
    }

    #[test]
    fn test_packet_serialization_game_state() {
        let players = vec![Player::new(1, 100.0, 200.0), Player::new(2, 300.0, 400.0)];

        let mut last_processed_input = HashMap::new();
        last_processed_input.insert(1, 10);
        last_processed_input.insert(2, 15);

        let packet = Packet::GameState {
            tick: 42,
            timestamp: 123456789,
            last_processed_input,
            players,
        };

        let serialized = bincode::serialize(&packet).unwrap();
        let deserialized: Packet = bincode::deserialize(&serialized).unwrap();

        match deserialized {
            Packet::GameState {
                tick,
                timestamp,
                last_processed_input,
                players,
            } => {
                assert_eq!(tick, 42);
                assert_eq!(timestamp, 123456789);
                assert_eq!(last_processed_input.get(&1), Some(&10));
                assert_eq!(last_processed_input.get(&2), Some(&15));
                assert_eq!(players.len(), 2);
                assert_eq!(players[0].id, 1);
                assert_eq!(players[1].id, 2);
            }
            _ => panic!("Wrong packet type after deserialization"),
        }
    }

    #[test]
    fn test_input_state_creation() {
        let input = InputState {
            sequence: 42,
            timestamp: 123456,
            left: true,
            right: false,
            jump: true,
        };

        assert_eq!(input.sequence, 42);
        assert_eq!(input.timestamp, 123456);
        assert!(input.left);
        assert!(!input.right);
        assert!(input.jump);
    }
}
