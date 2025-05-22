use serde::Serialize;

use crate::physics::{Object, Vector2};
use std::str::FromStr;

//For returning positions as JSON
#[derive(Serialize)]
pub struct PlayerPosition {
    id: usize,
    position: Vector2,
}

#[derive(Debug, Clone, Default)]
pub struct PlayerInputState {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    seq: u32,
}

impl FromStr for PlayerInputState {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut left = false;
        let mut right = false;
        let mut up = false;
        let mut down = false;
        let mut seq = 0;

        //parse input that is in the format "left=1 right=0 ..."
        for part in s.split_whitespace() {
            let mut kv = part.split('=');
            match (kv.next(), kv.next()) {
                (Some("left"), Some(v)) => left = v == "1",
                (Some("right"), Some(v)) => right = v == "1",
                (Some("up"), Some(v)) => up = v == "1",
                (Some("down"), Some(v)) => down = v == "1",
                (Some("seq"), Some(v)) => seq = v.parse().unwrap_or(0),
                _ => {}
            }
        }
        Ok(PlayerInputState {
            left,
            right,
            up,
            down,
            seq,
        })
    }
}

pub struct Player {
    id: usize,
    last_seq: u32,
    object: Object,
    acceleration_speed: f32,
    jump_speed: f32,
    input_state: PlayerInputState,
}

pub struct GameState {
    players: Vec<Player>,
    objects: Vec<Object>,
}

impl GameState {
    pub fn new() -> Self {
        GameState {
            players: Vec::new(),
            objects: Vec::new(),
        }
    }

    pub fn add_player(&mut self, id: usize) {
        let object = Object {
            position: Vector2 { x: 0.0, y: 0.0 },
            velocity: Vector2 { x: 0.0, y: 0.0 },
            acceleration: Vector2 { x: 0.0, y: 0.0 },
            max_speed: 10.0,
            gravity: 9.81,
        };
        let player = Player {
            id,
            last_seq: 0,
            object,
            acceleration_speed: 10.0,
            jump_speed: 20.0,
            input_state: PlayerInputState::default(),
        };
        self.players.push(player);
    }

    pub fn remove_player(&mut self, id: usize) {
        self.players.retain(|p| p.id != id);
    }

    pub fn get_player_positions(&self) -> Vec<PlayerPosition> {
        self.players
            .iter()
            .map(|p| PlayerPosition {
                id: p.id,
                position: p.object.position,
            })
            .collect()
    }

    pub fn get_player_seqs(&self) -> Vec<(usize, u32)> {
        self.players.iter().map(|p| (p.id, p.last_seq)).collect()
    }

    pub fn set_player_max_speed(&mut self, id: usize, speed: f32) {
        if let Some(player) = self.players.iter_mut().find(|p| p.id == id) {
            player.object.max_speed = speed;
        }
    }

    pub fn set_player_gravity(&mut self, id: usize, gravity: f32) {
        if let Some(player) = self.players.iter_mut().find(|p| p.id == id) {
            player.object.gravity = gravity;
        }
    }

    pub fn set_player_acceleration_speed(&mut self, id: usize, speed: f32) {
        if let Some(player) = self.players.iter_mut().find(|p| p.id == id) {
            player.acceleration_speed = speed;
        }
    }

    pub fn set_player_jump_speed(&mut self, id: usize, speed: f32) {
        if let Some(player) = self.players.iter_mut().find(|p| p.id == id) {
            player.jump_speed = speed;
        }
    }

    pub fn update_player_input(&mut self, id: usize, input: PlayerInputState) {
        if let Some(player) = self.players.iter_mut().find(|p| p.id == id) {
            // Only update if seq is newer
            if input.seq > player.last_seq {
                player.input_state = input.clone();
                player.last_seq = input.seq;
            }
        }
    }

    pub fn update_positions(&mut self, dt: f32) {
        for player in &mut self.players {
            //Horizontal movement
            //Left input
            if player.input_state.left && !player.input_state.right {
                player.object.acceleration.x = -player.acceleration_speed;
            //Right input
            } else if player.input_state.right && !player.input_state.left {
                player.object.acceleration.x = player.acceleration_speed;

            //No input
            } else {
                player.object.acceleration.x = 0.0;
                player.object.velocity.x = 0.0;
            }

            //Vertical movement
            // Jump if on ground and up pressed (and not already moving up)
            if player.input_state.up
                && !player.input_state.down
                && player.object.position.y <= 0.0
                && player.object.velocity.y <= 0.0
            {
                player.object.velocity.y = player.jump_speed;
            }
            // Stop moving upward if up is released
            else if !player.input_state.up && player.object.velocity.y > 0.0 {
                player.object.velocity.y = 0.0;
            }

            player.object.simulate(dt);
        }
        for object in &mut self.objects {
            object.simulate(dt);
        }
    }
}
