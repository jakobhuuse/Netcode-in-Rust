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

pub fn resolve_collision(&mut self, other: &Object) -> bool {
    // Calculate the bounds of both objects (position is center, Y-axis positive UP)
    let self_left = self.position.x - self.width / 2.0;
    let self_right = self.position.x + self.width / 2.0;
    let self_top = self.position.y + self.height / 2.0;
    let self_bottom = self.position.y - self.height / 2.0;

    let other_left = other.position.x - other.width / 2.0;
    let other_right = other.position.x + other.width / 2.0;
    let other_top = other.position.y + other.height / 2.0;
    let other_bottom = other.position.y - other.height / 2.0;
    
    // Check for collision using AABB (Axis-Aligned Bounding Box) intersection
    let collision = !(self_right <= other_left
        || self_left >= other_right
        || self_bottom >= other_top
        || self_top <= other_bottom);

    if collision {
        // Calculate overlap amounts (use corrected bounds here)
        let overlap_x = (self_right.min(other_right) - self_left.max(other_left)).abs();
        let overlap_y = (self_top.min(other_top) - self_bottom.max(other_bottom)).abs();

        // Determine which axis had the shallowest penetration and resolve along that axis
        if overlap_x < overlap_y {
            // Horizontal collision
            if self.position.x < other.position.x {
                // Self is to the left of other, move self left
                self.position.x = other_left - self.width / 2.0;
            } else {
                // Self is to the right of other, move self right
                self.position.x = other_right + self.width / 2.0;
            }
            // Stop horizontal movement
            self.velocity.x = 0.0;
            self.acceleration.x = 0.0;
        } else {
            // Vertical collision
            if self.position.y < other.position.y {
                self.position.y = other_bottom + self.height / 2.0;
            } else {
                self.position.y = other_top + self.height / 2.0;
            }
            self.velocity.y = 0.0;
            if self.velocity.y < 0.0 {
                 self.acceleration.y = 0.0;
            }
        }
        true
    } else {
        false
    }
}

    pub fn is_grounded(&self, other: &Object) -> bool {
    let self_left = self.position.x - self.width / 2.0;
    let self_right = self.position.x + self.width / 2.0;
    let self_bottom = self.position.y - self.height / 2.0;

    let other_left = other.position.x - other.width / 2.0;
    let other_right = other.position.x + other.width / 2.0;
    let other_top = other.position.y + other.height / 2.0;

    let horizontal_overlap = self_right > other_left && self_left < other_right;

    let vertical_touching = (self_bottom - other_top).abs() < 0.1; // Small tolerance

    horizontal_overlap && vertical_touching && self.position.y > other.position.y
}
}
