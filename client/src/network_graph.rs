//! Network performance graph for real-time network diagnostics

use macroquad::prelude::*;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Network performance metrics collected over time
#[derive(Debug, Clone)]
pub struct NetworkMetrics {
    pub ping_ms: f32,
    pub packet_loss_percent: f32,
    pub jitter_ms: f32,
    pub timestamp: Instant,
}

impl Default for NetworkMetrics {
    fn default() -> Self {
        Self {
            ping_ms: 0.0,
            packet_loss_percent: 0.0,
            jitter_ms: 0.0,
            timestamp: Instant::now(),
        }
    }
}

/// Real-time network performance graph
pub struct NetworkGraph {
    // Historical data storage
    metrics_history: VecDeque<NetworkMetrics>,
    max_samples: usize,
    sample_interval: Duration,
    last_sample_time: Instant,
    
    // Graph display settings
    graph_width: f32,
    graph_height: f32,
    visible: bool,
    internal_padding: f32,
    
    // Packet tracking for loss calculation
    packets_sent: u32,
    packets_received: u32,
    last_packet_count_reset: Instant,
    
    // Jitter calculation
    last_ping: Option<f32>,
    jitter_accumulator: f32,
    jitter_samples: u32,
    
    // Graph scaling
    ping_scale_max: f32,
    auto_scale: bool,
}

impl NetworkGraph {
    pub fn new() -> Self {
        Self {
            metrics_history: VecDeque::new(),
            max_samples: 100, // Store last 100 samples (10 seconds at 100ms intervals)
            sample_interval: Duration::from_millis(100), // Sample every 100ms
            last_sample_time: Instant::now(),
            
            graph_width: 300.0,
            graph_height: 120.0,
            visible: false,
            internal_padding: 12.0,
            
            packets_sent: 0,
            packets_received: 0,
            last_packet_count_reset: Instant::now(),
            
            last_ping: None,
            jitter_accumulator: 0.0,
            jitter_samples: 0,
            
            ping_scale_max: 100.0,
            auto_scale: true,
        }
    }
    
    /// Toggle graph visibility
    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }
    
    /// Check if graph is currently visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }
    
    /// Record a packet being sent (for packet loss calculation)
    pub fn record_packet_sent(&mut self) {
        self.packets_sent += 1;
    }
    
    /// Record a packet being received with ping data
    pub fn record_packet_received(&mut self, ping_ms: f32) {
        self.packets_received += 1;
        
        // Calculate jitter (ping variance)
        if let Some(last_ping) = self.last_ping {
            let ping_diff = (ping_ms - last_ping).abs();
            self.jitter_accumulator += ping_diff;
            self.jitter_samples += 1;
        }
        self.last_ping = Some(ping_ms);
        
        // Sample metrics at fixed intervals for consistent graph
        let now = Instant::now();
        if now.duration_since(self.last_sample_time) >= self.sample_interval {
            self.sample_metrics(ping_ms);
            self.last_sample_time = now;
        }
    }
    
    /// Sample current network metrics and add to history
    fn sample_metrics(&mut self, current_ping: f32) {
        let now = Instant::now();
        
        // Calculate packet loss percentage over last sample period
        let packet_loss = if self.packets_sent > 0 {
            let lost_packets = self.packets_sent.saturating_sub(self.packets_received);
            (lost_packets as f32 / self.packets_sent as f32) * 100.0
        } else {
            0.0
        };
        
        // Calculate average jitter over sample period
        let jitter = if self.jitter_samples > 0 {
            self.jitter_accumulator / self.jitter_samples as f32
        } else {
            0.0
        };
        
        let metrics = NetworkMetrics {
            ping_ms: current_ping,
            packet_loss_percent: packet_loss.min(100.0),
            jitter_ms: jitter,
            timestamp: now,
        };
        
        self.metrics_history.push_back(metrics);
        
        // Maintain max samples
        while self.metrics_history.len() > self.max_samples {
            self.metrics_history.pop_front();
        }
        
        // Reset packet counters periodically to prevent overflow
        if now.duration_since(self.last_packet_count_reset) >= Duration::from_secs(10) {
            self.packets_sent = 0;
            self.packets_received = 0;
            self.last_packet_count_reset = now;
            self.jitter_accumulator = 0.0;
            self.jitter_samples = 0;
        }
        
        // Auto-scale the graph if enabled
        if self.auto_scale {
            self.update_auto_scale();
        }
    }
    
    /// Update ping scale based on recent data
    fn update_auto_scale(&mut self) {
        if self.metrics_history.is_empty() {
            return;
        }
        
        let max_ping = self.metrics_history
            .iter()
            .map(|m| m.ping_ms)
            .fold(0.0f32, f32::max);
        
        // Set scale to accommodate highest ping with some headroom
        let desired_scale = (max_ping * 1.2).max(50.0);
        
        // Smooth scale changes to prevent jittery scaling
        self.ping_scale_max = self.ping_scale_max * 0.9 + desired_scale * 0.1;
    }
    
    /// Render the network graph in the top-right corner
    pub fn render(&self) {
        if !self.visible || self.metrics_history.is_empty() {
            return;
        }
        
        let screen_w = screen_width();
        let base_margin = 20.0;
        let label_space = 40.0;
        let legend_space = 50.0;
        
        // Calculate consistent margins - both top and right need same base margin
        // plus their respective extra spaces
        let right_margin = base_margin + label_space;
        let top_margin = base_margin + legend_space;
        
        // Background position (includes all needed space)
        let bg_x = screen_w - self.graph_width - right_margin;
        let bg_y = top_margin;
        
        // Graph content position (with internal padding)
        let graph_x = bg_x + self.internal_padding;
        let graph_y = bg_y + self.internal_padding;
        
        self.draw_background(bg_x, bg_y);
        self.draw_legend(bg_x, bg_y);
        
        // Graph content uses padded position
        self.draw_grid(graph_x, graph_y);
        self.draw_ping_line(graph_x, graph_y);
        self.draw_packet_loss_bars(graph_x, graph_y);
        self.draw_labels(graph_x, graph_y);
    }
    
    /// Draw semi-transparent background
    fn draw_background(&self, x: f32, y: f32) {
        let background_padding = 8.0;
        let label_space = 40.0;
        let legend_space = 50.0;
        let bottom_space = 30.0;
        
        draw_rectangle(
            x - background_padding,
            y - legend_space,
            self.graph_width + background_padding * 2.0 + label_space,
            self.graph_height + legend_space + bottom_space,
            Color::from_rgba(0, 0, 0, 200)
        );
        
        // Border
        draw_rectangle_lines(
            x - background_padding,
            y - legend_space,
            self.graph_width + background_padding * 2.0 + label_space,
            self.graph_height + legend_space + bottom_space,
            1.0,
            Color::from_rgba(120, 120, 120, 255)
        );
    }
    
    /// Draw grid lines for better readability
    fn draw_grid(&self, x: f32, y: f32) {
        let grid_color = Color::from_rgba(50, 50, 50, 255);
        let usable_width = self.graph_width - (self.internal_padding * 2.0);
        let usable_height = self.graph_height - (self.internal_padding * 2.0);
        
        // Horizontal grid lines (ping levels)
        let ping_intervals = [25.0, 50.0, 100.0, 150.0, 200.0];
        for &ping_level in &ping_intervals {
            if ping_level <= self.ping_scale_max {
                let grid_y = y + usable_height - (ping_level / self.ping_scale_max * usable_height);
                draw_line(x, grid_y, x + usable_width, grid_y, 1.0, grid_color);
            }
        }
        
        // Vertical grid lines (time intervals)
        let time_divisions = 5;
        let time_span_ms = self.get_time_span_ms();
        
        if time_span_ms > 0.0 {
            for i in 1..time_divisions {
                let grid_x = x + (i as f32 / time_divisions as f32) * usable_width;
                draw_line(grid_x, y, grid_x, y + usable_height, 1.0, grid_color);
                
                let time_offset_ms = (i as f32 / time_divisions as f32) * time_span_ms;
                let time_ago_ms = time_span_ms - time_offset_ms;
                let time_label = if time_ago_ms > 1000.0 {
                    format!("-{:.1}s", time_ago_ms / 1000.0)
                } else {
                    format!("-{:.0}ms", time_ago_ms)
                };
                draw_text(&time_label, grid_x - 15.0, y + usable_height + 12.0, 9.0, Color::from_rgba(180, 180, 180, 255));
            }
            
            draw_text("now", x + usable_width - 12.0, y + usable_height + 12.0, 9.0, Color::from_rgba(180, 180, 180, 255));
        }
    }
    
    /// Draw ping as a continuous line graph
    fn draw_ping_line(&self, x: f32, y: f32) {
        if self.metrics_history.len() < 2 {
            return;
        }
        
        let usable_width = self.graph_width - (self.internal_padding * 2.0);
        let usable_height = self.graph_height - (self.internal_padding * 2.0);
        
        // Calculate time span for proper X-axis scaling
        let time_span_ms = self.get_time_span_ms();
        if time_span_ms <= 0.0 {
            return;
        }
        
        // Get the oldest timestamp as our reference point
        let oldest_timestamp = self.metrics_history.front().unwrap().timestamp;
        
        for i in 1..self.metrics_history.len() {
            let prev_metrics = &self.metrics_history[i - 1];
            let curr_metrics = &self.metrics_history[i];
            
            // Calculate X positions based on time differences
            let prev_time_offset = prev_metrics.timestamp.duration_since(oldest_timestamp).as_millis() as f32;
            let curr_time_offset = curr_metrics.timestamp.duration_since(oldest_timestamp).as_millis() as f32;
            
            let x1 = x + (prev_time_offset / time_span_ms) * usable_width;
            let y1 = y + usable_height - (prev_metrics.ping_ms / self.ping_scale_max * usable_height);
            let x2 = x + (curr_time_offset / time_span_ms) * usable_width;
            let y2 = y + usable_height - (curr_metrics.ping_ms / self.ping_scale_max * usable_height);
            
            // Color based on ping quality (these are the colored lines you see)
            let ping_color = if curr_metrics.ping_ms < 30.0 {
                GREEN
            } else if curr_metrics.ping_ms < 60.0 {
                YELLOW
            } else if curr_metrics.ping_ms < 100.0 {
                ORANGE
            } else {
                RED
            };
            
            draw_line(x1, y1, x2, y2, 2.0, ping_color);
        }
    }
    
    /// Draw packet loss as colored bars
    fn draw_packet_loss_bars(&self, x: f32, y: f32) {
        let usable_width = self.graph_width - (self.internal_padding * 2.0);
        let usable_height = self.graph_height - (self.internal_padding * 2.0);
        
        // Calculate time span for proper X-axis scaling
        let time_span_ms = self.get_time_span_ms();
        if time_span_ms <= 0.0 {
            return;
        }
        
        // Get the oldest timestamp as our reference point
        let oldest_timestamp = self.metrics_history.front().unwrap().timestamp;
        let bar_width = (usable_width / self.max_samples as f32).max(2.0); // Minimum bar width
        
        for metrics in self.metrics_history.iter() {
            if metrics.packet_loss_percent > 0.1 {
                // Calculate X position based on time
                let time_offset = metrics.timestamp.duration_since(oldest_timestamp).as_millis() as f32;
                let bar_x = x + (time_offset / time_span_ms) * usable_width;
                let bar_height = (metrics.packet_loss_percent / 10.0 * usable_height * 0.3).min(usable_height * 0.3);
                let bar_y = y + usable_height - bar_height;
                
                let loss_color = if metrics.packet_loss_percent < 1.0 {
                    Color::from_rgba(255, 255, 0, 150)
                } else {
                    Color::from_rgba(255, 0, 0, 200)
                };
                
                draw_rectangle(bar_x - bar_width / 2.0, bar_y, bar_width * 0.8, bar_height, loss_color);
            }
        }
    }
    
    /// Draw scale labels and current values
    fn draw_labels(&self, x: f32, y: f32) {
        let label_color = WHITE;
        let font_size = 11.0;
        let usable_width = self.graph_width - (self.internal_padding * 2.0);
        let usable_height = self.graph_height - (self.internal_padding * 2.0);
        
        let ping_levels = [0.0, self.ping_scale_max * 0.25, self.ping_scale_max * 0.5, self.ping_scale_max * 0.75, self.ping_scale_max];
        for &ping_level in &ping_levels {
            let label_y = y + usable_height - (ping_level / self.ping_scale_max * usable_height);
            let label_text = if ping_level == 0.0 {
                "0ms".to_string()
            } else {
                format!("{:.0}ms", ping_level)
            };
            draw_text(&label_text, x + usable_width + 8.0, label_y + 4.0, font_size, label_color);
        }
        
        if let Some(latest) = self.metrics_history.back() {
            let current_info = format!(
                "Ping: {:.0}ms | Loss: {:.1}% | Jitter: {:.1}ms",
                latest.ping_ms,
                latest.packet_loss_percent,
                latest.jitter_ms
            );
            
            draw_text(&current_info, x - self.internal_padding, y + usable_height + 28.0, font_size, label_color);
        }
    }
    
    /// Draw legend explaining the graph elements
    fn draw_legend(&self, x: f32, y: f32) {
        let legend_space = 50.0;
        let legend_y = y - legend_space + 10.0;
        let font_size = 11.0;
        
        // Title with explanation
        draw_text("Network Graph (G to toggle)", x, legend_y, font_size, WHITE);
        
        // Explanation of different visual elements
        let explanation_y = legend_y + 12.0;
        draw_text("Line = Ping | Red bars = Packet loss", x, explanation_y, 10.0, Color::from_rgba(180, 180, 180, 255));
        
        // Current ping quality indicator
        if let Some(latest) = self.metrics_history.back() {
            let ping_explanation = if latest.ping_ms < 30.0 {
                "Excellent"
            } else if latest.ping_ms < 60.0 {
                "Good"
            } else if latest.ping_ms < 100.0 {
                "Fair"
            } else {
                "Poor"
            };
            
            let quality_color = if latest.ping_ms < 30.0 { GREEN }
            else if latest.ping_ms < 60.0 { YELLOW }
            else if latest.ping_ms < 100.0 { ORANGE }
            else { RED };
            
            draw_text("Quality:", x, explanation_y + 12.0, 10.0, WHITE);
            draw_text(ping_explanation, x + 45.0, explanation_y + 12.0, 10.0, quality_color);
        }
    }
    
    /// Calculate the time span covered by the current metrics history in milliseconds
    fn get_time_span_ms(&self) -> f32 {
        if self.metrics_history.len() < 2 {
            return 0.0;
        }
        
        let oldest = self.metrics_history.front().unwrap().timestamp;
        let newest = self.metrics_history.back().unwrap().timestamp;
        newest.duration_since(oldest).as_millis() as f32
    }
}

impl Default for NetworkGraph {
    fn default() -> Self {
        Self::new()
    }
}