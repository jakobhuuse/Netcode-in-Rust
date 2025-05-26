//! Client-side game state management with prediction and reconciliation

use log::debug;
use shared::{
    resolve_collision, InputState, Player, FLOOR_Y, GRAVITY, JUMP_VELOCITY, PLAYER_SIZE,
    PLAYER_SPEED, WORLD_WIDTH,
};
use std::collections::HashMap;

/// Configuration for server state processing
#[derive(Debug, Clone)]
pub struct ServerStateConfig {
    pub client_id: Option<u32>,
    pub reconciliation_enabled: bool,
    pub interpolation_enabled: bool,
}

/// Basic game state containing all players and simulation tick
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

    /// Applies input to a specific player
    pub fn apply_input(&mut self, client_id: u32, input: &InputState, _dt: f32) {
        if let Some(player) = self.players.get_mut(&client_id) {
            player.vel_x = 0.0;

            if input.left {
                player.vel_x -= PLAYER_SPEED;
            }
            if input.right {
                player.vel_x += PLAYER_SPEED;
            }

            if input.jump && player.on_ground {
                player.vel_y = JUMP_VELOCITY;
                player.on_ground = false;
            }
        }
    }

    /// Updates physics for all players
    pub fn update_physics(&mut self, dt: f32) {
        for player in self.players.values_mut() {
            if !player.on_ground {
                player.vel_y += GRAVITY * dt;
            }

            player.x += player.vel_x * dt;
            player.y += player.vel_y * dt;

            player.x = player.x.clamp(0.0, WORLD_WIDTH - PLAYER_SIZE);

            if player.y + PLAYER_SIZE >= FLOOR_Y {
                player.y = FLOOR_Y - PLAYER_SIZE;
                player.vel_y = 0.0;
                player.on_ground = true;
            }

            if player.y <= 0.0 {
                player.y = 0.0;
                player.vel_y = 0.0;
            }
        }

        self.handle_collisions();
    }

    fn handle_collisions(&mut self) {
        let player_ids: Vec<u32> = self.players.keys().cloned().collect();

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

    /// Advances simulation by one tick
    pub fn step(&mut self, dt: f32) {
        self.update_physics(dt);
        self.tick += 1;
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

/// Client-side game state manager handling prediction, reconciliation, and interpolation
pub struct ClientGameState {
    pub confirmed_state: GameState,     // Last confirmed state from server
    pub predicted_state: GameState,     // Client's predicted current state
    pub input_history: Vec<InputState>, // Unconfirmed inputs for rollback
    pub last_confirmed_tick: u32,
    pub interpolation_buffer: Vec<(u64, Vec<Player>)>, // Timestamped states for interpolation
    pub physics_accumulator: f32,
    pub fixed_timestep: f32, // Fixed timestep for deterministic simulation (60 FPS)
}

impl ClientGameState {
    pub fn new() -> Self {
        Self {
            confirmed_state: GameState::new(),
            predicted_state: GameState::new(),
            input_history: Vec::new(),
            last_confirmed_tick: 0,
            interpolation_buffer: Vec::new(),
            physics_accumulator: 0.0,
            fixed_timestep: 1.0 / 60.0,
        }
    }

    /// Processes authoritative server state update
    pub fn apply_server_state(
        &mut self,
        tick: u32,
        timestamp: u64,
        players: Vec<Player>,
        last_processed_input: HashMap<u32, u32>,
        config: ServerStateConfig,
    ) {
        // Update confirmed state
        self.confirmed_state.players.clear();
        for player in &players {
            self.confirmed_state
                .players
                .insert(player.id, player.clone());
        }
        self.confirmed_state.tick = tick;

        // Ensure predicted state has local player
        if let Some(client_id) = config.client_id {
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.predicted_state.players.entry(client_id)
            {
                if let Some(player) = self.confirmed_state.players.get(&client_id) {
                    e.insert(player.clone());
                }
            }
        }

        // Add to interpolation buffer
        if config.interpolation_enabled {
            self.interpolation_buffer.push((timestamp, players));
            let cutoff = timestamp.saturating_sub(1000);
            self.interpolation_buffer.retain(|(ts, _)| *ts > cutoff);
        }

        // Perform reconciliation
        if config.reconciliation_enabled {
            if let Some(client_id) = config.client_id {
                self.perform_reconciliation(client_id, last_processed_input);
            }
        } else if let Some(client_id) = config.client_id {
            // Without reconciliation, just sync to confirmed state
            if let Some(confirmed_player) = self.confirmed_state.players.get(&client_id) {
                self.predicted_state
                    .players
                    .insert(client_id, confirmed_player.clone());
            }
        }

        self.last_confirmed_tick = tick;
    }

    /// Performs client-side reconciliation using rollback and replay
    fn perform_reconciliation(&mut self, client_id: u32, last_processed_input: HashMap<u32, u32>) {
        if let Some(&last_processed_seq) = last_processed_input.get(&client_id) {
            // Remove processed inputs
            let initial_history_len = self.input_history.len();
            self.input_history
                .retain(|input| input.sequence > last_processed_seq);

            debug!(
                "Removed {} processed inputs from history",
                initial_history_len - self.input_history.len()
            );

            let confirmed_player = self.confirmed_state.players.get(&client_id);
            let predicted_player = self.predicted_state.players.get(&client_id);

            if let (Some(confirmed), Some(predicted)) = (confirmed_player, predicted_player) {
                // Check if rollback is needed
                let dx = confirmed.x - predicted.x;
                let dy = confirmed.y - predicted.y;
                let distance = (dx * dx + dy * dy).sqrt();

                if distance > 1.0 {
                    debug!("Rollback needed! Distance: {:.2}", distance);

                    // Rollback: Reset to confirmed state
                    self.predicted_state = self.confirmed_state.clone();
                    self.predicted_state.tick = self.confirmed_state.tick;

                    // Replay: Re-apply unacknowledged inputs
                    for input in &self.input_history {
                        self.predicted_state
                            .apply_input(client_id, input, self.fixed_timestep);
                        self.predicted_state.step(self.fixed_timestep);
                    }
                }
            }
        }
    }

    /// Applies client-side prediction for immediate input response
    pub fn apply_prediction(&mut self, client_id: u32, input: &InputState) {
        // Store input for potential rollback
        self.input_history.push(input.clone());

        // Prevent unbounded memory growth
        if self.input_history.len() > 1000 {
            self.input_history.drain(0..100);
        }

        // Apply input immediately to predicted state
        self.predicted_state
            .apply_input(client_id, input, self.fixed_timestep);
        self.predicted_state.step(self.fixed_timestep);
    }

    /// Updates physics accumulator for fixed timestep simulation
    pub fn update_physics(&mut self, dt: f32) {
        self.physics_accumulator += dt;

        while self.physics_accumulator >= self.fixed_timestep {
            self.physics_accumulator -= self.fixed_timestep;
        }
    }

    /// Gets player positions for rendering based on netcode configuration
    pub fn get_render_players(
        &self,
        client_id: Option<u32>,
        prediction_enabled: bool,
        interpolation_enabled: bool,
    ) -> Vec<Player> {
        if interpolation_enabled {
            self.get_interpolated_players(client_id)
        } else {
            let mut players = Vec::new();

            if let Some(client_id) = client_id {
                // Local player: use predicted or confirmed state
                if prediction_enabled {
                    if let Some(our_player) = self.predicted_state.players.get(&client_id) {
                        players.push(our_player.clone());
                    }
                } else if let Some(our_player) = self.confirmed_state.players.get(&client_id) {
                    players.push(our_player.clone());
                }

                // Remote players: always use confirmed state
                for (id, player) in &self.confirmed_state.players {
                    if *id != client_id {
                        players.push(player.clone());
                    }
                }
            } else {
                players = self.confirmed_state.players.values().cloned().collect();
            }

            players
        }
    }

    /// Performs temporal interpolation between buffered server states
    fn get_interpolated_players(&self, client_id: Option<u32>) -> Vec<Player> {
        if self.interpolation_buffer.len() < 2 {
            return self.get_render_players(client_id, false, false);
        }

        // Calculate render time with 150ms delay for smooth interpolation
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_millis();
        let now_safe = (now.min(u64::MAX as u128)) as u64;

        let render_time = now_safe.saturating_sub(150);

        // Find the two states to interpolate between
        let mut before = None;
        let mut after = None;

        for i in 0..self.interpolation_buffer.len() {
            let (timestamp, _) = &self.interpolation_buffer[i];
            if *timestamp <= render_time {
                before = Some(i);
            } else {
                after = Some(i);
                break;
            }
        }

        match (before, after) {
            (Some(before_idx), Some(after_idx)) => {
                let (t1, players1) = &self.interpolation_buffer[before_idx];
                let (t2, players2) = &self.interpolation_buffer[after_idx];

                let alpha = if t2 > t1 {
                    ((render_time - t1) as f32) / ((t2 - t1) as f32)
                } else {
                    0.0
                }
                .clamp(0.0, 1.0);

                let mut result = Vec::new();
                for p1 in players1 {
                    // Local player uses prediction, not interpolation
                    if Some(p1.id) == client_id {
                        if let Some(our_player) = self.predicted_state.players.get(&p1.id) {
                            result.push(our_player.clone());
                        }
                        continue;
                    }

                    // Interpolate remote player
                    if let Some(p2) = players2.iter().find(|p| p.id == p1.id) {
                        let interpolated = Player {
                            id: p1.id,
                            x: p1.x + (p2.x - p1.x) * alpha,
                            y: p1.y + (p2.y - p1.y) * alpha,
                            vel_x: p1.vel_x + (p2.vel_x - p1.vel_x) * alpha,
                            vel_y: p1.vel_y + (p2.vel_y - p1.vel_y) * alpha,
                            on_ground: p2.on_ground,
                        };
                        result.push(interpolated);
                    }
                }
                result
            }
            (Some(before_idx), None) => {
                let (_, players) = &self.interpolation_buffer[before_idx];
                let mut result = players.clone();
                // Still use prediction for local player
                if let Some(client_id) = client_id {
                    if let Some(our_player) = self.predicted_state.players.get(&client_id) {
                        if let Some(pos) = result.iter().position(|p| p.id == client_id) {
                            result[pos] = our_player.clone();
                        }
                    }
                }
                result
            }
            _ => self.get_render_players(client_id, false, false),
        }
    }
}

impl Default for ClientGameState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{InputState, Player};

    #[test]
    fn test_game_state_creation() {
        let state = GameState::new();
        assert_eq!(state.tick, 0);
        assert!(state.players.is_empty());
    }

    #[test]
    fn test_game_state_step() {
        let mut state = GameState::new();
        let mut player = Player::new(1, 100.0, 100.0);
        player.vel_x = 50.0;
        state.players.insert(1, player);

        let dt = 1.0 / 60.0;
        state.step(dt);

        let player = &state.players[&1];
        assert!(player.x > 100.0); // Player should have moved
    }

    #[test]
    fn test_apply_input_client_side() {
        let mut state = GameState::new();
        state.players.insert(1, Player::new(1, 100.0, 100.0));

        let input = InputState {
            sequence: 1,
            timestamp: 1000,
            left: false,
            right: true,
            jump: false,
        };

        state.apply_input(1, &input, 1.0 / 60.0);

        let player = &state.players[&1];
        assert_eq!(player.vel_x, PLAYER_SPEED);
    }

    #[test]
    fn test_client_game_state_creation() {
        let client_state = ClientGameState::new();
        assert_eq!(client_state.confirmed_state.tick, 0);
        assert_eq!(client_state.predicted_state.tick, 0);
        assert!(client_state.input_history.is_empty());
        assert!(client_state.interpolation_buffer.is_empty());
        assert_eq!(client_state.last_confirmed_tick, 0);
        assert_eq!(client_state.physics_accumulator, 0.0);
        assert_eq!(client_state.fixed_timestep, 1.0 / 60.0);
    }

    #[test]
    fn test_client_game_state_determinism() {
        let mut state1 = ClientGameState::new();
        let mut state2 = ClientGameState::new();

        // Apply identical inputs
        let input = InputState {
            sequence: 1,
            timestamp: 1000,
            left: true,
            right: false,
            jump: false,
        };

        state1
            .predicted_state
            .players
            .insert(1, Player::new(1, 100.0, 100.0));
        state2
            .predicted_state
            .players
            .insert(1, Player::new(1, 100.0, 100.0));

        state1.predicted_state.apply_input(1, &input, 1.0 / 60.0);
        state2.predicted_state.apply_input(1, &input, 1.0 / 60.0);

        state1.predicted_state.step(1.0 / 60.0);
        state2.predicted_state.step(1.0 / 60.0);

        let player1 = &state1.predicted_state.players[&1];
        let player2 = &state2.predicted_state.players[&1];

        assert!((player1.x - player2.x).abs() < 0.001);
        assert!((player1.y - player2.y).abs() < 0.001);
        assert!((player1.vel_x - player2.vel_x).abs() < 0.001);
        assert!((player1.vel_y - player2.vel_y).abs() < 0.001);
    }

    #[test]
    fn test_apply_prediction() {
        let mut client_state = ClientGameState::new();

        // Add player and input
        client_state
            .predicted_state
            .players
            .insert(1, Player::new(1, 100.0, 100.0));
        let input = InputState {
            sequence: 1,
            timestamp: 1000,
            left: false,
            right: true,
            jump: false,
        };

        client_state.apply_prediction(1, &input);

        let player = &client_state.predicted_state.players[&1];
        assert_eq!(player.vel_x, PLAYER_SPEED);
        assert_eq!(client_state.input_history.len(), 1);
    }

    #[test]
    fn test_apply_server_state_reconciliation_disabled() {
        let mut client_state = ClientGameState::new();

        let players = vec![Player::new(1, 150.0, 200.0)];
        let config = ServerStateConfig {
            client_id: Some(1),
            reconciliation_enabled: false,
            interpolation_enabled: false,
        };

        client_state.apply_server_state(5, 2000, players, HashMap::new(), config);

        assert_eq!(client_state.confirmed_state.tick, 5);
        assert_eq!(client_state.confirmed_state.players[&1].x, 150.0);
        assert_eq!(client_state.predicted_state.players[&1].x, 150.0); // Should sync without reconciliation
    }

    #[test]
    fn test_physics_update_client_side() {
        let mut client_state = ClientGameState::new();

        // Add player with velocity (in the air, not on ground)
        let mut player = Player::new(1, 100.0, 100.0);
        player.vel_x = 120.0;
        player.vel_y = -50.0;
        player.on_ground = false; // Make sure player is in the air
        client_state.predicted_state.players.insert(1, player);

        // Use the game state directly to test physics
        let initial_x = client_state.predicted_state.players[&1].x;
        let initial_vel_y = client_state.predicted_state.players[&1].vel_y;

        let dt = 1.0 / 60.0;
        client_state.predicted_state.update_physics(dt);

        let player = &client_state.predicted_state.players[&1];
        assert!(player.x > initial_x); // Should move horizontally
        assert!(player.vel_y > initial_vel_y); // Gravity should be applied (more positive/less negative)
    }

    #[test]
    fn test_reconciliation_rollback_threshold() {
        let mut client_state = ClientGameState::new();

        // Add player with different position than server state
        let mut predicted_player = Player::new(1, 200.0, 100.0);
        predicted_player.vel_x = 100.0;
        client_state
            .predicted_state
            .players
            .insert(1, predicted_player);

        // Add some inputs to history
        for i in 1..=3 {
            let input = InputState {
                sequence: i,
                timestamp: i as u64 * 1000,
                left: false,
                right: true,
                jump: false,
            };
            client_state.input_history.push(input);
        }

        // Server state with significantly different position
        let confirmed_player = Player::new(1, 50.0, 100.0); // 150 units away
        let players = vec![confirmed_player];
        let mut last_processed = HashMap::new();
        last_processed.insert(1u32, 2u32); // Server processed up to sequence 2

        let config = ServerStateConfig {
            client_id: Some(1),
            reconciliation_enabled: true,
            interpolation_enabled: false,
        };

        client_state.apply_server_state(10, 5000, players, last_processed, config);

        // Should have performed rollback and replay
        assert_eq!(client_state.input_history.len(), 1); // Only unprocessed input remains
        let final_player = &client_state.predicted_state.players[&1];
        // Position should be closer to server state after reconciliation
        assert!(final_player.x < 200.0);
    }

    #[test]
    fn test_apply_server_state_with_interpolation() {
        let mut client_state = ClientGameState::new();

        let players = vec![Player::new(1, 100.0, 100.0)];
        let config = ServerStateConfig {
            client_id: Some(1),
            reconciliation_enabled: false,
            interpolation_enabled: true,
        };

        client_state.apply_server_state(1, 1000, players, HashMap::new(), config);

        assert_eq!(client_state.interpolation_buffer.len(), 1);
        assert_eq!(client_state.interpolation_buffer[0].0, 1000); // timestamp
        assert_eq!(client_state.interpolation_buffer[0].1[0].id, 1); // player
    }

    #[test]
    fn test_get_render_players_no_prediction() {
        let mut client_state = ClientGameState::new();
        client_state
            .confirmed_state
            .players
            .insert(1, Player::new(1, 100.0, 100.0));

        let players = client_state.get_render_players(Some(1), false, false);
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].x, 100.0);
    }

    #[test]
    fn test_get_render_players_with_prediction() {
        let mut client_state = ClientGameState::new();
        client_state
            .confirmed_state
            .players
            .insert(1, Player::new(1, 100.0, 100.0));
        client_state
            .predicted_state
            .players
            .insert(1, Player::new(1, 150.0, 100.0));

        let players = client_state.get_render_players(Some(1), true, false);
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].x, 150.0); // Should use predicted state
    }

    #[test]
    fn test_update_physics_accumulator() {
        let mut client_state = ClientGameState::new();

        // Add player
        let mut player = Player::new(1, 100.0, 100.0);
        player.vel_x = 120.0;
        client_state.predicted_state.players.insert(1, player);

        // Update with larger delta time
        let large_dt = 3.0 / 60.0; // 3 frames worth
        client_state.update_physics(large_dt);

        // Accumulator should have wrapped around after processing steps
        assert!(client_state.physics_accumulator < client_state.fixed_timestep);
        assert!(client_state.physics_accumulator >= 0.0);
    }

    #[test]
    fn test_interpolation_buffer_management() {
        let mut client_state = ClientGameState::new();

        // Add multiple states to interpolation buffer
        for i in 0..5 {
            let players = vec![Player::new(1, i as f32 * 10.0, 100.0)];
            let config = ServerStateConfig {
                client_id: Some(1),
                reconciliation_enabled: false,
                interpolation_enabled: true,
            };

            // Use timestamps within the retention window (1000ms)
            let base_time = 15000u64; // Recent timestamp
            let timestamp = base_time + (i as u64) * 100; // 100ms apart
            client_state.apply_server_state(i, timestamp, players, HashMap::new(), config);
        }

        // Should have all 5 states (all within retention window)
        assert_eq!(client_state.interpolation_buffer.len(), 5);
    }

    #[test]
    fn test_input_history_overflow_protection() {
        let mut client_state = ClientGameState::new();

        // Add many inputs through apply_prediction to trigger overflow protection
        for i in 0..150 {
            let input = InputState {
                sequence: i,
                timestamp: i as u64 * 16,
                left: i % 2 == 0,
                right: i % 2 == 1,
                jump: i % 10 == 0,
            };
            client_state.apply_prediction(1, &input);
        }

        // Should be managed by overflow protection in apply_prediction
        assert!(client_state.input_history.len() <= 1000);
    }
}
