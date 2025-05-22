use crate::physics::{Object, Vector2};
use std::str::FromStr;
#[derive(Debug)]
pub enum PlayerAction {
    Halt,
    Left,
    Right,
    Jump,
}

impl FromStr for PlayerAction {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "halt" => Ok(PlayerAction::Halt),
            "left" => Ok(PlayerAction::Left),
            "right" => Ok(PlayerAction::Right),
            "jump" => Ok(PlayerAction::Jump),
            _ => Err(()),
        }
    }
}

pub struct Player {
    id: usize,
    last_seq: u32,
    object: Object,
    acceleration_speed: f32,
    jump_speed: f32,
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
            object: object,
            acceleration_speed: 10.0,
            jump_speed: 20.0,
        };
        self.players.push(player);
    }

    pub fn remove_player(&mut self, id: usize) {
        self.players.retain(|p| p.id != id);
    }

    pub fn get_player_positions(&self) -> Vec<(usize, Vector2)> {
        self.players
            .iter()
            .map(|p| (p.id, p.object.position))
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

    pub fn player_action(&mut self, id: usize, action: PlayerAction, seq: u32) {
        if let Some(player) = self.players.iter_mut().find(|p| p.id == id) {
            // Only process if seq is newer
            if seq > player.last_seq {
                println!(
                    "Executing action {:?} for player {} (seq {})",
                    action, id, seq
                );
                match action {
                    PlayerAction::Halt => {
                        player.object.velocity.x = 0.0;
                        player.object.acceleration.x = 0.0;
                    }
                    PlayerAction::Left => {
                        player.object.acceleration.x = -player.acceleration_speed;
                    }
                    PlayerAction::Right => {
                        player.object.acceleration.x = player.acceleration_speed;
                    }
                    PlayerAction::Jump => {
                        if player.object.position.y <= 0.0 && player.object.velocity.y <= 0.0 {
                            player.object.velocity.y = player.jump_speed;
                        }
                    }
                }
                player.last_seq = seq;
            } else {
                println!(
                    "Ignored out-of-order action for player {}: seq {} (last_seq {})",
                    id, seq, player.last_seq
                );
            }
        }
    }

    pub fn update_positions(&mut self, dt: f32) {
        for player in &mut self.players {
            player.object.simulate(dt);
        }
        for object in &mut self.objects {
            object.simulate(dt);
        }
    }
}
