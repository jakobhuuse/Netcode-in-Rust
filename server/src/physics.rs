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

#[derive(Debug, Clone, Copy)]
pub struct Object {
    pub position: Vector2,
    pub velocity: Vector2,
    pub acceleration: Vector2,
    pub max_speed: f32,
    pub gravity: f32,
}

impl Object {
    /// Simulates physics for a single object over a time step (dt, in seconds)
    pub fn simulate(&mut self, dt: f32) {
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

        // Ground check: prevent falling below y = 0.0
        if self.position.y < 0.0 {
            self.position.y = 0.0;
            if self.velocity.y < 0.0 {
                self.velocity.y = 0.0;
            }
        }
    }
}
