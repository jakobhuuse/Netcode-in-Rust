use log::info;
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

    pub fn add_player(&mut self, client_id: u32) {
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
}
