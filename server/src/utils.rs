use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Get current timestamp in milliseconds
pub fn get_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}

// Generate random colors based on client ID
pub fn generate_color(client_id: u32) -> String {
    let colors = ["blue", "red", "green", "purple", "orange", "cyan", "magenta", "yellow"];
    colors[(client_id as usize - 1) % colors.len()].to_string()
}

// Calculate normalized vector
pub fn normalize_vector(x: f32, y: f32) -> (f32, f32) {
    let magnitude = (x * x + y * y).sqrt();
    if magnitude > 0.0 {
        (x / magnitude, y / magnitude)
    } else {
        (0.0, 0.0)
    }
}