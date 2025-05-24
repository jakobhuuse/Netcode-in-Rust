use macroquad::prelude::*;
use shared::InputState;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct InputManager {
    next_sequence: u32,
    current_input: InputState,
    last_input_sent: Instant,

    prev_key_1: bool,
    prev_key_2: bool,
    prev_key_3: bool,
    prev_key_r: bool,
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
        }
    }

    pub fn update(&mut self) -> ((bool, bool, bool, bool), Option<InputState>) {
        let left = is_key_down(KeyCode::A) || is_key_down(KeyCode::Left);
        let right = is_key_down(KeyCode::D) || is_key_down(KeyCode::Right);
        let jump = is_key_down(KeyCode::Space);

        let key_1 = is_key_down(KeyCode::Key1);
        let key_2 = is_key_down(KeyCode::Key2);
        let key_3 = is_key_down(KeyCode::Key3);
        let key_r = is_key_down(KeyCode::R);

        let mut toggles = (false, false, false, false);

        if key_1 && !self.prev_key_1 {
            toggles.0 = true;
        }
        if key_2 && !self.prev_key_2 {
            toggles.1 = true;
        }
        if key_3 && !self.prev_key_3 {
            toggles.2 = true;
        }
        if key_r && !self.prev_key_r {
            toggles.3 = true;
        }

        self.prev_key_1 = key_1;
        self.prev_key_2 = key_2;
        self.prev_key_3 = key_3;
        self.prev_key_r = key_r;

        let input_changed = left != self.current_input.left
            || right != self.current_input.right
            || jump != self.current_input.jump;

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

    #[test]
    fn test_input_manager_creation() {
        let input_manager = InputManager::new();
        assert_eq!(input_manager.next_sequence, 1);
        assert_eq!(input_manager.current_input.sequence, 0);
        assert!(!input_manager.current_input.left);
        assert!(!input_manager.current_input.right);
        assert!(!input_manager.current_input.jump);
    }

    #[test]
    fn test_get_timestamp() {
        let timestamp1 = InputManager::get_timestamp();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let timestamp2 = InputManager::get_timestamp();

        assert!(timestamp2 > timestamp1);
    }

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
