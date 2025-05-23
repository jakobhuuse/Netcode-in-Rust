use serde::Serialize;

use crate::physics::{DynamicObject, Object, Vector2};
use std::{str::FromStr, usize};

/// Enum that speicifies what type of object it is.
/// Used for serializing objects before sending them to the client.
#[derive(Serialize)]
pub enum ObjectType {
    Player,
    Static,
}

/// Struct that specifies an object along with its type.
/// Used for serializing data before sending them to the client.
#[derive(Serialize)]
pub struct ObjectInfo {
    object_type: ObjectType,
    object: Object,
}

/// Struct that specifies info regarding sequence numbers.
/// Used for serializing data before sending them to the client.
#[derive(Serialize)]
pub struct SeqInfo {
    last_seq: u32,
}

/// Struct that describes the input state of an player.
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

    /// Creates a PlayerInputState from string.
    /// Parses input in the format "left=<0|1> right=<0|1> up=<0|1> down=<0|1> seq=<u32>".
    /// Order does not matter.
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

/// Struct that represents a player in the application.
pub struct Player {
    id: usize,
    /// The last sequence number sent by the player.
    last_seq: u32,
    /// The object the player is in control of.
    dynamic_object: DynamicObject,
    /// Specifies the rate of acceleration a player can move with.
    acceleration_speed: f32,
    jump_speed: f32,
    input_state: PlayerInputState,
}

impl Default for Player {
    fn default() -> Self {
        Player {
            id: usize::MAX,
            last_seq: u32::default(),
            dynamic_object: DynamicObject::default(),
            // Typical accelartion of a human.
            acceleration_speed: 3.5,
            // Typical vertical jump-speed of a human.
            jump_speed: 2.7,
            input_state: PlayerInputState::default(),
        }
    }
}

/// Struct that describes the current gamestate.
pub struct GameState {
    /// A collection of the players in the game.
    players: Vec<Player>,

    /// A collection of the static objects in the game.
    objects: Vec<Object>,
}

impl GameState {
    /// Creates an empty new GameState
    pub fn new() -> Self {
        GameState {
            players: Vec::new(),
            objects: Vec::new(),
        }
    }

    /// Adds a player to the game with a given ID.
    pub fn add_player(&mut self, id: usize) {
        self.players.push(Player {
            id: id,
            ..Default::default()
        })
    }

    /// Removes the player with the given ID.
    pub fn remove_player(&mut self, id: usize) {
        self.players.retain(|p| p.id != id);
    }

    /// Returns a mutable reference to the player with the given ID, if it exists.
    pub fn find_player_mut(&mut self, id: usize) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.id == id)
    }

    /// Returns the game-objects as a collection of ObjectInfo.
    pub fn get_objects(&self) -> Vec<ObjectInfo> {
        let mut result: Vec<ObjectInfo> = self
            .objects
            .iter()
            .map(|o| ObjectInfo {
                object_type: ObjectType::Static,
                object: *o,
            })
            .collect();

        result.extend(self.players.iter().map(|p| ObjectInfo {
            object_type: ObjectType::Player,
            object: p.dynamic_object.object,
        }));

        result
    }

    /// Returns the last sequence number of each
    pub fn get_players_last_seqs(&self) -> Vec<(usize, SeqInfo)> {
        self.players
            .iter()
            .map(|p| {
                (
                    p.id,
                    SeqInfo {
                        last_seq: p.last_seq,
                    },
                )
            })
            .collect()
    }

    /// Sets the maximum horizontal speed for the player with the given ID.
    /// If the player exists, updates their max_speed property.
    pub fn set_player_max_speed(&mut self, id: usize, speed: f32) {
        if let Some(player) = self.find_player_mut(id) {
            player.dynamic_object.max_speed = speed;
        }
    }

    /// Sets the gravity affecting the player with the given ID.
    /// If the player exists, updates their gravity property.
    pub fn set_player_gravity(&mut self, id: usize, gravity: f32) {
        if let Some(player) = self.find_player_mut(id) {
            player.dynamic_object.gravity = gravity;
        }
    }

    /// Sets the acceleration speed for the player with the given ID.
    /// If the player exists, updates their acceleration_speed property.
    pub fn set_player_acceleration_speed(&mut self, id: usize, speed: f32) {
        if let Some(player) = self.find_player_mut(id) {
            player.acceleration_speed = speed;
        }
    }

    /// Sets the jump speed for the player with the given ID.
    /// If the player exists, updates their jump_speed property.
    pub fn set_player_jump_speed(&mut self, id: usize, speed: f32) {
        if let Some(player) = self.find_player_mut(id) {
            player.jump_speed = speed;
        }
    }

    /// Updates the input state for the player with the given ID.
    /// Only updates if the input sequence number is newer than the last received.
    pub fn update_player_input(&mut self, id: usize, input: PlayerInputState) {
        if let Some(player) = self.find_player_mut(id) {
            if input.seq > player.last_seq {
                player.input_state = input.clone();
                player.last_seq = input.seq;
            }
        }
    }

    /// Processes the input of all players, and updates their dynamic objects accordingly.
    pub fn process_input(&mut self) {
        for player in &mut self.players {
            // Horizontal movement
            // Move left
            if player.input_state.left && !player.input_state.right {
                player.dynamic_object.acceleration.x = -player.acceleration_speed;
            // Move right
            } else if player.input_state.right && !player.input_state.left {
                player.dynamic_object.acceleration.x = player.acceleration_speed;
            // Stand still
            } else {
                player.dynamic_object.acceleration.x = 0.0;
                player.dynamic_object.velocity.x = 0.0;
            }

            // Vertical movement
            // Only allow jump if grounded
            if player.input_state.up && !player.input_state.down && player.dynamic_object.grounded {
                player.dynamic_object.velocity.y = player.jump_speed;
            // Stop moving up if player releases up-button and is moving upwards
            } else if !player.input_state.up && player.dynamic_object.velocity.y > 0.0 {
                player.dynamic_object.velocity.y = 0.0;
            }
        }
    }

    /// Simulate physics, resolves collisions, and checks for grounded
    /// for all dynamic objects (currently only players).
    pub fn update_positions(&mut self, dt: f32) {
        for player in &mut self.players {
            player.dynamic_object.simulate(dt);
            player.dynamic_object.resolve_collisions(&self.objects);
            player.dynamic_object.check_grounded(&self.objects);
        }
    }

    /// Add a static object to the game.
    pub fn add_object(&mut self, position: Vector2, width: f32, height: f32) {
        let object = Object {
            width: width,
            height: height,
            position,
        };
        self.objects.push(object);
    }
}
