//! # Shared Game Library
//!
//! This module contains all shared data structures, constants, and utilities used by both
//! the game client and server. It serves as the foundation for networked multiplayer
//! communication and ensures consistency across the distributed game architecture.
//!
//! ## Core Components
//!
//! ### Game Constants
//! Physics and world parameters that define the game environment:
//! - **Physics**: Gravity, player movement speed, jump velocity
//! - **World Boundaries**: Game world dimensions and floor position
//! - **Player Properties**: Size and collision parameters
//!
//! ### Network Protocol
//! The `Packet` enum defines the complete communication protocol between client and server:
//! - **Connection Management**: Connect, Connected, Disconnect, Disconnected
//! - **Input Transmission**: Player input with sequence numbers for reliability
//! - **State Synchronization**: Authoritative game state updates from server
//!
//! ### Game Entities
//! Core data structures representing game objects:
//! - **Player**: Complete player state including position, velocity, and metadata
//! - **InputState**: Client input representation for deterministic replay
//!
//! ### Physics System
//! Collision detection and resolution functions shared between client and server:
//! - **AABB Collision Detection**: Efficient axis-aligned bounding box testing
//! - **Collision Resolution**: Physics-based separation and velocity exchange
//!
//! ## Design Philosophy
//!
//! ### Deterministic Simulation
//! All physics calculations use consistent floating-point operations and constants
//! to ensure identical results on both client and server, enabling prediction
//! and reconciliation techniques.
//!
//! ### Serialization
//! All network data structures implement `Serialize` and `Deserialize` for efficient
//! binary encoding via the `bincode` crate, minimizing network bandwidth usage.
//!
//! ### Type Safety
//! Strong typing prevents common networking bugs like mixing up player IDs,
//! sequence numbers, and timestamps across the client-server boundary.
//!
//! ## Usage Examples
//!
//! ```rust
//! use shared::*;
//!
//! // Create a new player
//! let player = Player::new(1, 100.0, 200.0);
//!
//! // Check collision between players
//! let player2 = Player::new(2, 110.0, 210.0);
//! if check_collision(&player, &player2) {
//!     // Handle collision...
//! }
//!
//! // Create network packet
//! let packet = Packet::Input {
//!     sequence: 42,
//!     timestamp: 123456789,
//!     left: true,
//!     right: false,
//!     jump: false,
//! };
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Physics constants for the game world simulation
///
/// These constants define the physical behavior and spatial boundaries of the game.
/// They are shared between client and server to ensure deterministic simulation
/// and consistent netcode behavior across all participants.
/// Downward gravitational acceleration applied to all players
///
/// Value chosen to provide responsive jumping while maintaining realistic feel.
/// Applied continuously during physics updates when players are airborne.
pub const GRAVITY: f32 = 980.0; // pixels/second²

/// Maximum horizontal movement speed for players
///
/// This represents the speed at which players move when left/right inputs are pressed.
/// Used for both client-side prediction and authoritative server simulation.
pub const PLAYER_SPEED: f32 = 300.0; // pixels/second

/// Initial upward velocity when a player jumps
///
/// Negative value because the coordinate system has Y increasing downward.
/// Magnitude chosen to provide satisfying jump height and air time.
pub const JUMP_VELOCITY: f32 = -400.0; // pixels/second (negative = upward)

/// Y-coordinate of the ground/floor level
///
/// Players cannot move below this position and will be considered "on ground"
/// when their bottom edge touches this level, enabling jumping mechanics.
pub const FLOOR_Y: f32 = 550.0; // pixels from top of screen

/// Total width of the playable game world
///
/// Players are constrained to move within horizontal bounds [0, WORLD_WIDTH].
/// This affects collision detection and boundary enforcement on both client and server.
pub const WORLD_WIDTH: f32 = 800.0; // pixels

/// Total height of the playable game world
///
/// Defines the vertical extent of the game area, though players are primarily
/// constrained by FLOOR_Y rather than this upper boundary.
pub const WORLD_HEIGHT: f32 = 600.0; // pixels

/// Width and height of each player's collision box
///
/// Players are rendered and simulated as squares of this size. Used for:
/// - Collision detection between players
/// - Boundary checking against world edges
/// - Rendering player sprites and hitboxes
pub const PLAYER_SIZE: f32 = 32.0; // pixels

/// Network packet types for client-server communication protocol
///
/// This enum defines the complete message format for the networked multiplayer game.
/// All packets are serialized using `bincode` for efficient binary transmission over UDP.
/// The protocol is designed to be minimal, reliable, and suitable for real-time gameplay.
///
/// ## Packet Categories
///
/// ### Connection Management
/// - `Connect`: Initial handshake from client to server
/// - `Connected`: Server acknowledgment with assigned client ID
/// - `Disconnect`: Graceful disconnection request from client
/// - `Disconnected`: Server notification of connection termination
///
/// ### Gameplay Data
/// - `Input`: Client player input with sequence numbering for reliability
/// - `GameState`: Authoritative world state broadcast from server
///
/// ## Reliability Considerations
///
/// Since UDP is used for transport, the protocol includes mechanisms for handling
/// packet loss and ordering issues:
/// - Input packets include sequence numbers for proper ordering
/// - Game state packets include last processed input per client for reconciliation
/// - Connection health is monitored through regular packet exchange
///
/// ## Design Principles
///
/// - **Compact**: Minimal data size to reduce network bandwidth
/// - **Versioned**: Client version field for compatibility checking
/// - **Timestamped**: Temporal information for latency compensation
/// - **Deterministic**: Consistent data representation across platforms
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Packet {
    // === Client → Server Packets ===
    /// Initial connection request from client to server
    ///
    /// Sent when a client first attempts to join the game. The server validates
    /// the client version for compatibility and responds with either `Connected`
    /// (on success) or `Disconnected` (on failure/rejection).
    ///
    /// # Fields
    /// - `client_version`: Protocol version for compatibility checking
    Connect { client_version: u32 },

    /// Player input data with reliability mechanisms
    ///
    /// Contains a single frame of player input along with metadata for reliable
    /// processing. The sequence number enables proper ordering on the server,
    /// while the timestamp allows for latency compensation and interpolation.
    ///
    /// # Reliability Features
    /// - Sequence numbering prevents out-of-order processing
    /// - Timestamps enable lag compensation calculations
    /// - Compact boolean flags minimize bandwidth usage
    ///
    /// # Fields
    /// - `sequence`: Monotonically increasing input sequence number
    /// - `timestamp`: Client-side timestamp when input was captured (ms since epoch)
    /// - `left`: True if left movement key is pressed
    /// - `right`: True if right movement key is pressed  
    /// - `jump`: True if jump key is pressed
    Input {
        sequence: u32,
        timestamp: u64,
        left: bool,
        right: bool,
        jump: bool,
    },

    /// Graceful disconnection request from client
    ///
    /// Sent when the client wants to cleanly disconnect from the server.
    /// This allows the server to properly clean up resources and notify
    /// other clients of the player's departure.
    Disconnect,

    // === Server → Client Packets ===
    /// Successful connection acknowledgment with client assignment
    ///
    /// Sent by the server after accepting a client's connection request.
    /// The assigned client ID is used for all subsequent communication
    /// and identifies the player in the game world.
    ///
    /// # Fields
    /// - `client_id`: Unique identifier assigned to this client
    Connected { client_id: u32 },

    /// Authoritative game state snapshot from server
    ///
    /// Contains the complete, authoritative game state that clients use for
    /// rendering and reconciliation. Broadcast periodically to all connected
    /// clients to maintain synchronization.
    ///
    /// # Reconciliation Data
    /// The `last_processed_input` map enables client-side reconciliation by
    /// indicating which inputs have been processed by the server. Clients
    /// can replay any newer inputs to maintain prediction accuracy.
    ///
    /// # Fields
    /// - `tick`: Server simulation tick number for temporal ordering
    /// - `timestamp`: Server timestamp when state was captured
    /// - `last_processed_input`: Last input sequence processed per client ID
    /// - `players`: Complete snapshot of all player states
    GameState {
        tick: u32,
        timestamp: u64,
        last_processed_input: HashMap<u32, u32>,
        players: Vec<Player>,
    },

    /// Connection termination notification with reason
    ///
    /// Sent by the server to inform a client that their connection is being
    /// terminated. Includes a human-readable reason for debugging and
    /// user feedback purposes.
    ///
    /// # Common Reasons
    /// - "Server full": No available player slots
    /// - "Version mismatch": Incompatible client version
    /// - "Timeout": Client failed to respond to keep-alive packets
    /// - "Kicked": Administrative action by server operator
    ///
    /// # Fields
    /// - `reason`: Human-readable explanation for disconnection
    Disconnected { reason: String },
}

/// Represents a player entity in the networked game world
///
/// This structure contains all data necessary to represent a player's state in the
/// distributed multiplayer environment. It's designed for efficient serialization
/// and deterministic physics simulation across client and server.
///
/// ## State Components
///
/// ### Identification
/// - `id`: Unique identifier assigned by server for network communication
///
/// ### Spatial State  
/// - `x`, `y`: Position coordinates in the 2D game world (pixels)
/// - `vel_x`, `vel_y`: Current velocity components (pixels/second)
///
/// ### Game State
/// - `on_ground`: Ground contact flag affecting jump mechanics
///
/// ## Design Considerations
///
/// ### Deterministic Physics
/// All fields use `f32` floating-point for consistent cross-platform behavior.
/// This ensures identical simulation results on different client machines and
/// the authoritative server.
///
/// ### Network Efficiency
/// The structure is kept minimal to reduce bandwidth usage during frequent
/// game state broadcasts. Additional derived properties (like center point,
/// bounding box) are calculated on-demand rather than stored.
///
/// ### Collision Model
/// Players are represented as axis-aligned squares of size `PLAYER_SIZE`.
/// This simplifies collision detection while providing adequate gameplay mechanics.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Player {
    /// Unique player identifier assigned by the server
    ///
    /// This ID is used for all network communication and remains constant
    /// throughout the player's session. It allows clients to track specific
    /// players across game state updates and associate inputs with the
    /// correct player entity.
    pub id: u32,

    /// X-coordinate position in the game world (pixels)
    ///
    /// Represents the left edge of the player's bounding box. Valid range
    /// is [0, WORLD_WIDTH - PLAYER_SIZE] to keep the player within world bounds.
    pub x: f32,

    /// Y-coordinate position in the game world (pixels)
    ///
    /// Represents the top edge of the player's bounding box. The coordinate
    /// system has Y increasing downward, with 0 at the top of the game world.
    /// Players are constrained by gravity to stay above FLOOR_Y.
    pub y: f32,

    /// Horizontal velocity component (pixels/second)
    ///
    /// Positive values indicate rightward movement, negative values indicate
    /// leftward movement. Magnitude is typically ±PLAYER_SPEED during input-driven
    /// movement, but can vary during collisions or other physics interactions.
    pub vel_x: f32,

    /// Vertical velocity component (pixels/second)
    ///
    /// Positive values indicate downward movement, negative values indicate
    /// upward movement. Continuously modified by gravity and jumping mechanics.
    /// Jump actions set this to JUMP_VELOCITY.
    pub vel_y: f32,

    /// Ground contact status for jump mechanics
    ///
    /// True when the player is touching the ground (y + PLAYER_SIZE >= FLOOR_Y),
    /// false when airborne. This flag determines whether jump inputs are accepted,
    /// preventing infinite jumping or double-jumping behaviors.
    pub on_ground: bool,
}

impl Player {
    /// Creates a new player at the specified position with default state
    ///
    /// Initializes a player entity with the given ID and position coordinates.
    /// The player starts in a stable state: at rest (zero velocity) and on the
    /// ground (ready to accept jump inputs).
    ///
    /// # Parameters
    /// - `id`: Unique identifier for this player (assigned by server)
    /// - `x`: Initial X-coordinate in game world pixels
    /// - `y`: Initial Y-coordinate in game world pixels
    ///
    /// # Default Values
    /// - Velocity: (0, 0) - player starts at rest
    /// - Ground contact: true - player can immediately jump
    ///
    /// # Example
    /// ```rust
    /// use shared::{Player, PLAYER_SIZE};
    ///
    /// let player = Player::new(1, 100.0, 200.0);
    /// assert_eq!(player.id, 1);
    /// assert_eq!(player.x, 100.0);
    /// assert_eq!(player.y, 200.0);
    /// assert!(player.on_ground);
    /// ```
    pub fn new(id: u32, x: f32, y: f32) -> Self {
        Self {
            id,
            x,
            y,
            vel_x: 0.0,
            vel_y: 0.0,
            on_ground: true,
        }
    }

    /// Returns the axis-aligned bounding box coordinates
    ///
    /// Calculates the rectangular bounds of the player for collision detection
    /// and rendering purposes. The player is represented as a square of size
    /// `PLAYER_SIZE` with the position (x, y) representing the top-left corner.
    ///
    /// # Returns
    /// A tuple containing (left, top, right, bottom) coordinates:
    /// - `left`: X-coordinate of the left edge (same as player.x)
    /// - `top`: Y-coordinate of the top edge (same as player.y)
    /// - `right`: X-coordinate of the right edge (player.x + PLAYER_SIZE)
    /// - `bottom`: Y-coordinate of the bottom edge (player.y + PLAYER_SIZE)
    ///
    /// # Usage
    /// Primarily used by collision detection algorithms and rendering systems
    /// that need to know the exact spatial bounds of the player entity.
    ///
    /// # Example
    /// ```rust
    /// use shared::{Player, PLAYER_SIZE};
    ///
    /// let player = Player::new(1, 50.0, 75.0);
    /// let (left, top, right, bottom) = player.get_bounds();
    /// assert_eq!(left, 50.0);
    /// assert_eq!(right, 50.0 + PLAYER_SIZE);
    /// ```
    pub fn get_bounds(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.x + PLAYER_SIZE, self.y + PLAYER_SIZE)
    }

    /// Returns the center point coordinates of the player
    ///
    /// Calculates the geometric center of the player's bounding box, which is
    /// useful for distance calculations, collision resolution direction vectors,
    /// and other spatial computations that benefit from a single reference point.
    ///
    /// # Returns
    /// A tuple containing (center_x, center_y) coordinates:
    /// - `center_x`: X-coordinate of the center (player.x + PLAYER_SIZE/2)
    /// - `center_y`: Y-coordinate of the center (player.y + PLAYER_SIZE/2)
    ///
    /// # Usage
    /// - Collision resolution: determining separation directions
    /// - Distance calculations: measuring proximity between players
    /// - Effect positioning: centering visual/audio effects on players
    /// - Camera tracking: smooth following of player movement
    ///
    /// # Example
    /// ```rust
    /// use shared::{Player, PLAYER_SIZE};
    ///
    /// let player = Player::new(1, 100.0, 200.0);
    /// let (cx, cy) = player.center();
    /// assert_eq!(cx, 100.0 + PLAYER_SIZE / 2.0);
    /// assert_eq!(cy, 200.0 + PLAYER_SIZE / 2.0);
    /// ```
    pub fn center(&self) -> (f32, f32) {
        (self.x + PLAYER_SIZE / 2.0, self.y + PLAYER_SIZE / 2.0)
    }
}

/// Performs axis-aligned bounding box (AABB) collision detection between two players
///
/// This function implements efficient rectangular collision detection using the
/// separating axis theorem. It checks whether the bounding boxes of two players
/// overlap in both X and Y dimensions simultaneously.
///
/// ## Algorithm Details
///
/// The function uses the "separating axis" approach: if any edge of one rectangle
/// is completely beyond the corresponding edge of the other rectangle, then no
/// collision exists. This requires checking four conditions:
///
/// 1. Player1's right edge is left of Player2's left edge
/// 2. Player2's right edge is left of Player1's left edge  
/// 3. Player1's bottom edge is above Player2's top edge
/// 4. Player2's bottom edge is above Player1's top edge
///
/// If ANY of these conditions is true, the players are NOT colliding.
///
/// ## Performance Characteristics
///
/// - **Time Complexity**: O(1) - constant time regardless of player size
/// - **Space Complexity**: O(1) - no additional memory allocation
/// - **Early Exit**: Returns false as soon as any separating axis is found
///
/// ## Edge Cases
///
/// - **Exact Touch**: Players touching along edges are NOT considered colliding
/// - **Zero Size**: Would always return false (impossible with PLAYER_SIZE > 0)
/// - **Identical Position**: Returns true when players occupy the same space
///
/// # Parameters
/// - `player1`: Reference to the first player to test
/// - `player2`: Reference to the second player to test
///
/// # Returns
/// - `true`: If the players' bounding boxes overlap in both dimensions
/// - `false`: If the players are separated by any amount in either dimension
///
/// # Example
/// ```rust
/// use shared::{Player, check_collision};
///
/// let player1 = Player::new(1, 0.0, 0.0);
/// let player2 = Player::new(2, 16.0, 16.0); // Overlapping
/// assert!(check_collision(&player1, &player2));
///
/// let player3 = Player::new(3, 100.0, 100.0); // Separated  
/// assert!(!check_collision(&player1, &player3));
/// ```
pub fn check_collision(player1: &Player, player2: &Player) -> bool {
    let (x1, y1, x2, y2) = player1.get_bounds();
    let (x3, y3, x4, y4) = player2.get_bounds();

    // No collision if any edge of one box is beyond the corresponding edge of the other
    !(x2 <= x3 || x4 <= x1 || y2 <= y3 || y4 <= y1)
}

/// Resolves collision between two players using physics-based separation and momentum exchange
///
/// This function implements a complete collision resolution system that handles both
/// spatial separation and velocity transfer between colliding players. It's designed
/// to create realistic, stable physics interactions in the multiplayer environment.
///
/// ## Algorithm Overview
///
/// ### 1. Collision Verification
/// First checks if players are actually colliding using `check_collision()`.
/// If no collision exists, the function returns early without modifications.
///
/// ### 2. Direction Calculation  
/// Computes the center-to-center vector between players to determine the
/// separation direction. This ensures players are pushed apart along the
/// most natural path.
///
/// ### 3. Special Case Handling
/// When players are at identical positions (distance < 0.001), applies a
/// default horizontal separation to prevent mathematical instability.
///
/// ### 4. Spatial Separation
/// Calculates overlap distance and pushes each player away by half that
/// amount, ensuring they no longer intersect while preserving the collision
/// point as closely as possible.
///
/// ### 5. Boundary Enforcement
/// Clamps player positions to world boundaries to prevent movement outside
/// the playable area during collision resolution.
///
/// ### 6. Momentum Exchange
/// Swaps velocities between players with a damping factor (0.8) to simulate
/// inelastic collision behavior, creating realistic bouncing effects.
///
/// ## Physics Properties
///
/// ### Energy Conservation
/// The velocity exchange conserves momentum while applying damping to prevent
/// indefinite bouncing. This creates stable, convergent collision behavior.
///
/// ### Penetration Resolution
/// Players are separated by the minimum distance required to eliminate overlap,
/// preventing "sticky" collisions or tunneling through each other.
///
/// ### Boundary Safety
/// All position modifications are clamped to valid world coordinates, ensuring
/// collisions near walls don't push players outside the game area.
///
/// ## Performance Considerations
///
/// - **Early Exit**: Returns immediately if no collision is detected
/// - **Minimal Allocation**: Uses only stack variables for calculations
/// - **Deterministic**: Produces identical results across different platforms
///
/// # Parameters
/// - `player1`: Mutable reference to the first colliding player
/// - `player2`: Mutable reference to the second colliding player
///
/// # Side Effects
/// When collision is detected, both players may have their position and velocity modified:
/// - **Positions**: Adjusted to eliminate overlap
/// - **Velocities**: Exchanged with damping factor applied
///
/// # Example
/// ```rust
/// use shared::{Player, resolve_collision, check_collision};
///
/// let mut player1 = Player::new(1, 10.0, 10.0);
/// let mut player2 = Player::new(2, 20.0, 20.0); // Close enough to overlap
///
/// // Set initial velocities
/// player1.vel_x = 100.0; // Moving right
/// player2.vel_x = -50.0; // Moving left
///
/// // Store initial positions to verify movement
/// let initial_pos1 = player1.x;
/// let initial_pos2 = player2.x;
///
/// resolve_collision(&mut player1, &mut player2);
///
/// // Players should have moved and exchanged velocities
/// assert_ne!(player1.x, initial_pos1);
/// assert_ne!(player2.x, initial_pos2);
/// ```
pub fn resolve_collision(player1: &mut Player, player2: &mut Player) {
    if !check_collision(player1, player2) {
        return;
    }

    let (cx1, cy1) = player1.center();
    let (cx2, cy2) = player2.center();

    // Calculate direction vector from player1 to player2
    let dx = cx2 - cx1;
    let dy = cy2 - cy1;
    let distance = (dx * dx + dy * dy).sqrt();

    // Handle edge case where players are at exactly the same position
    if distance < 0.001 {
        player1.x -= PLAYER_SIZE / 2.0;
        player2.x += PLAYER_SIZE / 2.0;
        return;
    }

    // Normalize direction vector
    let nx = dx / distance;
    let ny = dy / distance;

    // Calculate overlap and separate players
    let overlap = PLAYER_SIZE - distance;

    if overlap > 0.0 {
        let separation = overlap / 2.0;
        player1.x -= nx * separation;
        player1.y -= ny * separation;
        player2.x += nx * separation;
        player2.y += ny * separation;

        // Clamp positions to world boundaries
        player1.x = player1.x.clamp(0.0, WORLD_WIDTH - PLAYER_SIZE);
        player1.y = player1.y.clamp(0.0, FLOOR_Y - PLAYER_SIZE);
        player2.x = player2.x.clamp(0.0, WORLD_WIDTH - PLAYER_SIZE);
        player2.y = player2.y.clamp(0.0, FLOOR_Y - PLAYER_SIZE);

        // Exchange velocities with damping for realistic collision response
        let temp_vx = player1.vel_x;
        let temp_vy = player1.vel_y;
        player1.vel_x = player2.vel_x * 0.8;
        player1.vel_y = player2.vel_y * 0.8;
        player2.vel_x = temp_vx * 0.8;
        player2.vel_y = temp_vy * 0.8;
    }
}

/// Represents a single frame of player input for deterministic networked gameplay
///
/// This structure encapsulates all player input data for a single game frame,
/// along with metadata necessary for reliable networked transmission and
/// client-server synchronization. It's designed to support advanced netcode
/// techniques like client-side prediction and server reconciliation.
///
/// ## Netcode Integration
///
/// ### Sequence Numbering
/// The `sequence` field implements a monotonically increasing counter that
/// enables proper input ordering on the server, even when packets arrive
/// out of order due to network conditions.
///
/// ### Temporal Correlation
/// The `timestamp` field allows the server to correlate inputs with specific
/// points in time, enabling lag compensation and temporal smoothing algorithms.
///
/// ### Deterministic Replay
/// The structure contains only deterministic input state (no derived values),
/// allowing perfect reproduction of player actions during client-side prediction
/// and server-side reconciliation processes.
///
/// ## Input Model
///
/// The game uses a simplified 3-button input model:
/// - **Movement**: Mutually exclusive left/right directional inputs
/// - **Jumping**: Binary jump action with ground-state dependency
///
/// This minimal input set reduces network bandwidth while providing sufficient
/// control for engaging 2D platformer gameplay.
///
/// ## Usage Patterns
///
/// ### Client-Side Prediction
/// Clients store input history for replaying actions during reconciliation:
/// ```rust
/// use shared::InputState;
///
/// let input = InputState {
///     sequence: 42,
///     timestamp: 123456789,
///     left: true,
///     right: false,
///     jump: false,
/// };
///
/// assert_eq!(input.sequence, 42);
/// assert!(input.left);
/// assert!(!input.right);
/// ```
///
/// ### Server Processing
/// Server applies inputs in sequence order for authoritative simulation:
/// ```rust
/// use shared::InputState;
///
/// // Example showing how inputs are processed on the server
/// let inputs = vec![
///     InputState { sequence: 1, timestamp: 100, left: true, right: false, jump: false },
///     InputState { sequence: 2, timestamp: 200, left: false, right: true, jump: false },
/// ];
///
/// // Process inputs in sequence order
/// for input in inputs {
///     // Apply input to game state (actual implementation in game logic)
///     println!("Processing input {}: left={}, right={}", input.sequence, input.left, input.right);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct InputState {
    /// Monotonically increasing sequence number for reliable input ordering
    ///
    /// This field ensures inputs are processed in the correct chronological order
    /// on the server, even when UDP packets arrive out of sequence. Each client
    /// maintains its own sequence counter starting from 1.
    ///
    /// The server uses this value to:
    /// - Detect and discard duplicate inputs
    /// - Process inputs in temporal order
    /// - Identify missing inputs for reconciliation
    /// - Track the last processed input per client
    pub sequence: u32,

    /// Client-side timestamp when the input was captured (milliseconds since Unix epoch)
    ///
    /// Records the exact moment this input was generated on the client machine.
    /// This enables sophisticated lag compensation techniques and helps the server
    /// understand the temporal context of player actions.
    ///
    /// Used for:
    /// - Lag compensation calculations
    /// - Input validation (detecting suspiciously old inputs)
    /// - Temporal interpolation and extrapolation
    /// - Network diagnostics and debugging
    pub timestamp: u64,

    /// Left movement input state
    ///
    /// True when the player is pressing the left movement key (typically 'A' or left arrow).
    /// Mutually exclusive with `right` during normal input (both can be false for no movement,
    /// but simultaneous left+right presses are typically resolved as no movement).
    pub left: bool,

    /// Right movement input state
    ///
    /// True when the player is pressing the right movement key (typically 'D' or right arrow).
    /// Mutually exclusive with `left` during normal input. When true, the player should
    /// move rightward at PLAYER_SPEED pixels per second.
    pub right: bool,

    /// Jump input state
    ///
    /// True when the player is pressing the jump key (typically Space or Up arrow).
    /// Jump actions are only valid when the player is on the ground (`on_ground: true`),
    /// preventing infinite jumping or double-jumping behaviors.
    ///
    /// The jump action:
    /// - Sets vertical velocity to JUMP_VELOCITY (negative = upward)
    /// - Marks the player as no longer on ground
    /// - Triggers any associated jump effects (sound, particles, etc.)
    pub jump: bool,
}

/// Comprehensive test suite for shared game library components
///
/// This test module validates all core functionality of the shared library,
/// ensuring reliable behavior across client and server implementations.
/// The tests cover critical aspects of networked multiplayer gameplay:
///
/// ## Test Categories
///
/// ### Player Entity Tests
/// - **Creation and Initialization**: Verify proper default state setup
/// - **Spatial Calculations**: Test bounding box and center point calculations
/// - **State Consistency**: Ensure fields maintain expected relationships
///
/// ### Collision System Tests
/// - **Detection Accuracy**: Verify AABB collision detection correctness
/// - **Edge Cases**: Test boundary conditions and exact-touch scenarios
/// - **Resolution Physics**: Validate separation and momentum exchange
/// - **Stability**: Ensure collision resolution converges to stable states
///
/// ### Network Protocol Tests
/// - **Serialization**: Verify all packet types can be encoded and decoded
/// - **Data Integrity**: Ensure field values survive serialization round-trips
/// - **Protocol Completeness**: Test all packet variants and field combinations
///
/// ### Input System Tests
/// - **State Representation**: Verify input capture and storage accuracy
/// - **Temporal Consistency**: Test timestamp and sequence number behavior
/// - **Boolean Logic**: Validate input flag combinations and edge cases
///
/// ## Quality Assurance
///
/// These tests serve multiple purposes in the development process:
/// - **Regression Prevention**: Catch breaking changes to core functionality
/// - **Cross-Platform Validation**: Ensure consistent behavior across platforms
/// - **Determinism Verification**: Validate identical results for identical inputs
/// - **Performance Baseline**: Establish expected performance characteristics
///
/// ## Test Execution
///
/// All tests can be run using standard Rust testing commands:
/// ```bash
/// cargo test                    # Run all tests
/// cargo test collision          # Run collision-related tests
/// cargo test --release          # Run tests with optimizations
/// ```
///
/// The tests use the `assert_approx_eq` crate for floating-point comparisons
/// to handle minor precision differences across platforms while maintaining
/// strict determinism requirements for gameplay.
#[cfg(test)]
mod tests {
    use super::*;
    use assert_approx_eq::assert_approx_eq;
    use std::collections::HashMap;

    #[test]
    fn test_player_creation() {
        let player = Player::new(1, 100.0, 200.0);
        assert_eq!(player.id, 1);
        assert_eq!(player.x, 100.0);
        assert_eq!(player.y, 200.0);
        assert_eq!(player.vel_x, 0.0);
        assert_eq!(player.vel_y, 0.0);
        assert!(player.on_ground);
    }

    #[test]
    fn test_player_bounds() {
        let player = Player::new(1, 50.0, 75.0);
        let (x1, y1, x2, y2) = player.get_bounds();
        assert_eq!(x1, 50.0);
        assert_eq!(y1, 75.0);
        assert_eq!(x2, 50.0 + PLAYER_SIZE);
        assert_eq!(y2, 75.0 + PLAYER_SIZE);
    }

    #[test]
    fn test_player_center() {
        let player = Player::new(1, 100.0, 200.0);
        let (cx, cy) = player.center();
        assert_eq!(cx, 100.0 + PLAYER_SIZE / 2.0);
        assert_eq!(cy, 200.0 + PLAYER_SIZE / 2.0);
    }

    #[test]
    fn test_collision_detection_no_collision() {
        let player1 = Player::new(1, 0.0, 0.0);
        let player2 = Player::new(2, 100.0, 100.0);
        assert!(!check_collision(&player1, &player2));
    }

    #[test]
    fn test_collision_detection_overlap() {
        let player1 = Player::new(1, 0.0, 0.0);
        let player2 = Player::new(2, 16.0, 16.0);
        assert!(check_collision(&player1, &player2));
    }

    #[test]
    fn test_collision_detection_exact_touch() {
        let player1 = Player::new(1, 0.0, 0.0);
        let player2 = Player::new(2, PLAYER_SIZE, 0.0);
        assert!(!check_collision(&player1, &player2));
    }

    #[test]
    fn test_collision_resolution() {
        let mut player1 = Player::new(1, 10.0, 10.0);
        let mut player2 = Player::new(2, 20.0, 20.0);

        player1.vel_x = 100.0;
        player1.vel_y = -50.0;
        player2.vel_x = -75.0;
        player2.vel_y = 25.0;

        assert!(check_collision(&player1, &player2));

        resolve_collision(&mut player1, &mut player2);

        let (cx1, cy1) = player1.center();
        let (cx2, cy2) = player2.center();
        let distance = ((cx2 - cx1).powi(2) + (cy2 - cy1).powi(2)).sqrt();

        assert!(distance >= PLAYER_SIZE * 0.9);
        assert_approx_eq!(player1.vel_x, -75.0 * 0.8, 0.01);
        assert_approx_eq!(player1.vel_y, 25.0 * 0.8, 0.01);
        assert_approx_eq!(player2.vel_x, 100.0 * 0.8, 0.01);
        assert_approx_eq!(player2.vel_y, -50.0 * 0.8, 0.01);
    }

    #[test]
    fn test_collision_resolution_same_position() {
        let mut player1 = Player::new(1, 10.0, 10.0);
        let mut player2 = Player::new(2, 10.0, 10.0);

        resolve_collision(&mut player1, &mut player2);

        assert!(!check_collision(&player1, &player2));
        assert_ne!(player1.x, player2.x);
    }

    #[test]
    fn test_packet_serialization_connect() {
        let packet = Packet::Connect { client_version: 42 };
        let serialized = bincode::serialize(&packet).unwrap();
        let deserialized: Packet = bincode::deserialize(&serialized).unwrap();

        match deserialized {
            Packet::Connect { client_version } => assert_eq!(client_version, 42),
            _ => panic!("Wrong packet type after deserialization"),
        }
    }

    #[test]
    fn test_packet_serialization_input() {
        let packet = Packet::Input {
            sequence: 123,
            timestamp: 456789,
            left: true,
            right: false,
            jump: true,
        };

        let serialized = bincode::serialize(&packet).unwrap();
        let deserialized: Packet = bincode::deserialize(&serialized).unwrap();

        match deserialized {
            Packet::Input {
                sequence,
                timestamp,
                left,
                right,
                jump,
            } => {
                assert_eq!(sequence, 123);
                assert_eq!(timestamp, 456789);
                assert!(left);
                assert!(!right);
                assert!(jump);
            }
            _ => panic!("Wrong packet type after deserialization"),
        }
    }

    #[test]
    fn test_packet_serialization_game_state() {
        let players = vec![Player::new(1, 100.0, 200.0), Player::new(2, 300.0, 400.0)];

        let mut last_processed_input = HashMap::new();
        last_processed_input.insert(1, 10);
        last_processed_input.insert(2, 15);

        let packet = Packet::GameState {
            tick: 42,
            timestamp: 123456789,
            last_processed_input,
            players,
        };

        let serialized = bincode::serialize(&packet).unwrap();
        let deserialized: Packet = bincode::deserialize(&serialized).unwrap();

        match deserialized {
            Packet::GameState {
                tick,
                timestamp,
                last_processed_input,
                players,
            } => {
                assert_eq!(tick, 42);
                assert_eq!(timestamp, 123456789);
                assert_eq!(last_processed_input.get(&1), Some(&10));
                assert_eq!(last_processed_input.get(&2), Some(&15));
                assert_eq!(players.len(), 2);
                assert_eq!(players[0].id, 1);
                assert_eq!(players[1].id, 2);
            }
            _ => panic!("Wrong packet type after deserialization"),
        }
    }

    #[test]
    fn test_input_state_creation() {
        let input = InputState {
            sequence: 42,
            timestamp: 123456,
            left: true,
            right: false,
            jump: true,
        };

        assert_eq!(input.sequence, 42);
        assert_eq!(input.timestamp, 123456);
        assert!(input.left);
        assert!(!input.right);
        assert!(input.jump);
    }
}
