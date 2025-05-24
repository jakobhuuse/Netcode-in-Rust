//! # Client Input Management
//!
//! This module handles user input collection, processing, and transmission for the
//! networked game client. It provides a clean abstraction layer between raw keyboard
//! input and the game's networking system.
//!
//! ## Key Features
//!
//! - **Input Sequencing**: Assigns monotonic sequence numbers to track input order
//! - **State Change Detection**: Only sends inputs when state actually changes
//! - **Keep-Alive Mechanism**: Periodically sends inputs to maintain connection
//! - **Debug Controls**: Handles special keys for toggling netcode features
//! - **Multi-Key Support**: Supports both WASD and arrow key movement schemes
//!
//! ## Input Flow
//!
//! 1. **Sampling**: Raw keyboard state is sampled each frame
//! 2. **Edge Detection**: Toggle keys are processed for press events (not hold)
//! 3. **Change Detection**: Movement state is compared to previous frame
//! 4. **Transmission Logic**: Inputs are sent when changed or on timeout
//! 5. **Sequencing**: Each sent input gets a unique, incrementing sequence number
//!
//! ## Debug Controls
//!
//! The input manager supports several debug keys for testing netcode features:
//! - **1**: Toggle client-side prediction on/off
//! - **2**: Toggle server reconciliation on/off  
//! - **3**: Toggle interpolation on/off
//! - **R**: Force client reconnection

use macroquad::prelude::*;
use shared::InputState;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Manages user input collection and transformation into networked game inputs.
///
/// The InputManager serves as the bridge between raw keyboard input and the game's
/// networking layer. It implements several critical netcode concepts:
///
/// ## Input Sequencing
/// Each input is assigned a monotonically increasing sequence number, allowing
/// the server to process inputs in the correct order even if packets arrive
/// out of sequence due to network conditions.
///
/// ## Efficient Transmission
/// Inputs are only sent when the movement state changes or after a timeout period.
/// This reduces network bandwidth while maintaining connection health.
///
/// ## Debug Controls
/// Special keys allow runtime toggling of netcode features for testing and
/// demonstration purposes.
pub struct InputManager {
    /// Monotonically increasing sequence number for inputs
    /// Ensures server can process inputs in correct chronological order
    next_sequence: u32,

    /// Last captured input state
    /// Used for change detection to minimize network traffic
    current_input: InputState,

    /// Timestamp of last input transmission
    /// Used for keep-alive mechanism to prevent connection timeouts
    last_input_sent: Instant,

    // Previous frame key states for edge detection (press events, not hold)
    /// Previous state of '1' key for prediction toggle detection
    prev_key_1: bool,
    /// Previous state of '2' key for reconciliation toggle detection
    prev_key_2: bool,
    /// Previous state of '3' key for interpolation toggle detection
    prev_key_3: bool,
    /// Previous state of 'R' key for reconnect command detection
    prev_key_r: bool,
}

impl InputManager {
    /// Creates a new input manager with default state.
    ///
    /// Initializes all input tracking to default values:
    /// - Sequence starts at 1 (0 is reserved for invalid/initial state)
    /// - All movement keys start as unpressed
    /// - Input timer starts at current time
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
        }
    }

    /// Updates input state based on current keyboard input and returns control events.
    ///
    /// This method performs several critical functions:
    ///
    /// ## Input Sampling
    /// - Samples current keyboard state for movement (WASD + arrow keys)
    /// - Samples debug control keys (1, 2, 3, R)
    /// - Performs edge detection for toggle keys (press events only)
    ///
    /// ## Change Detection
    /// - Compares current movement state to previous frame
    /// - Determines if network transmission is necessary
    ///
    /// ## Transmission Logic
    /// - Sends input when movement state changes (responsive gameplay)
    /// - Sends periodic keep-alive inputs (~60Hz) even when idle
    /// - Assigns sequence numbers and timestamps to sent inputs
    ///
    /// # Returns
    /// A tuple containing:
    /// - `(bool, bool, bool, bool)`: Toggle flags for (prediction, reconciliation, interpolation, reconnect)
    /// - `Option<InputState>`: Input to send to server (None if no transmission needed)
    pub fn update(&mut self) -> ((bool, bool, bool, bool), Option<InputState>) {
        // Sample current movement keys (support both WASD and arrow keys for accessibility)
        let left = is_key_down(KeyCode::A) || is_key_down(KeyCode::Left);
        let right = is_key_down(KeyCode::D) || is_key_down(KeyCode::Right);
        let jump = is_key_down(KeyCode::Space);

        // Sample debug/control keys for netcode feature toggling
        let key_1 = is_key_down(KeyCode::Key1);
        let key_2 = is_key_down(KeyCode::Key2);
        let key_3 = is_key_down(KeyCode::Key3);
        let key_r = is_key_down(KeyCode::R);

        let mut toggles = (false, false, false, false);

        // Detect key press events using edge detection (current && !previous)
        // This prevents toggle spam when keys are held down
        if key_1 && !self.prev_key_1 {
            toggles.0 = true; // Toggle client-side prediction
        }
        if key_2 && !self.prev_key_2 {
            toggles.1 = true; // Toggle server reconciliation
        }
        if key_3 && !self.prev_key_3 {
            toggles.2 = true; // Toggle interpolation smoothing
        }
        if key_r && !self.prev_key_r {
            toggles.3 = true; // Request client reconnection
        }

        // Update previous key states for next frame's edge detection
        self.prev_key_1 = key_1;
        self.prev_key_2 = key_2;
        self.prev_key_3 = key_3;
        self.prev_key_r = key_r;

        // Check if movement input state has changed since last frame
        let input_changed = left != self.current_input.left
            || right != self.current_input.right
            || jump != self.current_input.jump;

        // Send input if changed or periodically to maintain connection (16ms ≈ 60Hz)
        // Keep-alive mechanism prevents server from timing out idle clients
        let time_to_send = self.last_input_sent.elapsed() >= Duration::from_millis(16);

        let should_send = input_changed || time_to_send;
        let mut input_to_send = None;

        if should_send {
            // Create new input packet with current timestamp and incremented sequence
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

    /// Gets current timestamp in milliseconds since Unix epoch.
    ///
    /// This timestamp is used for:
    /// - Input timing and chronological ordering
    /// - Server-side latency calculations  
    /// - Network diagnostic measurements
    ///
    /// # Returns
    /// Current time as milliseconds since Unix epoch (January 1, 1970)
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

    /// Test basic input manager creation and initialization.
    #[test]
    fn test_input_manager_creation() {
        let input_manager = InputManager::new();
        assert_eq!(input_manager.next_sequence, 1);
        assert_eq!(input_manager.current_input.sequence, 0);
        assert!(!input_manager.current_input.left);
        assert!(!input_manager.current_input.right);
        assert!(!input_manager.current_input.jump);
    }

    /// Test timestamp generation functionality.
    #[test]
    fn test_get_timestamp() {
        let timestamp1 = InputManager::get_timestamp();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let timestamp2 = InputManager::get_timestamp();

        assert!(timestamp2 > timestamp1);
    }

    /// Test that sequence numbers increment correctly.
    #[test]
    fn test_sequence_increment() {
        let mut input_manager = InputManager::new();
        assert_eq!(input_manager.next_sequence, 1);

        input_manager.current_input = InputState {
            sequence: input_manager.next_sequence,
            timestamp: InputManager::get_timestamp(),
            left: true,
            right: false,
            jump: false,
        };

        input_manager.next_sequence += 1;
        assert_eq!(input_manager.next_sequence, 2);
    }
}
