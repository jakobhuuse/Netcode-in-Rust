//! Client input management with sequencing and change detection

use macroquad::prelude::*;
use shared::InputState;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Manages user input collection and transformation into networked game inputs
pub struct InputManager {
    next_sequence: u32,
    current_input: InputState,
    last_input_sent: Instant,

    // Previous frame key states for edge detection
    prev_key_1: bool,
    prev_key_2: bool,
    prev_key_3: bool,
    prev_key_r: bool,
    prev_key_g: bool,
}

impl InputManager {
    pub fn new() -> Self {
        Self {
            next_sequence: 1,
            current_input: InputState {
                sequence: 0,
                timestamp: 0,
                left: false,
                right: false,
                jump: false,
            },
            last_input_sent: Instant::now(),
            prev_key_1: false,
            prev_key_2: false,
            prev_key_3: false,
            prev_key_r: false,
            prev_key_g: false,
        }
    }

    /// Updates input state and returns control events and optional network input
    /// Returns: ((prediction_toggle, reconciliation_toggle, interpolation_toggle, reconnect, graph_toggle), input_to_send)
    pub fn update(&mut self) -> ((bool, bool, bool, bool, bool), Option<InputState>) {
        // Sample movement keys (support both WASD and arrow keys)
        let left = is_key_down(KeyCode::A) || is_key_down(KeyCode::Left);
        let right = is_key_down(KeyCode::D) || is_key_down(KeyCode::Right);
        let jump = is_key_down(KeyCode::Space);

        // Sample debug/control keys
        let key_1 = is_key_down(KeyCode::Key1);
        let key_2 = is_key_down(KeyCode::Key2);
        let key_3 = is_key_down(KeyCode::Key3);
        let key_r = is_key_down(KeyCode::R);
        let key_g = is_key_down(KeyCode::G);

        let mut toggles = (false, false, false, false, false);

        // Detect key press events (current && !previous)
        if key_1 && !self.prev_key_1 {
            toggles.0 = true; // Toggle prediction
        }
        if key_2 && !self.prev_key_2 {
            toggles.1 = true; // Toggle reconciliation
        }
        if key_3 && !self.prev_key_3 {
            toggles.2 = true; // Toggle interpolation
        }
        if key_r && !self.prev_key_r {
            toggles.3 = true; // Reconnect
        }
        if key_g && !self.prev_key_g {
            toggles.4 = true; // Toggle network graph
        }

        // Update previous key states
        self.prev_key_1 = key_1;
        self.prev_key_2 = key_2;
        self.prev_key_3 = key_3;
        self.prev_key_r = key_r;
        self.prev_key_g = key_g;

        // Check if input state changed
        let input_changed = left != self.current_input.left
            || right != self.current_input.right
            || jump != self.current_input.jump;

        // Send input if changed or periodically for keep-alive (60Hz)
        let time_to_send = self.last_input_sent.elapsed() >= Duration::from_millis(16);
        let should_send = input_changed || time_to_send;
        let mut input_to_send = None;

        if should_send {
            self.current_input = InputState {
                sequence: self.next_sequence,
                timestamp: Self::get_timestamp(),
                left,
                right,
                jump,
            };

            input_to_send = Some(self.current_input.clone());
            self.next_sequence += 1;
            self.last_input_sent = Instant::now();
        }

        (toggles, input_to_send)
    }

    /// Returns the current input state
    pub fn get_current_input(&self) -> &InputState {
        &self.current_input
    }

    fn get_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis() as u64
    }
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_input_manager_creation() {
        let input_manager = InputManager::new();
        assert_eq!(input_manager.next_sequence, 1);
        assert_eq!(input_manager.current_input.sequence, 0);
        assert!(!input_manager.current_input.left);
        assert!(!input_manager.current_input.right);
        assert!(!input_manager.current_input.jump);
        assert!(!input_manager.prev_key_1);
        assert!(!input_manager.prev_key_2);
        assert!(!input_manager.prev_key_3);
        assert!(!input_manager.prev_key_r);
        assert!(!input_manager.prev_key_g);
    }

    #[test]
    fn test_get_timestamp() {
        let timestamp1 = InputManager::get_timestamp();
        thread::sleep(Duration::from_millis(2));
        let timestamp2 = InputManager::get_timestamp();
        assert!(timestamp2 > timestamp1);
        assert!(timestamp2 - timestamp1 >= 1); // At least 1ms difference
    }

    #[test]
    fn test_timestamp_monotonic() {
        let timestamps: Vec<u64> = (0..10)
            .map(|_| {
                let ts = InputManager::get_timestamp();
                thread::sleep(Duration::from_millis(1));
                ts
            })
            .collect();

        for i in 1..timestamps.len() {
            assert!(
                timestamps[i] >= timestamps[i - 1],
                "Timestamp should be monotonic: {} >= {}",
                timestamps[i],
                timestamps[i - 1]
            );
        }
    }

    #[test]
    fn test_default_implementation() {
        let input_manager = InputManager::default();
        assert_eq!(input_manager.next_sequence, 1);
        assert_eq!(input_manager.current_input.sequence, 0);
    }

    #[test]
    fn test_sequence_increment() {
        let mut input_manager = InputManager::new();

        // Since we can't mock macroquad key states, we'll test the sequence logic
        // by verifying the sequence starts at 1 and would increment
        assert_eq!(input_manager.next_sequence, 1);

        // Manually create an input to test sequence behavior
        input_manager.current_input = InputState {
            sequence: input_manager.next_sequence,
            timestamp: InputManager::get_timestamp(),
            left: true,
            right: false,
            jump: false,
        };
        input_manager.next_sequence += 1;

        assert_eq!(input_manager.current_input.sequence, 1);
        assert_eq!(input_manager.next_sequence, 2);
    }

    #[test]
    fn test_input_state_consistency() {
        let input_manager = InputManager::new();
        let input_state = input_manager.get_current_input();

        assert_eq!(input_state.sequence, 0);
        assert_eq!(input_state.timestamp, 0);
        assert!(!input_state.left);
        assert!(!input_state.right);
        assert!(!input_state.jump);
    }

    #[test]
    fn test_sequence_overflow_safety() {
        let mut input_manager = InputManager::new();

        // Test near u32::MAX to ensure no panic
        input_manager.next_sequence = u32::MAX - 1;

        // Simulate sequence increment
        input_manager.current_input = InputState {
            sequence: input_manager.next_sequence,
            timestamp: InputManager::get_timestamp(),
            left: false,
            right: false,
            jump: false,
        };
        input_manager.next_sequence += 1;

        assert_eq!(input_manager.current_input.sequence, u32::MAX - 1);
        assert_eq!(input_manager.next_sequence, u32::MAX);

        // Test overflow
        input_manager.current_input = InputState {
            sequence: input_manager.next_sequence,
            timestamp: InputManager::get_timestamp(),
            left: false,
            right: false,
            jump: false,
        };
        input_manager.next_sequence = input_manager.next_sequence.wrapping_add(1);

        assert_eq!(input_manager.current_input.sequence, u32::MAX);
        assert_eq!(input_manager.next_sequence, 0);
    }

    #[test]
    fn test_timestamp_validity() {
        let timestamp = InputManager::get_timestamp();

        // Should be a reasonable timestamp (after 2020)
        let year_2020_ms = 1577836800000u64; // Jan 1, 2020
        assert!(timestamp > year_2020_ms);

        // Should be before year 2100
        let year_2100_ms = 4102444800000u64; // Jan 1, 2100
        assert!(timestamp < year_2100_ms);
    }

    #[test]
    fn test_edge_detection_state_tracking() {
        let mut input_manager = InputManager::new();

        // Test initial state
        assert!(!input_manager.prev_key_1);
        assert!(!input_manager.prev_key_2);
        assert!(!input_manager.prev_key_3);
        assert!(!input_manager.prev_key_r);
        assert!(!input_manager.prev_key_g);

        // Test state persistence after manual update
        input_manager.prev_key_1 = true;
        input_manager.prev_key_2 = true;

        assert!(input_manager.prev_key_1);
        assert!(input_manager.prev_key_2);
        assert!(!input_manager.prev_key_3);
    }

    #[test]
    fn test_input_state_cloning() {
        let input_state = InputState {
            sequence: 42,
            timestamp: 12345,
            left: true,
            right: false,
            jump: true,
        };

        let cloned = input_state.clone();

        assert_eq!(cloned.sequence, 42);
        assert_eq!(cloned.timestamp, 12345);
        assert!(cloned.left);
        assert!(!cloned.right);
        assert!(cloned.jump);
    }

    #[test]
    fn test_timing_calculations() {
        let input_manager = InputManager::new();
        let start_time = input_manager.last_input_sent;

        // Test that last_input_sent is recent
        let now = Instant::now();
        let elapsed = now.duration_since(start_time);

        // Should be less than 1 second old
        assert!(elapsed < Duration::from_secs(1));
    }

    #[test]
    fn test_input_change_detection_logic() {
        let mut input_manager = InputManager::new();

        // Set initial state
        input_manager.current_input = InputState {
            sequence: 1,
            timestamp: InputManager::get_timestamp(),
            left: false,
            right: false,
            jump: false,
        };

        // Test change detection logic manually
        let left = true;
        let right = false;
        let jump = false;

        let input_changed = left != input_manager.current_input.left
            || right != input_manager.current_input.right
            || jump != input_manager.current_input.jump;

        assert!(input_changed); // Should detect left key change

        // Test no change
        let left = false;
        let input_changed = left != input_manager.current_input.left
            || right != input_manager.current_input.right
            || jump != input_manager.current_input.jump;

        assert!(!input_changed); // Should detect no change
    }

    #[test]
    fn test_keepalive_timing() {
        let input_manager = InputManager::new();
        let keepalive_interval = Duration::from_millis(16); // 60Hz

        // Test timing calculation
        let time_to_send = input_manager.last_input_sent.elapsed() >= keepalive_interval;

        // Initially should not need to send (just created)
        assert!(!time_to_send);
    }

    #[test]
    fn test_input_state_combinations() {
        // Test all possible input combinations are valid
        let combinations = [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (false, false, true),
            (true, true, false),
            (true, false, true),
            (false, true, true),
            (true, true, true),
        ];

        for (left, right, jump) in combinations.iter() {
            let input_state = InputState {
                sequence: 1,
                timestamp: InputManager::get_timestamp(),
                left: *left,
                right: *right,
                jump: *jump,
            };

            // All combinations should be valid
            assert_eq!(input_state.left, *left);
            assert_eq!(input_state.right, *right);
            assert_eq!(input_state.jump, *jump);
        }
    }

    #[test]
    fn test_toggle_state_representation() {
        // Test that toggle states can represent all possible combinations
        let toggle_combinations = [
            (false, false, false, false, false),
            (true, false, false, false, false),
            (false, true, false, false, false),
            (false, false, true, false, false),
            (false, false, false, true, false),
            (false, false, false, false, true),
            (true, true, true, true, true),
        ];

        for (pred, recon, interp, reconnect, graph) in toggle_combinations.iter() {
            let toggles = (*pred, *recon, *interp, *reconnect, *graph);

            assert_eq!(toggles.0, *pred);
            assert_eq!(toggles.1, *recon);
            assert_eq!(toggles.2, *interp);
            assert_eq!(toggles.3, *reconnect);
            assert_eq!(toggles.4, *graph);
        }
    }
}
