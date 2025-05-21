pub struct Player {
    id: usize,
    position: (f32, f32),
}

pub struct GameState {
    players: Vec<Player>,
}
impl GameState {
    pub fn new() -> Self {
        GameState {
            players: Vec::new(),
        }
    }

    pub fn add_player(&mut self, id: usize) {
        let player = Player {
            id,
            position: (0.0, 0.0),
        };
        self.players.push(player);
    }

    pub fn remove_player(&mut self, id: usize) {
        self.players.retain(|p| p.id != id);
    }

    pub fn update_player_position(&mut self, id: usize, position: (f32, f32)) {
        if let Some(player) = self.players.iter_mut().find(|p| p.id == id) {
            player.position = position;
        }
    }

    pub fn move_player(&mut self, id: usize, direction: (f32, f32)) {
        if let Some(player) = self.players.iter_mut().find(|p| p.id == id) {
            player.position = (
                player.position.0 + direction.0,
                player.position.1 + direction.1,
            );
        }
    }

    pub fn get_player_positions(&self) -> Vec<(usize, (f32, f32))> {
        self.players.iter().map(|p| (p.id, p.position)).collect()
    }
}
