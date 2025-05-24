//! # Client Rendering System
//!
//! This module handles all visual rendering for the networked game client, including
//! player visualization, debug overlays, and status indicators. It provides a clean
//! interface between the game state and the visual presentation layer.
//!
//! ## Key Features
//!
//! - **Player Rendering**: Visual representation of all players with unique colors
//! - **Debug Visualization**: Velocity vectors and movement indicators for development
//! - **Netcode Status UI**: Real-time display of connection and feature states
//! - **Performance Monitoring**: Latency visualization and connection health
//! - **Player Identification**: Color-coded player IDs for easy identification
//!
//! ## Rendering Architecture
//!
//! The renderer follows a configuration-driven approach where all rendering decisions
//! are made based on the provided `RenderConfig` struct. This keeps the rendering
//! logic separate from game state management.
//!
//! ## Visual Elements
//!
//! - **Players**: Colored squares with borders and ID indicators
//! - **Floor**: Static ground plane for spatial reference
//! - **Velocity Vectors**: Debug arrows showing player movement (local player only)
//! - **UI Overlays**: Connection status, latency bars, and feature toggles
//!
//! ## Color Coding
//!
//! - **Green**: Local player, good connection status, enabled features
//! - **Red**: Remote players, poor connection, disabled features  
//! - **Yellow**: Velocity vectors, warnings, moderate latency
//! - **Blue**: Player count indicators
//! - **White**: Borders, labels, and neutral elements

use macroquad::prelude::*;
use shared::{Player, FLOOR_Y, PLAYER_SIZE};

/// Configuration for rendering a single frame.
///
/// Contains all the information needed to render the current game state,
/// including player data and netcode status indicators.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Local player ID (None if disconnected)
    pub client_id: Option<u32>,
    /// Whether client-side prediction is currently active
    pub prediction_enabled: bool,
    /// Whether server reconciliation is currently active
    pub reconciliation_enabled: bool,
    /// Whether interpolation smoothing is currently active
    pub interpolation_enabled: bool,
    /// Measured network round-trip latency in milliseconds
    pub ping_ms: u64,
    /// Simulated artificial latency in milliseconds (0 = disabled)
    pub fake_ping_ms: u64,
}

/// Extended configuration for UI rendering with additional statistical data.
///
/// Extends RenderConfig with information needed for comprehensive UI display.
#[derive(Debug, Clone)]
pub struct UiConfig {
    pub client_id: Option<u32>,
    pub prediction_enabled: bool,
    pub reconciliation_enabled: bool,
    pub interpolation_enabled: bool,
    pub ping_ms: u64,
    pub fake_ping_ms: u64,
    /// Number of players currently connected to the server
    pub player_count: usize,
}

/// Handles all game rendering including players, UI, and debug visualizations.
///
/// The Renderer is responsible for converting game state into visual elements.
/// It maintains no state between frames and operates purely based on the
/// provided configuration and player data.
///
/// ## Design Philosophy
/// - **Stateless**: No persistent rendering state between frames
/// - **Configuration-Driven**: All rendering decisions based on provided config
/// - **Modular**: Separate methods for different visual elements
/// - **Debug-Friendly**: Built-in visualization tools for development
pub struct Renderer {}

impl Renderer {
    /// Creates a new renderer instance.
    ///
    /// The renderer requires no initialization beyond creation, as it maintains
    /// no persistent state between frames.
    ///
    /// # Returns
    /// A Result containing the new Renderer instance
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Renderer {})
    }

    /// Renders a complete frame with players and UI elements.
    ///
    /// This is the main rendering entry point that orchestrates drawing all
    /// visual elements for a single frame. The rendering order is important:
    /// 1. Background (clear screen)
    /// 2. Static elements (floor)
    /// 3. Dynamic elements (players, velocity vectors)
    /// 4. UI overlays (status, debug info)
    ///
    /// # Arguments
    /// * `players` - Slice of all players to render
    /// * `config` - Rendering configuration including netcode status
    pub fn render(&mut self, players: &[Player], config: RenderConfig) {
        // Clear screen with dark background for good contrast
        clear_background(Color::from_rgba(26, 26, 26, 255));

        // Draw static elements first
        self.draw_floor();

        // Render all players with appropriate visual styling
        for player in players {
            let is_local_player = Some(player.id) == config.client_id;
            let color = if is_local_player {
                GREEN // Local player is easily identifiable
            } else {
                Color::from_rgba(255, 68, 68, 255) // Other players are red for contrast
            };

            self.draw_player(player, color);

            // Show velocity vector for local player only (debugging aid)
            if is_local_player {
                self.draw_velocity_vector(player);
            }

            // Draw player ID indicator for all players
            self.draw_player_id(player);
        }

        // Convert config and render comprehensive UI
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

    /// Draws the game floor/ground plane.
    ///
    /// Renders a static floor surface that provides spatial reference for players
    /// and helps establish the game world's coordinate system.
    fn draw_floor(&mut self) {
        let floor_y = FLOOR_Y;
        let current_width = screen_width();
        let current_height = screen_height();
        draw_rectangle(
            0.0,
            floor_y,
            current_width,
            current_height - floor_y,
            Color::from_rgba(68, 68, 68, 255),
        );
    }

    /// Renders a player as a colored square with border.
    ///
    /// Players are drawn as simple geometric shapes for clarity and performance.
    /// The white border ensures visibility against various backgrounds.
    ///
    /// # Arguments
    /// * `player` - Player data including position
    /// * `color` - Fill color for the player square
    fn draw_player(&mut self, player: &Player, color: Color) {
        draw_rectangle(player.x, player.y, PLAYER_SIZE, PLAYER_SIZE, color);
        // White border for visibility against any background
        draw_rectangle_lines(player.x, player.y, PLAYER_SIZE, PLAYER_SIZE, 2.0, WHITE);
    }

    /// Draws velocity vector for debugging player movement.
    ///
    /// Renders an arrow indicating the player's current velocity direction and
    /// magnitude. This is a valuable debugging tool for understanding movement
    /// behavior and physics interactions.
    ///
    /// Only shown if velocity magnitude exceeds a minimum threshold to avoid
    /// visual clutter from micro-movements.
    ///
    /// # Arguments
    /// * `player` - Player data including position and velocity
    fn draw_velocity_vector(&mut self, player: &Player) {
        let center_x = player.x + PLAYER_SIZE / 2.0;
        let center_y = player.y + PLAYER_SIZE / 2.0;

        // Scale velocity for visibility (15% of actual magnitude)
        let vel_scale = 0.15;
        let end_x = center_x + player.vel_x * vel_scale;
        let end_y = center_y + player.vel_y * vel_scale;

        // Only draw if moving significantly (avoids clutter from micro-movements)
        if player.vel_x.abs() > 10.0 || player.vel_y.abs() > 10.0 {
            draw_line(center_x, center_y, end_x, end_y, 2.0, YELLOW);
            self.draw_arrow_head(center_x, center_y, end_x, end_y);
        }
    }

    /// Draws arrowhead at the end of velocity vector.
    ///
    /// Creates a proper directional arrow by adding wing lines to the velocity
    /// vector. The arrowhead size is proportional to the vector length.
    ///
    /// # Arguments
    /// * `x0, y0` - Starting point of the vector (player center)
    /// * `x1, y1` - End point of the vector
    fn draw_arrow_head(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let length = (dx * dx + dy * dy).sqrt();

        // Don't draw arrowhead for very short vectors to avoid visual artifacts
        if length < 5.0 {
            return;
        }

        let arrow_size = 5.0;
        let nx = dx / length; // Normalized direction vector
        let ny = dy / length;

        // Perpendicular vector for arrow wing calculation
        let px = -ny;
        let py = nx;

        // Calculate arrowhead wing points
        let base_x = x1 - nx * arrow_size;
        let base_y = y1 - ny * arrow_size;

        let left_x = base_x + px * (arrow_size / 2.0);
        let left_y = base_y + py * (arrow_size / 2.0);
        let right_x = base_x - px * (arrow_size / 2.0);
        let right_y = base_y - py * (arrow_size / 2.0);

        // Draw arrow wings to complete the directional indicator
        draw_line(x1, y1, left_x, left_y, 1.0, YELLOW);
        draw_line(x1, y1, right_x, right_y, 1.0, YELLOW);
    }

    /// Draws a small colored square above each player showing their ID.
    ///
    /// Each player gets a unique color based on their ID for easy identification
    /// in multiplayer scenarios. The indicator appears above the player sprite.
    ///
    /// # Arguments
    /// * `player` - Player data including ID and position
    fn draw_player_id(&mut self, player: &Player) {
        // Cycle through colors based on player ID for consistent identification
        let id_color = match player.id % 8 {
            0 => WHITE,
            1 => RED,
            2 => GREEN,
            3 => BLUE,
            4 => YELLOW,
            5 => MAGENTA,
            6 => Color::from_rgba(0, 255, 255, 255), // Cyan
            _ => Color::from_rgba(136, 136, 136, 255), // Gray
        };

        // Position indicator above player sprite
        let id_x = player.x + PLAYER_SIZE / 2.0 - 2.0;
        let id_y = player.y - 8.0;

        draw_rectangle(id_x, id_y, 4.0, 4.0, id_color);
    }

    /// Renders comprehensive debug UI showing netcode status and connection info.
    ///
    /// The UI provides real-time feedback about:
    /// - **Netcode Features**: Visual indicators for prediction, reconciliation, interpolation
    /// - **Connection Status**: Connected/disconnected state with reconnect hints
    /// - **Latency Monitoring**: Bar graph visualization of network ping
    /// - **Player Count**: Visual representation of connected players
    ///
    /// Layout is designed to be non-intrusive while providing essential debugging
    /// information for development and testing.
    ///
    /// # Arguments
    /// * `config` - UI configuration including all status information
    fn draw_ui(&mut self, config: UiConfig) {
        let y_start = 10.0;
        let indicator_size = 12.0;
        let spacing = 25.0;

        // Draw netcode feature indicators: P(rediction), R(econciliation), I(nterpolation)
        let features = [
            ("P", config.prediction_enabled),
            ("R", config.reconciliation_enabled),
            ("I", config.interpolation_enabled),
        ];

        for (i, (label, enabled)) in features.iter().enumerate() {
            let x = 10.0 + (i as f32) * spacing;
            let color = if *enabled { GREEN } else { RED };

            // Draw feature status indicator box
            draw_rectangle(x, y_start, indicator_size, indicator_size, color);
            draw_rectangle_lines(x, y_start, indicator_size, indicator_size, 1.0, WHITE);

            // Draw feature label below indicator
            draw_text(label, x + 3.0, y_start + indicator_size + 12.0, 12.0, WHITE);
        }

        // Connection status indicator with clear visual feedback
        let connection_color = if config.client_id.is_some() {
            GREEN // Connected
        } else {
            RED // Disconnected
        };
        draw_rectangle(10.0, y_start + 35.0, 8.0, 8.0, connection_color);
        let connection_text = if config.client_id.is_some() {
            "CON" // Connected
        } else {
            "DIS" // Disconnected
        };
        draw_text(connection_text, 20.0, y_start + 35.0 + 8.0, 12.0, WHITE);

        // Show helpful reconnect hint when disconnected
        if config.client_id.is_none() {
            draw_text("Press R to reconnect", 10.0, y_start + 55.0, 12.0, YELLOW);
        }

        // Ping visualization using bar graph for intuitive understanding
        let ping_y = if config.client_id.is_none() {
            y_start + 70.0
        } else {
            y_start + 50.0
        };
        let total_ping = if config.fake_ping_ms > 0 {
            config.fake_ping_ms // Use simulated ping if artificial latency is enabled
        } else {
            config.ping_ms // Use actual measured ping
        };
        // Convert ping to bar count (20ms per bar, max 10 bars = 200ms+)
        let ping_bars = ((total_ping / 20).min(10)) as i32;

        // Draw ping bars with color-coded quality indication
        for i in 0..10i32 {
            let bar_color = if i < ping_bars {
                if total_ping < 50 {
                    GREEN // Excellent ping (< 50ms)
                } else if total_ping < 100 {
                    YELLOW // Good ping (50-100ms)
                } else {
                    RED // Poor ping (> 100ms)
                }
            } else {
                Color::from_rgba(51, 51, 51, 255) // Empty bar (dark gray)
            };

            draw_rectangle(10.0 + (i as f32) * 3.0, ping_y, 2.0, 8.0, bar_color);
        }

        let ping_text = format!("{}ms", total_ping);
        draw_text(&ping_text, 45.0, ping_y + 8.0, 12.0, WHITE);

        // Player count visualization using small squares
        let player_y = ping_y + 15.0;
        for i in 0..(config.player_count.min(8)) {
            // Max 8 squares to fit UI space
            draw_rectangle(
                10.0 + (i as f32) * 4.0,
                player_y,
                3.0,
                3.0,
                Color::from_rgba(0, 170, 255, 255), // Blue squares for players
            );
        }
        let player_text = format!("{} players", config.player_count);
        draw_text(&player_text, 45.0, player_y + 3.0, 12.0, WHITE);
    }
}
