use log::debug;
use shared::{
    resolve_collision, InputState, Player, FLOOR_Y, GRAVITY, JUMP_VELOCITY, PLAYER_SIZE,
    PLAYER_SPEED, WORLD_WIDTH,
};
use std::collections::HashMap;

/// Configuration for how server state updates should be processed
#[derive(Debug, Clone)]
pub struct ServerStateConfig {
    pub client_id: Option<u32>,       // Local player ID (if connected)
    pub reconciliation_enabled: bool, // Whether to perform rollback/replay
    pub interpolation_enabled: bool,  // Whether to buffer states for interpolation
}

/// Basic game state containing all players and simulation tick
/// Used for both authoritative server state and predicted client state
#[derive(Debug, Clone)]
pub struct GameState {
    pub tick: u32,                     // Simulation tick number
    pub players: HashMap<u32, Player>, // All players indexed by ID
}

impl GameState {
    /// Creates a new empty game state
    pub fn new() -> Self {
        Self {
            tick: 0,
            players: HashMap::new(),
        }
    }

    /// Applies input to a specific player, updating their velocity
    /// Input is processed immediately (no physics integration yet)
    pub fn apply_input(&mut self, client_id: u32, input: &InputState, _dt: f32) {
        if let Some(player) = self.players.get_mut(&client_id) {
            // Reset horizontal velocity first
            player.vel_x = 0.0;

            // Apply horizontal movement
            if input.left {
                player.vel_x -= PLAYER_SPEED;
            }
            if input.right {
                player.vel_x += PLAYER_SPEED;
            }

            // Apply jump (only if grounded to prevent infinite jumping)
            if input.jump && player.on_ground {
                player.vel_y = JUMP_VELOCITY;
                player.on_ground = false;
            }
        }
    }

    /// Updates physics for all players (gravity, movement, collisions, boundaries)
    pub fn update_physics(&mut self, dt: f32) {
        for player in self.players.values_mut() {
            // Apply gravity to players not on ground
            if !player.on_ground {
                player.vel_y += GRAVITY * dt;
            }

            // Integrate velocity to update position
            player.x += player.vel_x * dt;
            player.y += player.vel_y * dt;

            // Clamp horizontal position to world boundaries
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

    /// Handles player-to-player collisions using the shared collision system
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

                    // Use shared collision resolution
                    resolve_collision(&mut player1, &mut player2);

                    // Update players in map
                    self.players.insert(id1, player1);
                    self.players.insert(id2, player2);
                }
            }
        }
    }

    /// Advances simulation by one tick (combines physics update with tick increment)
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

/// Client-side game state manager that handles prediction, reconciliation, and interpolation
/// Maintains both confirmed (authoritative) and predicted game states
pub struct ClientGameState {
    pub confirmed_state: GameState,     // Last confirmed state from server
    pub predicted_state: GameState,     // Client's predicted current state
    pub input_history: Vec<InputState>, // Unconfirmed inputs for rollback
    pub last_confirmed_tick: u32,       // Tick of last confirmed server state
    pub interpolation_buffer: Vec<(u64, Vec<Player>)>, // Timestamped states for interpolation
    pub physics_accumulator: f32,       // Accumulates time for fixed timestep
    pub fixed_timestep: f32,            // Fixed timestep for deterministic simulation (60 FPS)
}

impl ClientGameState {
    /// Creates new client game state with default values
    pub fn new() -> Self {
        Self {
            confirmed_state: GameState::new(),
            predicted_state: GameState::new(),
            input_history: Vec::new(),
            last_confirmed_tick: 0,
            interpolation_buffer: Vec::new(),
            physics_accumulator: 0.0,
            fixed_timestep: 1.0 / 60.0, // 60 FPS for deterministic simulation
        }
    }

    /// Processes authoritative server state update
    /// Handles confirmed state update, interpolation buffering, and reconciliation
    pub fn apply_server_state(
        &mut self,
        tick: u32,
        timestamp: u64,
        players: Vec<Player>,
        last_processed_input: HashMap<u32, u32>,
        config: ServerStateConfig,
    ) {
        // Update confirmed state with server's authoritative data
        self.confirmed_state.players.clear();
        for player in &players {
            self.confirmed_state
                .players
                .insert(player.id, player.clone());
        }
        self.confirmed_state.tick = tick;

        // Ensure predicted state has local player if connected but not present
        if let Some(client_id) = config.client_id {
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.predicted_state.players.entry(client_id)
            {
                if let Some(player) = self.confirmed_state.players.get(&client_id) {
                    e.insert(player.clone());
                }
            }
        }

        // Add to interpolation buffer if enabled
        if config.interpolation_enabled {
            self.interpolation_buffer.push((timestamp, players));
            // Keep only recent states (1000ms = 1 second buffer)
            let cutoff = timestamp.saturating_sub(1000);
            self.interpolation_buffer.retain(|(ts, _)| *ts > cutoff);
        }

        // Perform rollback and replay if reconciliation is enabled
        if config.reconciliation_enabled {
            if let Some(client_id) = config.client_id {
                self.perform_reconciliation(client_id, last_processed_input);
            }
        } else if let Some(client_id) = config.client_id {
            // Without reconciliation, just sync predicted state to confirmed state
            if let Some(confirmed_player) = self.confirmed_state.players.get(&client_id) {
                self.predicted_state
                    .players
                    .insert(client_id, confirmed_player.clone());
            }
        }

        self.last_confirmed_tick = tick;
    }

    /// Performs client-side reconciliation to correct prediction errors
    ///
    /// Reconciliation is a key netcode technique that corrects client prediction when
    /// the server's authoritative state differs from the client's predicted state.
    /// This method implements rollback and replay:
    /// 1. Removes acknowledged inputs from history
    /// 2. Checks if predicted state diverged from server state
    /// 3. If divergence is significant, rolls back to server state
    /// 4. Replays unacknowledged inputs to restore prediction
    fn perform_reconciliation(&mut self, client_id: u32, last_processed_input: HashMap<u32, u32>) {
        if let Some(&last_processed_seq) = last_processed_input.get(&client_id) {
            // Clean up input history - remove inputs the server has already processed
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
                // Calculate position difference between server and client prediction
                let dx = confirmed.x - predicted.x;
                let dy = confirmed.y - predicted.y;
                let distance = (dx * dx + dy * dy).sqrt();

                // Only perform expensive rollback if prediction error is significant
                if distance > 1.0 {
                    debug!("Rollback needed! Distance: {:.2}", distance);

                    // Rollback: Reset predicted state to confirmed server state
                    self.predicted_state = self.confirmed_state.clone();
                    self.predicted_state.tick = self.confirmed_state.tick;

                    // Replay: Re-apply all unacknowledged inputs to restore prediction
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
    ///
    /// Client-side prediction allows the game to feel responsive by immediately
    /// applying player input locally rather than waiting for server confirmation.
    /// The input is stored in history for potential reconciliation later.
    pub fn apply_prediction(&mut self, client_id: u32, input: &InputState) {
        // Store input for potential rollback/replay during reconciliation
        self.input_history.push(input.clone());

        // Prevent unbounded memory growth by trimming old inputs
        if self.input_history.len() > 1000 {
            self.input_history.drain(0..100);
        }

        // Apply input immediately to predicted state for responsive gameplay
        self.predicted_state
            .apply_input(client_id, input, self.fixed_timestep);
        self.predicted_state.step(self.fixed_timestep);
    }

    /// Updates physics accumulator for fixed timestep simulation
    ///
    /// Accumulates frame time and processes physics in fixed timesteps
    /// to ensure deterministic simulation regardless of frame rate.
    /// Uses remainder accumulation to handle fractional timesteps.
    pub fn update_physics(&mut self, dt: f32) {
        self.physics_accumulator += dt;

        // Process physics in fixed timesteps for determinism
        while self.physics_accumulator >= self.fixed_timestep {
            self.physics_accumulator -= self.fixed_timestep;
        }
    }

    /// Gets player positions for rendering based on netcode configuration
    ///
    /// Returns appropriate player state for rendering depending on enabled features:
    /// - Interpolation: Returns interpolated positions for smooth remote player movement
    /// - Prediction: Uses predicted state for local player, confirmed for others
    /// - Neither: Uses confirmed server state for all players
    ///
    /// This separation allows different netcode features to be toggled independently.
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
                // For local player: use predicted state if prediction enabled, otherwise confirmed
                if prediction_enabled {
                    if let Some(our_player) = self.predicted_state.players.get(&client_id) {
                        players.push(our_player.clone());
                    }
                } else if let Some(our_player) = self.confirmed_state.players.get(&client_id) {
                    players.push(our_player.clone());
                }

                // For remote players: always use confirmed state (no prediction for others)
                for (id, player) in &self.confirmed_state.players {
                    if *id != client_id {
                        players.push(player.clone());
                    }
                }
            } else {
                // No local player - return all confirmed states
                players = self.confirmed_state.players.values().cloned().collect();
            }

            players
        }
    }

    /// Performs temporal interpolation between buffered server states
    ///
    /// Interpolation smooths remote player movement by rendering positions
    /// between two buffered server states. This creates visually smooth movement
    /// for remote players while maintaining gameplay responsiveness.
    ///
    /// The implementation uses a 150ms render delay to ensure enough buffered
    /// states for smooth interpolation, trading slight visual latency for smoothness.
    fn get_interpolated_players(&self, client_id: Option<u32>) -> Vec<Player> {
        // Need at least 2 states to interpolate between
        if self.interpolation_buffer.len() < 2 {
            return self.get_render_players(client_id, false, false);
        }

        // Calculate render time with 150ms delay for interpolation buffer
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_millis() as u64;

        let render_time = now.saturating_sub(150);

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

                // Calculate interpolation factor (0.0 to 1.0)
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

                    // Interpolate remote player between two states
                    if let Some(p2) = players2.iter().find(|p| p.id == p1.id) {
                        let interpolated = Player {
                            id: p1.id,
                            x: p1.x + (p2.x - p1.x) * alpha,
                            y: p1.y + (p2.y - p1.y) * alpha,
                            vel_x: p1.vel_x + (p2.vel_x - p1.vel_x) * alpha,
                            vel_y: p1.vel_y + (p2.vel_y - p1.vel_y) * alpha,
                            on_ground: p2.on_ground, // Boolean state from newer frame
                        };
                        result.push(interpolated);
                    }
                }
                result
            }
            (Some(before_idx), None) => {
                // Only one state available - use it directly
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
