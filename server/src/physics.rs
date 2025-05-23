use serde::Serialize;

///Represents a vector in 2D space.
#[derive(Debug, Clone, Copy, Serialize, Default)]
pub struct Vector2 {
    ///Value along the x-axis.
    /// Positive direction is to the right.
    pub x: f32,
    ///Value along the y-axis.
    /// Positive direction is up.
    pub y: f32,
}

impl Vector2 {
    ///Returns the magnitude of the vector.
    pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    ///Returns the normalized vector.
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

    ///Returns the scaled vector.
    pub fn scale(&self, scalar: f32) -> Vector2 {
        Vector2 {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }

    ///Returns the sum of two vectors.
    pub fn add(&self, other: &Vector2) -> Vector2 {
        Vector2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

///Represents a static object in 2D space.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Object {
    pub width: f32,
    pub height: f32,
    ///The positional center of the object.
    pub position: Vector2,
}

impl Default for Object {
    fn default() -> Self {
        Object {
            width: 1.0,
            height: 1.0,
            position: Vector2::default(),
        }
    }
}

///Represents an dynamic object in 2D space.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct DynamicObject {
    pub object: Object,
    pub velocity: Vector2,
    pub acceleration: Vector2,
    pub max_speed: f32,
    pub gravity: f32,
    pub grounded: bool,
}

impl Default for DynamicObject {
    fn default() -> Self {
        DynamicObject {
            object: Object::default(),
            velocity: Vector2::default(),
            acceleration: Vector2::default(),
            max_speed: 2.0,
            gravity: 9.81,
            grounded: bool::default(),
        }
    }
}

impl DynamicObject {
    ///Simulates physics on the dynamic object.
    pub fn simulate(&mut self, dt: f32) {
        // Apply gravity to vertical acceleration
        let mut total_acceleration = self.acceleration;
        total_acceleration.y -= self.gravity;

        // Update velocity calculated byw acceleration
        self.velocity = self.velocity.add(&total_acceleration.scale(dt));

        // Clamp velocity to max_speed
        let speed = self.velocity.magnitude();
        if speed > self.max_speed {
            self.velocity = self.velocity.normalize().scale(self.max_speed);
        }

        // Update position based on velocity
        self.object.position = self.object.position.add(&self.velocity.scale(dt));
    }

    ///Checks for and resolves collisions between the dynamic object and a collection of objects.
    ///Returns true if any collision was detected and resolved.
    pub fn resolve_collisions(&mut self, others: &[Object]) -> bool {
        let mut collided = false;
        for other in others {
            // Calculate the bounds of both objects (position is center)
            let self_left = self.object.position.x - self.object.width / 2.0;
            let self_right = self.object.position.x + self.object.width / 2.0;
            let self_top = self.object.position.y + self.object.height / 2.0;
            let self_bottom = self.object.position.y - self.object.height / 2.0;

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
                    if self.object.position.x < other.position.x {
                        // Self is to the left of other, move self left
                        self.object.position.x = other_left - self.object.width / 2.0;
                    } else {
                        // Self is to the right of other, move self right
                        self.object.position.x = other_right + self.object.width / 2.0;
                    }
                    // Stop horizontal movement
                    self.velocity.x = 0.0;
                    self.acceleration.x = 0.0;
                } else {
                    // Vertical collision
                    if self.object.position.y < other.position.y {
                        self.object.position.y = other_bottom + self.object.height / 2.0;
                    } else {
                        self.object.position.y = other_top + self.object.height / 2.0;
                    }
                    self.velocity.y = 0.0;
                    if self.velocity.y < 0.0 {
                        self.acceleration.y = 0.0;
                    }
                }
                collided = true;
            }
        }
        collided
    }

    /// Checks if the dynamic object is grounded against any object in the collection.
    /// Updates the grounded property and returns true if it is.
    pub fn check_grounded(&mut self, others: &[Object]) {
        let tolerance = 0.1;

        let self_left = self.object.position.x - self.object.width / 2.0;
        let self_right = self.object.position.x + self.object.width / 2.0;
        let self_bottom = self.object.position.y - self.object.height / 2.0;

        for other in others {
            let other_left = other.position.x - other.width / 2.0;
            let other_right = other.position.x + other.width / 2.0;
            let other_top = other.position.y + other.height / 2.0;

            let horizontal_overlap = self_right > other_left && self_left < other_right;
            let vertical_touching = (self_bottom - other_top).abs() < tolerance;

            if horizontal_overlap && vertical_touching && self.object.position.y > other.position.y
            {
                return self.grounded = true;
            }
        }
        self.grounded = false
    }
}
