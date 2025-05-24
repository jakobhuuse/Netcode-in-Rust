//! # Game Server Library
//!
//! This library provides the authoritative server implementation for the networked
//! multiplayer game. It manages the canonical game state, processes client inputs,
//! and broadcasts updates to maintain synchronization across all connected clients.
//!
//! ## Core Responsibilities
//!
//! ### Authoritative Simulation
//! The server runs the definitive version of the game physics and state. All
//! game logic decisions are made here, with clients receiving and conforming
//! to the server's authoritative state updates.
//!
//! ### Client Management
//! Handles the complete lifecycle of client connections including:
//! - Connection establishment and player assignment
//! - Input processing and validation
//! - Disconnection handling and cleanup
//! - Anti-cheat and abuse prevention
//!
//! ### State Broadcasting
//! Regularly transmits the current game state to all connected clients,
//! enabling them to stay synchronized and perform reconciliation when
//! their predictions diverge from reality.
//!
//! ## Architecture Design
//!
//! ### Single-Threaded Event Loop
//! The server uses a single-threaded, event-driven architecture that processes
//! all network events and game updates sequentially. This eliminates race
//! conditions and ensures deterministic behavior while maintaining high
//! performance for the expected player count.
//!
//! ### UDP-Based Communication
//! Uses UDP sockets for low-latency communication with clients. The protocol
//! includes reliability mechanisms for critical data while allowing some
//! packets (like frequent state updates) to be lost without disrupting gameplay.
//!
//! ### Input Processing Pipeline
//! Client inputs are processed in sequence order to maintain fairness and
//! prevent temporal inconsistencies. The server tracks the last processed
//! input per client to enable proper reconciliation.
//!
//! ## Module Organization
//!
//! ### Client Manager Module (`client_manager`)
//! Manages individual client connections and their associated state:
//! - Connection tracking and player ID assignment
//! - Input queue management and processing
//! - Client timeout detection and cleanup
//! - Per-client statistics and monitoring
//!
//! ### Game Module (`game`)
//! Contains the authoritative game state and simulation logic:
//! - Master game state with all player positions and velocities
//! - Physics simulation identical to client prediction
//! - Collision detection and resolution
//! - Game rule enforcement and validation
//!
//! ### Network Module (`network`)
//! Handles all networking operations and protocol implementation:
//! - UDP socket management and packet processing
//! - Message serialization and deserialization
//! - Connection establishment and termination
//! - Rate limiting and flood protection
//!
//! ## Performance Characteristics
//!
//! ### Tick Rate
//! The server runs at a fixed tick rate (typically 60Hz) to ensure consistent
//! simulation timing. Each tick processes all pending inputs and generates
//! a new game state snapshot.
//!
//! ### Scalability
//! Designed to handle multiple concurrent clients (typically 2-16 players)
//! with room for expansion. Memory usage and CPU requirements scale linearly
//! with player count.
//!
//! ### Latency Optimization
//! Minimizes processing time between input receipt and state broadcast to
//! reduce the total round-trip time experienced by clients.
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use server::network::Server;
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a new server bound to address with 60Hz tick rate and max 32 clients
//!     let mut server = Server::new(
//!         "127.0.0.1:8080",
//!         Duration::from_millis(16), // 60Hz = ~16.67ms per tick
//!         32
//!     ).await?;
//!     
//!     // Start the server - this runs the main game loop which:
//!     // - Listens for client connections and input packets
//!     // - Processes inputs in chronological order across all clients
//!     // - Runs physics simulation at the specified tick rate
//!     // - Broadcasts game state updates to all connected clients
//!     // - Handles client timeouts and disconnections
//!     server.run().await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! The server uses an event-driven architecture with internal async tasks that handle:
//! - **Network Receiver**: Continuously listens for incoming packets
//! - **Network Sender**: Processes outgoing packet queue and broadcasts
//! - **Timeout Checker**: Monitors client health and removes inactive connections
//! - **Main Game Loop**: Processes inputs, runs physics, and broadcasts state
//!
//! ## Security Considerations
//!
//! ### Input Validation
//! All client inputs are validated before application to prevent cheating
//! and ensure game rule compliance. Invalid inputs are discarded with
//! appropriate logging.
//!
//! ### Rate Limiting
//! Connection attempts and input frequency are rate-limited to prevent
//! denial-of-service attacks and resource exhaustion.
//!
//! ### State Authority
//! The server maintains absolute authority over game state, preventing
//! clients from manipulating their position or other game variables
//! through modified clients.

pub mod client_manager;
pub mod game;
pub mod network;
