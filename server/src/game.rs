//! Server-side game state management and physics simulation

use log::info;
use shared::{
    resolve_collision, InputState, Player, FLOOR_Y, GRAVITY, JUMP_VELOCITY, PLAYER_SIZE,
    PLAYER_SPEED, WORLD_WIDTH,
};
use std::collections::HashMap;

/// Authoritative game state maintained by the server
#[derive(Debug, Clone)]
pub struct GameState {
    pub tick: u32,
    pub players: HashMap<u32, Player>,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            tick: 0,
            players: HashMap::new(),
        }
    }

    /// Adds a new player at a safe spawn position
    pub fn add_player(&mut self, client_id: u32) {
        // Distribute spawn positions to avoid collisions
        let spawn_x = 100.0 + (client_id as f32 * 60.0) % (WORLD_WIDTH - 200.0);
        let spawn_y = FLOOR_Y - PLAYER_SIZE;

        let player = Player::new(client_id, spawn_x, spawn_y);
        info!("Added player {} at ({}, {})", client_id, player.x, player.y);
        self.players.insert(client_id, player);
    }

    pub fn remove_player(&mut self, client_id: &u32) {
        self.players.remove(client_id);
        info!("Removed player {}", client_id);
    }

    /// Applies validated client input to update player state
    pub fn apply_input(&mut self, client_id: u32, input: &InputState, _dt: f32) {
        if let Some(player) = self.players.get_mut(&client_id) {
            // Reset horizontal velocity (no momentum)
            player.vel_x = 0.0;

            // Apply horizontal movement
            if input.left {
                player.vel_x -= PLAYER_SPEED;
            }
            if input.right {
                player.vel_x += PLAYER_SPEED;
            }

            // Apply jump only when on ground
            if input.jump && player.on_ground {
                player.vel_y = JUMP_VELOCITY;
                player.on_ground = false;
            }
        }
    }

    /// Updates physics simulation using fixed timestep
    pub fn update_physics(&mut self, dt: f32) {
        for player in self.players.values_mut() {
            // Apply gravity when not on ground
            if !player.on_ground {
                player.vel_y += GRAVITY * dt;
            }

            // Update position based on velocity
            player.x += player.vel_x * dt;
            player.y += player.vel_y * dt;

            // Enforce horizontal boundaries
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

        self.handle_collisions();
    }

    /// Handles collision detection and resolution between all players
    fn handle_collisions(&mut self) {
        let player_ids: Vec<u32> = self.players.keys().cloned().collect();

        // Check all pairs of players for collisions
        for i in 0..player_ids.len() {
            for j in (i + 1)..player_ids.len() {
                let id1 = player_ids[i];
                let id2 = player_ids[j];

                if let (Some(p1), Some(p2)) = (
                    self.players.get(&id1).cloned(),
                    self.players.get(&id2).cloned(),
                ) {
                    let mut player1 = p1;
                    let mut player2 = p2;

                    resolve_collision(&mut player1, &mut player2);

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
        let player = game_state.players.get(&1).unwrap();
        assert_eq!(player.id, 1);
        assert!(player.on_ground);
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
    fn test_physics_gravity() {
        let mut game_state = GameState::new();
        game_state.add_player(1);

        // Make player jump
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
}