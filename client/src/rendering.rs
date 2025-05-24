use macroquad::prelude::*;
use shared::{Player, FLOOR_Y, PLAYER_SIZE};

#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub client_id: Option<u32>,
    pub prediction_enabled: bool,
    pub reconciliation_enabled: bool,
    pub interpolation_enabled: bool,
    pub ping_ms: u64,
    pub fake_ping_ms: u64,
}

#[derive(Debug, Clone)]
pub struct UiConfig {
    pub client_id: Option<u32>,
    pub prediction_enabled: bool,
    pub reconciliation_enabled: bool,
    pub interpolation_enabled: bool,
    pub ping_ms: u64,
    pub fake_ping_ms: u64,
    pub player_count: usize,
}

pub struct Renderer {
    width: f32,
    height: f32,
}

impl Renderer {
    pub fn new(width: usize, height: usize) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Renderer {
            width: width as f32,
            height: height as f32,
        })
    }

    pub fn render(&mut self, players: &[Player], config: RenderConfig) {
        clear_background(Color::from_rgba(26, 26, 26, 255));

        self.draw_floor();

        for player in players {
            let is_local_player = Some(player.id) == config.client_id;
            let color = if is_local_player {
                GREEN
            } else {
                Color::from_rgba(255, 68, 68, 255)
            };

            self.draw_player(player, color);

            if is_local_player {
                self.draw_velocity_vector(player);
            }

            self.draw_player_id(player);
        }

        let ui_config = UiConfig {
            client_id: config.client_id,
            prediction_enabled: config.prediction_enabled,
            reconciliation_enabled: config.reconciliation_enabled,
            interpolation_enabled: config.interpolation_enabled,
            ping_ms: config.ping_ms,
            fake_ping_ms: config.fake_ping_ms,
            player_count: players.len(),
        };
        self.draw_ui(ui_config);
    }

    fn draw_floor(&mut self) {
        let floor_y = FLOOR_Y;
        draw_rectangle(
            0.0,
            floor_y,
            self.width,
            self.height - floor_y,
            Color::from_rgba(68, 68, 68, 255),
        );
    }

    fn draw_player(&mut self, player: &Player, color: Color) {
        draw_rectangle(player.x, player.y, PLAYER_SIZE, PLAYER_SIZE, color);

        draw_rectangle_lines(player.x, player.y, PLAYER_SIZE, PLAYER_SIZE, 2.0, WHITE);
    }

    fn draw_velocity_vector(&mut self, player: &Player) {
        let center_x = player.x + PLAYER_SIZE / 2.0;
        let center_y = player.y + PLAYER_SIZE / 2.0;

        let vel_scale = 0.15;
        let end_x = center_x + player.vel_x * vel_scale;
        let end_y = center_y + player.vel_y * vel_scale;

        if player.vel_x.abs() > 10.0 || player.vel_y.abs() > 10.0 {
            draw_line(center_x, center_y, end_x, end_y, 2.0, YELLOW);
            self.draw_arrow_head(center_x, center_y, end_x, end_y);
        }
    }

    fn draw_arrow_head(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let length = (dx * dx + dy * dy).sqrt();

        if length < 5.0 {
            return;
        }

        let arrow_size = 5.0;
        let nx = dx / length;
        let ny = dy / length;

        let px = -ny;
        let py = nx;

        let base_x = x1 - nx * arrow_size;
        let base_y = y1 - ny * arrow_size;

        let left_x = base_x + px * (arrow_size / 2.0);
        let left_y = base_y + py * (arrow_size / 2.0);
        let right_x = base_x - px * (arrow_size / 2.0);
        let right_y = base_y - py * (arrow_size / 2.0);

        draw_line(x1, y1, left_x, left_y, 1.0, YELLOW);
        draw_line(x1, y1, right_x, right_y, 1.0, YELLOW);
    }

    fn draw_player_id(&mut self, player: &Player) {
        let id_color = match player.id % 8 {
            0 => WHITE,
            1 => RED,
            2 => GREEN,
            3 => BLUE,
            4 => YELLOW,
            5 => MAGENTA,
            6 => Color::from_rgba(0, 255, 255, 255),
            _ => Color::from_rgba(136, 136, 136, 255),
        };

        let id_x = player.x + PLAYER_SIZE / 2.0 - 2.0;
        let id_y = player.y - 8.0;

        draw_rectangle(id_x, id_y, 4.0, 4.0, id_color);
    }

    fn draw_ui(&mut self, config: UiConfig) {
        let y_start = 10.0;
        let indicator_size = 12.0;
        let spacing = 25.0;

        let features = [
            ("P", config.prediction_enabled),
            ("R", config.reconciliation_enabled),
            ("I", config.interpolation_enabled),
        ];

        for (i, (label, enabled)) in features.iter().enumerate() {
            let x = 10.0 + (i as f32) * spacing;
            let color = if *enabled { GREEN } else { RED };

            draw_rectangle(x, y_start, indicator_size, indicator_size, color);
            draw_rectangle_lines(x, y_start, indicator_size, indicator_size, 1.0, WHITE);

            draw_text(label, x + 3.0, y_start + indicator_size + 12.0, 12.0, WHITE);
        }

        let connection_color = if config.client_id.is_some() {
            GREEN
        } else {
            RED
        };
        draw_rectangle(10.0, y_start + 35.0, 8.0, 8.0, connection_color);
        draw_text("CON", 20.0, y_start + 35.0 + 8.0, 12.0, WHITE);

        let ping_y = y_start + 50.0;
        let total_ping = config.ping_ms + config.fake_ping_ms;
        let ping_bars = ((total_ping / 20).min(10)) as i32;

        for i in 0..10i32 {
            let bar_color = if i < ping_bars {
                if total_ping < 50 {
                    GREEN
                } else if total_ping < 100 {
                    YELLOW
                } else {
                    RED
                }
            } else {
                Color::from_rgba(51, 51, 51, 255)
            };

            draw_rectangle(10.0 + (i as f32) * 3.0, ping_y, 2.0, 8.0, bar_color);
        }

        let ping_text = format!("{}ms", total_ping);
        draw_text(&ping_text, 45.0, ping_y + 8.0, 12.0, WHITE);

        let player_y = ping_y + 15.0;
        for i in 0..(config.player_count.min(8)) {
            draw_rectangle(
                10.0 + (i as f32) * 4.0,
                player_y,
                3.0,
                3.0,
                Color::from_rgba(0, 170, 255, 255),
            );
        }
        let player_text = format!("{} players", config.player_count);
        draw_text(&player_text, 45.0, player_y + 3.0, 12.0, WHITE);
    }
}
