use log::debug;
use shared::{
    resolve_collision, InputState, Player, FLOOR_Y, GRAVITY, JUMP_VELOCITY, PLAYER_SIZE,
    PLAYER_SPEED, WORLD_WIDTH,
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ServerStateConfig {
    pub client_id: Option<u32>,
    pub reconciliation_enabled: bool,
    pub interpolation_enabled: bool,
}

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

    pub fn step(&mut self, dt: f32) {
        self.update_physics(dt);
        self.tick += 1;
    }
}

pub struct ClientGameState {
    pub confirmed_state: GameState,
    pub predicted_state: GameState,
    pub input_history: Vec<InputState>,
    pub last_confirmed_tick: u32,
    pub interpolation_buffer: Vec<(u64, Vec<Player>)>,
    pub physics_accumulator: f32,
    pub fixed_timestep: f32,
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

    pub fn apply_server_state(
        &mut self,
        tick: u32,
        timestamp: u64,
        players: Vec<Player>,
        last_processed_input: HashMap<u32, u32>,
        config: ServerStateConfig,
    ) {
        self.confirmed_state.players.clear();
        for player in &players {
            self.confirmed_state
                .players
                .insert(player.id, player.clone());
        }
        self.confirmed_state.tick = tick;

        if let Some(client_id) = config.client_id {
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.predicted_state.players.entry(client_id)
            {
                if let Some(player) = self.confirmed_state.players.get(&client_id) {
                    e.insert(player.clone());
                }
            }
        }

        if config.interpolation_enabled {
            self.interpolation_buffer.push((timestamp, players));
            let cutoff = timestamp.saturating_sub(1000);
            self.interpolation_buffer.retain(|(ts, _)| *ts > cutoff);
        }

        if config.reconciliation_enabled {
            if let Some(client_id) = config.client_id {
                self.perform_reconciliation(client_id, last_processed_input);
            }
        } else if let Some(client_id) = config.client_id {
            if let Some(confirmed_player) = self.confirmed_state.players.get(&client_id) {
                self.predicted_state
                    .players
                    .insert(client_id, confirmed_player.clone());
            }
        }

        self.last_confirmed_tick = tick;
    }

    fn perform_reconciliation(&mut self, client_id: u32, last_processed_input: HashMap<u32, u32>) {
        if let Some(&last_processed_seq) = last_processed_input.get(&client_id) {
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
                let dx = confirmed.x - predicted.x;
                let dy = confirmed.y - predicted.y;
                let distance = (dx * dx + dy * dy).sqrt();

                if distance > 1.0 {
                    debug!("Rollback needed! Distance: {:.2}", distance);

                    self.predicted_state = self.confirmed_state.clone();
                    self.predicted_state.tick = self.confirmed_state.tick;

                    for input in &self.input_history {
                        self.predicted_state
                            .apply_input(client_id, input, self.fixed_timestep);
                        self.predicted_state.step(self.fixed_timestep);
                    }
                }
            }
        }
    }

    pub fn apply_prediction(&mut self, client_id: u32, input: &InputState) {
        self.input_history.push(input.clone());

        if self.input_history.len() > 1000 {
            self.input_history.drain(0..100);
        }

        self.predicted_state
            .apply_input(client_id, input, self.fixed_timestep);
        self.predicted_state.step(self.fixed_timestep);
    }

    pub fn update_physics(&mut self, dt: f32) {
        self.physics_accumulator += dt;

        while self.physics_accumulator >= self.fixed_timestep {
            self.physics_accumulator -= self.fixed_timestep;
        }
    }

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
                if prediction_enabled {
                    if let Some(our_player) = self.predicted_state.players.get(&client_id) {
                        players.push(our_player.clone());
                    }
                } else if let Some(our_player) = self.confirmed_state.players.get(&client_id) {
                    players.push(our_player.clone());
                }

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

    fn get_interpolated_players(&self, client_id: Option<u32>) -> Vec<Player> {
        if self.interpolation_buffer.len() < 2 {
            return self.get_render_players(client_id, false, false);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_millis() as u64;

        let render_time = now.saturating_sub(150);

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
                    if Some(p1.id) == client_id {
                        if let Some(our_player) = self.predicted_state.players.get(&p1.id) {
                            result.push(our_player.clone());
                        }
                        continue;
                    }

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
