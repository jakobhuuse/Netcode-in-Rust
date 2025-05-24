use log::info;
use minifb::{Key, Window};
use shared::InputState;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct InputManager {
    next_sequence: u32,
    current_input: InputState,
    last_input_sent: Instant,

    prev_key_1: bool,
    prev_key_2: bool,
    prev_key_3: bool,
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
        }
    }

    pub fn update(&mut self, window: &Window) -> ((bool, bool, bool), Option<InputState>) {
        let keys = window.get_keys();

        let left = keys.contains(&Key::A) || keys.contains(&Key::Left);
        let right = keys.contains(&Key::D) || keys.contains(&Key::Right);
        let jump = keys.contains(&Key::Space);

        let key_1 = keys.contains(&Key::Key1);
        let key_2 = keys.contains(&Key::Key2);
        let key_3 = keys.contains(&Key::Key3);

        let mut toggles = (false, false, false);

        if key_1 && !self.prev_key_1 {
            toggles.0 = true;
        }
        if key_2 && !self.prev_key_2 {
            toggles.1 = true;
        }
        if key_3 && !self.prev_key_3 {
            toggles.2 = true;
        }

        self.prev_key_1 = key_1;
        self.prev_key_2 = key_2;
        self.prev_key_3 = key_3;

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
