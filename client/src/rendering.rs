use minifb::{Scale, Window, WindowOptions};
use shared::{Player, FLOOR_Y, PLAYER_SIZE};

pub struct Renderer {
    pub window: Window,
    buffer: Vec<u32>,
    width: usize,
    height: usize,
}

impl Renderer {
    pub fn new(width: usize, height: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let window = Window::new(
            "Rollback Netcode Demo - Use A/D to move, Space to jump, 1/2/3 to toggle features",
            width,
            height,
            WindowOptions {
                scale: Scale::X1,
                ..WindowOptions::default()
            },
        )?;

        Ok(Renderer {
            window,
            buffer: vec![0; width * height],
            width,
            height,
        })
    }

    pub fn render(
        &mut self,
        players: &[Player],
        client_id: Option<u32>,
        prediction_enabled: bool,
        reconciliation_enabled: bool,
        interpolation_enabled: bool,
        ping_ms: u64,
        fake_ping_ms: u64,
    ) {
        for pixel in &mut self.buffer {
            *pixel = 0x1a1a1a;
        }

        self.draw_floor();

        for player in players {
            let is_local_player = Some(player.id) == client_id;
            let color = if is_local_player { 0x00ff00 } else { 0xff4444 };

            self.draw_player(player, color);

            if is_local_player {
                self.draw_velocity_vector(player);
            }

            self.draw_player_id(player);
        }

        self.draw_ui(
            client_id,
            prediction_enabled,
            reconciliation_enabled,
            interpolation_enabled,
            ping_ms,
            fake_ping_ms,
            players.len(),
        );

        if let Err(e) = self
            .window
            .update_with_buffer(&self.buffer, self.width, self.height)
        {
            eprintln!("Failed to update window: {}", e);
        }
    }

    fn draw_floor(&mut self) {
        let floor_y = FLOOR_Y as usize;
        for y in floor_y..self.height {
            for x in 0..self.width {
                if let Some(pixel) = self.get_pixel_mut(x, y) {
                    *pixel = 0x444444;
                }
            }
        }
    }

    fn draw_player(&mut self, player: &Player, color: u32) {
        self.draw_rect(
            player.x as i32,
            player.y as i32,
            PLAYER_SIZE as i32,
            PLAYER_SIZE as i32,
            color,
        );

        self.draw_rect_outline(
            player.x as i32,
            player.y as i32,
            PLAYER_SIZE as i32,
            PLAYER_SIZE as i32,
            0xffffff,
        );
    }

    fn draw_velocity_vector(&mut self, player: &Player) {
        let center_x = player.x + PLAYER_SIZE / 2.0;
        let center_y = player.y + PLAYER_SIZE / 2.0;

        let vel_scale = 0.15;
        let end_x = center_x + player.vel_x * vel_scale;
        let end_y = center_y + player.vel_y * vel_scale;

        if player.vel_x.abs() > 10.0 || player.vel_y.abs() > 10.0 {
            self.draw_line(
                center_x as i32,
                center_y as i32,
                end_x as i32,
                end_y as i32,
                0xffff00,
                2,
            );

            self.draw_arrow_head(
                center_x as i32,
                center_y as i32,
                end_x as i32,
                end_y as i32,
                0xffff00,
            );
        }
    }

    fn draw_player_id(&mut self, player: &Player) {
        let id_color = match player.id % 8 {
            0 => 0xffffff,
            1 => 0xff0000,
            2 => 0x00ff00,
            3 => 0x0000ff,
            4 => 0xffff00,
            5 => 0xff00ff,
            6 => 0x00ffff,
            _ => 0x888888,
        };

        self.draw_rect(
            (player.x + PLAYER_SIZE / 2.0 - 2.0) as i32,
            (player.y - 8.0) as i32,
            4,
            4,
            id_color,
        );
    }

    fn draw_ui(
        &mut self,
        client_id: Option<u32>,
        prediction_enabled: bool,
        reconciliation_enabled: bool,
        interpolation_enabled: bool,
        ping_ms: u64,
        fake_ping_ms: u64,
        player_count: usize,
    ) {
        let y_start = 10i32;
        let indicator_size = 12i32;
        let spacing = 20i32;

        let features = [
            ("P", prediction_enabled),
            ("R", reconciliation_enabled),
            ("I", interpolation_enabled),
        ];

        for (i, (_label, enabled)) in features.iter().enumerate() {
            let x = 10i32 + (i as i32) * spacing;
            let color = if *enabled { 0x00ff00 } else { 0xff0000 };

            self.draw_rect(x, y_start, indicator_size, indicator_size, color);
            self.draw_rect_outline(x, y_start, indicator_size, indicator_size, 0xffffff);
        }

        let connection_color = if client_id.is_some() {
            0x00ff00
        } else {
            0xff0000
        };
        self.draw_rect(10, y_start + 25, 8, 8, connection_color);

        let ping_y = y_start + 40;
        let total_ping = ping_ms + fake_ping_ms;
        let ping_bars = ((total_ping / 20).min(10)) as i32;

        for i in 0..10i32 {
            let bar_color = if i < ping_bars {
                if total_ping < 50 {
                    0x00ff00
                } else if total_ping < 100 {
                    0xffff00
                } else {
                    0xff0000
                }
            } else {
                0x333333
            };

            self.draw_rect(10 + i * 3, ping_y, 2, 8, bar_color);
        }

        let player_y = ping_y + 15;
        for i in 0..(player_count.min(8) as i32) {
            self.draw_rect(10 + i * 4, player_y, 3, 3, 0x00aaff);
        }
    }

    fn draw_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        for dy in 0..h {
            for dx in 0..w {
                let px = x + dx;
                let py = y + dy;
                if px >= 0 && py >= 0 && (px as usize) < self.width && (py as usize) < self.height {
                    if let Some(pixel) = self.get_pixel_mut(px as usize, py as usize) {
                        *pixel = color;
                    }
                }
            }
        }
    }

    fn draw_rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        for dx in 0..w {
            let px = x + dx;
            if px >= 0 && y >= 0 && (px as usize) < self.width && (y as usize) < self.height {
                if let Some(pixel) = self.get_pixel_mut(px as usize, y as usize) {
                    *pixel = color;
                }
            }
            let bottom_y = y + h - 1;
            if px >= 0
                && bottom_y >= 0
                && (px as usize) < self.width
                && (bottom_y as usize) < self.height
            {
                if let Some(pixel) = self.get_pixel_mut(px as usize, bottom_y as usize) {
                    *pixel = color;
                }
            }
        }

        for dy in 0..h {
            let py = y + dy;
            if x >= 0 && py >= 0 && (x as usize) < self.width && (py as usize) < self.height {
                if let Some(pixel) = self.get_pixel_mut(x as usize, py as usize) {
                    *pixel = color;
                }
            }
            let right_x = x + w - 1;
            if right_x >= 0
                && py >= 0
                && (right_x as usize) < self.width
                && (py as usize) < self.height
            {
                if let Some(pixel) = self.get_pixel_mut(right_x as usize, py as usize) {
                    *pixel = color;
                }
            }
        }
    }

    fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u32, thickness: i32) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            for dt in -(thickness / 2)..=(thickness / 2) {
                for dt2 in -(thickness / 2)..=(thickness / 2) {
                    let px = x + dt;
                    let py = y + dt2;
                    if px >= 0
                        && py >= 0
                        && (px as usize) < self.width
                        && (py as usize) < self.height
                    {
                        if let Some(pixel) = self.get_pixel_mut(px as usize, py as usize) {
                            *pixel = color;
                        }
                    }
                }
            }

            if x == x1 && y == y1 {
                break;
            }

            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
    }

    fn draw_arrow_head(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let length = ((dx * dx + dy * dy) as f32).sqrt();

        if length < 5.0 {
            return;
        }

        let arrow_size = 5;
        let nx = dx as f32 / length;
        let ny = dy as f32 / length;

        let px = -ny;
        let py = nx;

        let tip_x = x1;
        let tip_y = y1;
        let base_x = x1 - (nx * arrow_size as f32) as i32;
        let base_y = y1 - (ny * arrow_size as f32) as i32;

        let left_x = base_x + (px * (arrow_size as f32 / 2.0)) as i32;
        let left_y = base_y + (py * (arrow_size as f32 / 2.0)) as i32;
        let right_x = base_x - (px * (arrow_size as f32 / 2.0)) as i32;
        let right_y = base_y - (py * (arrow_size as f32 / 2.0)) as i32;

        self.draw_line(tip_x, tip_y, left_x, left_y, color, 1);
        self.draw_line(tip_x, tip_y, right_x, right_y, color, 1);
    }

    fn get_pixel_mut(&mut self, x: usize, y: usize) -> Option<&mut u32> {
        if x < self.width && y < self.height {
            self.buffer.get_mut(x + y * self.width)
        } else {
            None
        }
    }

    pub fn is_open(&self) -> bool {
        self.window.is_open()
    }

    pub fn get_keys(&self) -> Vec<minifb::Key> {
        self.window.get_keys()
    }
}
