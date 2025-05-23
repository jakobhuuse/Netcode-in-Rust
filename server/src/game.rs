//! Server-side game state management and physics simulation
//!
//! This module implements the authoritative game simulation that runs on the server.
//! It handles:
//! - Authoritative player state management and physics
//! - Input validation and processing from connected clients
//! - Deterministic physics simulation for consistent multiplayer state
//! - Player collision detection and resolution
//! - World boundary enforcement and game rules

use log::info;
use shared::{
    resolve_collision, InputState, Player, FLOOR_Y, GRAVITY, JUMP_VELOCITY, PLAYER_SIZE,
    PLAYER_SPEED, WORLD_WIDTH,
};
use std::collections::HashMap;

/// Authoritative game state maintained by the server
///
/// This structure represents the single source of truth for the game world.
/// All clients must synchronize to this state, which is updated deterministically
/// using fixed timesteps to ensure consistency across the multiplayer session.
#[derive(Debug, Clone)]
pub struct GameState {
    /// Current simulation tick counter for synchronization
    pub tick: u32,
    /// All connected players indexed by their client ID
    pub players: HashMap<u32, Player>,
}

impl GameState {
    /// Creates a new empty game state
    ///
    /// Initializes the authoritative game world with no players and tick 0.
    /// Players will be added dynamically as clients connect to the server.
    pub fn new() -> Self {
        Self {
            tick: 0,
            players: HashMap::new(),
        }
    }

    /// Adds a new player to the game world when a client connects
    ///
    /// Spawns the player at a deterministic position based on their client ID
    /// to avoid overlapping spawns. The spawn position is distributed across
    /// the game world width to separate multiple players.
    pub fn add_player(&mut self, client_id: u32) {
        // Distribute spawn positions across the world to avoid collisions
        let spawn_x = 100.0 + (client_id as f32 * 60.0) % (WORLD_WIDTH - 200.0);
        let spawn_y = FLOOR_Y - PLAYER_SIZE;

        let player = Player::new(client_id, spawn_x, spawn_y);

        info!("Added player {} at ({}, {})", client_id, player.x, player.y);
        self.players.insert(client_id, player);
    }

    /// Removes a player from the game world when a client disconnects
    ///
    /// Cleans up player state and logs the disconnection for server monitoring.
    /// Other players will no longer see or collide with the disconnected player.
    pub fn remove_player(&mut self, client_id: &u32) {
        self.players.remove(client_id);
        info!("Removed player {}", client_id);
    }

    /// Applies validated client input to update player state
    ///
    /// Processes input commands from clients and updates the corresponding
    /// player's velocity and state. Input validation ensures only connected
    /// players can affect the game state. Movement and jumping are applied
    /// according to game physics rules.
    pub fn apply_input(&mut self, client_id: u32, input: &InputState, _dt: f32) {
        if let Some(player) = self.players.get_mut(&client_id) {
            // Reset horizontal velocity each frame (no momentum)
            player.vel_x = 0.0;

            // Apply horizontal movement based on input
            if input.left {
                player.vel_x -= PLAYER_SPEED;
            }
            if input.right {
                player.vel_x += PLAYER_SPEED;
            }

            // Apply jump only when on ground to prevent double jumping
            if input.jump && player.on_ground {
                player.vel_y = JUMP_VELOCITY;
                player.on_ground = false;
            }
        }
    }

    /// Updates physics simulation for all players using fixed timestep
    ///
    /// Applies physics forces (gravity), updates positions based on velocity,
    /// enforces world boundaries, and handles ground/ceiling collisions.
    /// Uses deterministic fixed timestep to ensure identical simulation
    /// results across server and client prediction.
    pub fn update_physics(&mut self, dt: f32) {
        for player in self.players.values_mut() {
            // Apply gravity when not on ground
            if !player.on_ground {
                player.vel_y += GRAVITY * dt;
            }

            // Update position based on velocity
            player.x += player.vel_x * dt;
            player.y += player.vel_y * dt;

            // Enforce horizontal world boundaries
            player.x = player.x.clamp(0.0, WORLD_WIDTH - PLAYER_SIZE);

            // Handle floor collision
            if player.y + PLAYER_SIZE >= FLOOR_Y {
                player.y = FLOOR_Y - PLAYER_SIZE;
                player.vel_y = 0.0;
                player.on_ground = true;
            }

            // Handle ceiling collision
            if player.y <= 0.0 {
                player.y = 0.0;
                player.vel_y = 0.0;
            }
        }

        // Process player-to-player collisions
        self.handle_collisions();
    }

    /// Handles collision detection and resolution between all players
    ///
    /// Iterates through all player pairs to detect overlaps and applies
    /// collision resolution using the shared collision system. This ensures
    /// players cannot occupy the same space and creates realistic physics
    /// interactions between players.
    fn handle_collisions(&mut self) {
        let player_ids: Vec<u32> = self.players.keys().cloned().collect();

        // Check all pairs of players for collisions
        for i in 0..player_ids.len() {
            for j in (i + 1)..player_ids.len() {
                let id1 = player_ids[i];
                let id2 = player_ids[j];

                // Get player copies for collision processing
                if let (Some(p1), Some(p2)) = (
                    self.players.get(&id1).cloned(),
                    self.players.get(&id2).cloned(),
                ) {
                    let mut player1 = p1;
                    let mut player2 = p2;

                    // Apply collision resolution from shared module
                    resolve_collision(&mut player1, &mut player2);

                    // Update players with resolved positions
                    self.players.insert(id1, player1);
                    self.players.insert(id2, player2);
                }
            }
        }
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_approx_eq::assert_approx_eq;
    use shared::{InputState, FLOOR_Y, PLAYER_SIZE};

    #[test]
    fn test_game_state_creation() {
        let game_state = GameState::new();
        assert_eq!(game_state.tick, 0);
        assert!(game_state.players.is_empty());
    }

    #[test]
    fn test_add_player() {
        let mut game_state = GameState::new();
        game_state.add_player(1);

        assert_eq!(game_state.players.len(), 1);
        assert!(game_state.players.contains_key(&1));

        let player = game_state.players.get(&1).unwrap();
        assert_eq!(player.id, 1);
        assert_eq!(player.y, FLOOR_Y - PLAYER_SIZE);
        assert!(player.on_ground);
    }

    #[test]
    fn test_add_multiple_players() {
        let mut game_state = GameState::new();
        game_state.add_player(1);
        game_state.add_player(2);
        game_state.add_player(3);

        assert_eq!(game_state.players.len(), 3);

        let player1_x = game_state.players.get(&1).unwrap().x;
        let player2_x = game_state.players.get(&2).unwrap().x;
        let player3_x = game_state.players.get(&3).unwrap().x;

        assert_ne!(player1_x, player2_x);
        assert_ne!(player2_x, player3_x);
    }

    #[test]
    fn test_remove_player() {
        let mut game_state = GameState::new();
        game_state.add_player(1);
        game_state.add_player(2);

        assert_eq!(game_state.players.len(), 2);

        game_state.remove_player(&1);
        assert_eq!(game_state.players.len(), 1);
        assert!(!game_state.players.contains_key(&1));
        assert!(game_state.players.contains_key(&2));
    }

    #[test]
    fn test_apply_input_movement() {
        let mut game_state = GameState::new();
        game_state.add_player(1);

        let input = InputState {
            sequence: 1,
            timestamp: 0,
            left: true,
            right: false,
            jump: false,
        };

        game_state.apply_input(1, &input, 1.0 / 60.0);

        let player = game_state.players.get(&1).unwrap();
        assert_eq!(player.vel_x, -PLAYER_SPEED);
    }

    #[test]
    fn test_apply_input_jump() {
        let mut game_state = GameState::new();
        game_state.add_player(1);

        let input = InputState {
            sequence: 1,
            timestamp: 0,
            left: false,
            right: false,
            jump: true,
        };

        game_state.apply_input(1, &input, 1.0 / 60.0);

        let player = game_state.players.get(&1).unwrap();
        assert_eq!(player.vel_y, JUMP_VELOCITY);
        assert!(!player.on_ground);
    }

    #[test]
    fn test_apply_input_no_double_jump() {
        let mut game_state = GameState::new();
        game_state.add_player(1);

        let input = InputState {
            sequence: 1,
            timestamp: 0,
            left: false,
            right: false,
            jump: true,
        };

        game_state.apply_input(1, &input, 1.0 / 60.0);
        let player = game_state.players.get(&1).unwrap();
        let first_vel_y = player.vel_y;

        game_state.apply_input(1, &input, 1.0 / 60.0);
        let player = game_state.players.get(&1).unwrap();
        assert_eq!(player.vel_y, first_vel_y);
    }

    #[test]
    fn test_update_physics_gravity() {
        let mut game_state = GameState::new();
        game_state.add_player(1);

        let input = InputState {
            sequence: 1,
            timestamp: 0,
            left: false,
            right: false,
            jump: true,
        };

        game_state.apply_input(1, &input, 1.0 / 60.0);
        let initial_vel_y = game_state.players.get(&1).unwrap().vel_y;

        let dt = 1.0 / 60.0;
        game_state.update_physics(dt);

        let player = game_state.players.get(&1).unwrap();
        let expected_vel_y = initial_vel_y + GRAVITY * dt;
        assert_approx_eq!(player.vel_y, expected_vel_y, 0.001);
    }

    #[test]
    fn test_update_physics_horizontal_movement() {
        let mut game_state = GameState::new();
        game_state.add_player(1);

        let initial_x = game_state.players.get(&1).unwrap().x;

        if let Some(player) = game_state.players.get_mut(&1) {
            player.vel_x = PLAYER_SPEED;
        }

        let dt = 1.0 / 60.0;
        game_state.update_physics(dt);

        let player = game_state.players.get(&1).unwrap();
        let expected_x = initial_x + PLAYER_SPEED * dt;
        assert_approx_eq!(player.x, expected_x, 0.001);
    }

    #[test]
    fn test_update_physics_boundary_clamping() {
        let mut game_state = GameState::new();
        game_state.add_player(1);

        if let Some(player) = game_state.players.get_mut(&1) {
            player.x = -10.0;
            player.vel_x = -PLAYER_SPEED;
        }

        let dt = 1.0 / 60.0;
        game_state.update_physics(dt);

        let player = game_state.players.get(&1).unwrap();
        assert_eq!(player.x, 0.0);
    }

    #[test]
    fn test_update_physics_floor_collision() {
        let mut game_state = GameState::new();
        game_state.add_player(1);

        if let Some(player) = game_state.players.get_mut(&1) {
            player.y = FLOOR_Y + 10.0;
            player.vel_y = 100.0;
            player.on_ground = false;
        }

        let dt = 1.0 / 60.0;
        game_state.update_physics(dt);

        let player = game_state.players.get(&1).unwrap();
        assert_eq!(player.y, FLOOR_Y - PLAYER_SIZE);
        assert_eq!(player.vel_y, 0.0);
        assert!(player.on_ground);
    }

    #[test]
    fn test_update_physics_ceiling_collision() {
        let mut game_state = GameState::new();
        game_state.add_player(1);

        if let Some(player) = game_state.players.get_mut(&1) {
            player.y = -10.0;
            player.vel_y = -100.0;
        }

        let dt = 1.0 / 60.0;
        game_state.update_physics(dt);

        let player = game_state.players.get(&1).unwrap();
        assert_eq!(player.y, 0.0);
        assert_eq!(player.vel_y, 0.0);
    }
}
