use serde::{Deserialize, Serialize};

// Entity representation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Entity {
    pub id: u32,
    pub entity_type: EntityType,
    pub position: (f32, f32),
    pub velocity: (f32, f32),
    pub radius: f32,
    pub color: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum EntityType {
    Player,
}

impl Entity {
    pub fn new(id: u32, entity_type: EntityType, position: (f32, f32), radius: f32, color: String) -> Self {
        Entity {
            id,
            entity_type,
            position,
            velocity: (0.0, 0.0),
            radius,
            color,
        }
    }

    // Update entity position based on velocity and delta time
    pub fn update_position(&mut self, dt: f32, world_width: f32, world_height: f32) {
        self.position.0 += self.velocity.0 * dt;
        self.position.1 += self.velocity.1 * dt;
        
        // Apply boundary constraints
        self.position.0 = self.position.0.max(self.radius).min(world_width - self.radius);
        self.position.1 = self.position.1.max(self.radius).min(world_height - self.radius);
    }
    
    // Check collision with another entity
    pub fn check_collision(&self, other: &Entity) -> bool {
        let dx = other.position.0 - self.position.0;
        let dy = other.position.1 - self.position.1;
        let distance_squared = dx * dx + dy * dy;
        
        distance_squared < (self.radius + other.radius).powi(2)
    }
    
    // Resolve collision with another entity
    pub fn resolve_collision(&mut self, other: &mut Entity) {
        let dx = other.position.0 - self.position.0;
        let dy = other.position.1 - self.position.1;
        let distance = (dx * dx + dy * dy).sqrt();
        
        if distance < self.radius + other.radius {
            // Calculate unit vector between entities
            let nx = dx / distance;
            let ny = dy / distance;
            
            // Calculate overlap
            let overlap = self.radius + other.radius - distance;
            
            // Resolve overlap
            self.position.0 -= nx * overlap * 0.5;
            self.position.1 -= ny * overlap * 0.5;
            
            other.position.0 += nx * overlap * 0.5;
            other.position.1 += ny * overlap * 0.5;
            
            // Reflect velocity (simple bounce)
            let dot1 = self.velocity.0 * nx + self.velocity.1 * ny;
            self.velocity.0 -= 2.0 * dot1 * nx;
            self.velocity.1 -= 2.0 * dot1 * ny;
            
            let dot2 = other.velocity.0 * nx + other.velocity.1 * ny;
            other.velocity.0 -= 2.0 * dot2 * nx;
            other.velocity.1 -= 2.0 * dot2 * ny;
        }
    }
}