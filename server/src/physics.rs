use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

impl Vector2 {
    pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn normalize(&self) -> Vector2 {
        let mag = self.magnitude();
        if mag == 0.0 {
            Vector2 { x: 0.0, y: 0.0 }
        } else {
            Vector2 {
                x: self.x / mag,
                y: self.y / mag,
            }
        }
    }

    pub fn scale(&self, scalar: f32) -> Vector2 {
        Vector2 {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }

    pub fn add(&self, other: &Vector2) -> Vector2 {
        Vector2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Object {
    pub is_static: bool,
    pub width: f32,
    pub height: f32,
    pub position: Vector2,
    pub velocity: Vector2,
    pub acceleration: Vector2,
    pub max_speed: f32,
    pub gravity: f32,
}

impl Object {
    // Simulates physics for a single object over a time step (dt, in seconds)
    pub fn simulate(&mut self, dt: f32) {
        //Don't simulate physics if object is static
        if self.is_static {
            return;
        }

        // Apply gravity to vertical acceleration
        let mut total_acceleration = self.acceleration;
        total_acceleration.y -= self.gravity;

        // Update velocity with acceleration
        self.velocity = self.velocity.add(&total_acceleration.scale(dt));

        // Clamp velocity to max_speed
        let speed = self.velocity.magnitude();
        if speed > self.max_speed {
            self.velocity = self.velocity.normalize().scale(self.max_speed);
        }

        // Update position with velocity
        self.position = self.position.add(&self.velocity.scale(dt));
    }
}

/// Returns true if two objects (using their bounding box) overlap
fn check_collision(a: &Object, b: &Object) -> bool {
    let a_left = a.position.x - a.width / 2.0;
    let a_right = a.position.x + a.width / 2.0;
    let a_bottom = a.position.y - a.height / 2.0;
    let a_top = a.position.y + a.height / 2.0;

    let b_left = b.position.x - b.width / 2.0;
    let b_right = b.position.x + b.width / 2.0;
    let b_bottom = b.position.y - b.height / 2.0;
    let b_top = b.position.y + b.height / 2.0;

    !(a_left > b_right || a_right < b_left || a_top < b_bottom || a_bottom > b_top)
}

pub fn resolve_collision(a: &mut Object, b: &Object) {
    if check_collision(a, b) {
        // Calculate overlap on both axes
        let a_left = a.position.x - a.width / 2.0;
        let a_right = a.position.x + a.width / 2.0;
        let a_bottom = a.position.y - a.height / 2.0;
        let a_top = a.position.y + a.height / 2.0;

        let b_left = b.position.x - b.width / 2.0;
        let b_right = b.position.x + b.width / 2.0;
        let b_bottom = b.position.y - b.height / 2.0;
        let b_top = b.position.y + b.height / 2.0;

        let overlap_x = (a_right.min(b_right)) - (a_left.max(b_left));
        let overlap_y = (a_top.min(b_top)) - (a_bottom.max(b_bottom));

        // Move player out of collision along the smallest overlap
        if overlap_x < overlap_y {
            // Resolve X axis
            if a.position.x < b.position.x {
                a.position.x -= overlap_x;
            } else {
                a.position.x += overlap_x;
            }
            a.velocity.x = 0.0;
        } else {
            // Resolve Y axis
            if a.position.y < b.position.y && a.velocity.y <= 0.0 {
                // Landed on top: snap bottom of player to top of platform
                a.position.y = b.position.y + b.height / 2.0 + a.height / 2.0;
                a.velocity.y = 0.0;
            } else if a.position.y > b.position.y {
                // Hit from below: snap top of player to bottom of platform
                a.position.y = b.position.y - b.height / 2.0 - a.height / 2.0;
                a.velocity.y = 0.0;
            }
        }
    }
}

/// Checks if the player's object is standing on any static object (ground).
/// Returns true if grounded, false otherwise.
pub fn check_grounded(player: &Object, objects: &[Object]) -> bool {
    let epsilon = 0.01;
    let player_bottom = player.position.y - player.height / 2.0;
    for object in objects {
        if object.is_static {
            let object_top = object.position.y + object.height / 2.0;
            let horizontally_overlapping = player.position.x + player.width / 2.0
                > object.position.x - object.width / 2.0
                && player.position.x - player.width / 2.0 < object.position.x + object.width / 2.0;
            if horizontally_overlapping
                && (player_bottom - object_top).abs() < epsilon
                && player.velocity.y <= 0.0
            {
                return true;
            }
        }
    }
    false
}
