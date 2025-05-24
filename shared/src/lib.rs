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
