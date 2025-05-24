use log::debug;
use shared::{
    resolve_collision, InputState, Player, FLOOR_Y, GRAVITY, JUMP_VELOCITY, PLAYER_SIZE,
    PLAYER_SPEED, WORLD_HEIGHT, WORLD_WIDTH,
};
use std::collections::HashMap;

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

    pub fn apply_input(&mut self, client_id: u32, input: &InputState, dt: f32) {
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

            player.x = player.x.max(0.0).min(WORLD_WIDTH - PLAYER_SIZE);

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
}

impl ClientGameState {
    pub fn new() -> Self {
        Self {
            confirmed_state: GameState::new(),
            predicted_state: GameState::new(),
            input_history: Vec::new(),
            last_confirmed_tick: 0,
            interpolation_buffer: Vec::new(),
        }
    }

    pub fn apply_server_state(
        &mut self,
        tick: u32,
        timestamp: u64,
        players: Vec<Player>,
        last_processed_input: HashMap<u32, u32>,
        client_id: Option<u32>,
        reconciliation_enabled: bool,
        interpolation_enabled: bool,
    ) {
        self.confirmed_state.players.clear();
        for player in &players {
            self.confirmed_state
                .players
                .insert(player.id, player.clone());
        }
        self.confirmed_state.tick = tick;

        if let Some(client_id) = client_id {
            if !self.predicted_state.players.contains_key(&client_id) {
                if let Some(player) = self.confirmed_state.players.get(&client_id) {
                    self.predicted_state
                        .players
                        .insert(client_id, player.clone());
                }
            }
        }

        if interpolation_enabled {
            self.interpolation_buffer.push((timestamp, players));

            let cutoff = timestamp.saturating_sub(500);
            self.interpolation_buffer.retain(|(ts, _)| *ts > cutoff);
        }

        if reconciliation_enabled {
            if let Some(client_id) = client_id {
                self.perform_reconciliation(client_id, last_processed_input);
            }
        }

        self.last_confirmed_tick = tick;
    }

    fn perform_reconciliation(&mut self, client_id: u32, last_processed_input: HashMap<u32, u32>) {
        if let Some(&last_processed_seq) = last_processed_input.get(&client_id) {
            self.input_history
                .retain(|input| input.sequence > last_processed_seq);

            let confirmed_player = self.confirmed_state.players.get(&client_id);
            let predicted_player = self.predicted_state.players.get(&client_id);

            if let (Some(confirmed), Some(predicted)) = (confirmed_player, predicted_player) {
                let dx = confirmed.x - predicted.x;
                let dy = confirmed.y - predicted.y;
                let distance = (dx * dx + dy * dy).sqrt();

                if distance > 5.0 {
                    debug!("Rollback needed! Distance: {:.2}", distance);

                    self.predicted_state = self.confirmed_state.clone();

                    let dt = 1.0 / 60.0;
                    for input in &self.input_history {
                        self.predicted_state.apply_input(client_id, input, dt);
                        self.predicted_state.step(dt);
                    }
                }
            }
        }
    }

    pub fn apply_prediction(&mut self, client_id: u32, input: &InputState) {
        self.input_history.push(input.clone());

        let dt = 1.0 / 60.0;
        self.predicted_state.apply_input(client_id, input, dt);
    }

    pub fn update_physics(&mut self, dt: f32) {
        self.predicted_state.step(dt);
    }

    pub fn get_render_players(
        &self,
        client_id: Option<u32>,
        interpolation_enabled: bool,
    ) -> Vec<Player> {
        if interpolation_enabled {
            self.get_interpolated_players(client_id)
        } else {
            let mut players = Vec::new();

            if let Some(client_id) = client_id {
                if let Some(our_player) = self.predicted_state.players.get(&client_id) {
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
            return self.get_render_players(client_id, false);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_millis() as u64;

        let render_time = now.saturating_sub(100);

        let mut before = None;
        let mut after = None;

        for (timestamp, players) in &self.interpolation_buffer {
            if *timestamp <= render_time {
                before = Some((*timestamp, players));
            } else {
                after = Some((*timestamp, players));
                break;
            }
        }

        match (before, after) {
            (Some((t1, players1)), Some((t2, players2))) => {
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
            (Some((_, players)), None) => {
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
            _ => self.get_render_players(client_id, false),
        }
    }
}
