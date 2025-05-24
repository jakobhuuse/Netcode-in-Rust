//! # Game Client Library
//!
//! This library provides the complete client-side implementation for the networked
//! multiplayer game. It handles all aspects of client functionality including input
//! capture, network communication, local game state management, and rendering.
//!
//! ## Architecture Overview
//!
//! The client is designed around a predictive netcode architecture that provides
//! responsive gameplay despite network latency and packet loss. Key components work
//! together to deliver a smooth multiplayer experience:
//!
//! ### Client-Side Prediction
//! The client maintains a local copy of the game state and applies player inputs
//! immediately without waiting for server confirmation. This eliminates the
//! perceived input lag that would otherwise make the game feel unresponsive.
//!
//! ### Server Reconciliation
//! When authoritative game states arrive from the server, the client compares
//! them with its predicted state. Any discrepancies trigger a reconciliation
//! process that replays unconfirmed inputs to bring the client back in sync.
//!
//! ### Lag Compensation
//! Input timing and interpolation algorithms help compensate for network latency,
//! ensuring that players see consistent and fair gameplay regardless of their
//! connection quality.
//!
//! ## Module Organization
//!
//! ### Game Module (`game`)
//! Contains the client-side game state management including:
//! - Local player state prediction
//! - Input history for reconciliation
//! - Physics simulation (identical to server)
//! - State reconciliation algorithms
//!
//! ### Input Module (`input`)
//! Handles player input capture and processing:
//! - Keyboard/controller input detection
//! - Input state packaging for network transmission
//! - Input history management for reconciliation
//! - Input sequence numbering for reliability
//!
//! ### Network Module (`network`)
//! Manages all client-server communication:
//! - UDP socket management and connection handling
//! - Packet serialization and deserialization
//! - Reliability mechanisms for critical packets
//! - Connection state monitoring and recovery
//!
//! ### Rendering Module (`rendering`)
//! Provides the visual representation of the game:
//! - Player and world rendering
//! - Interpolation for smooth animation
//! - Debug visualization tools
//! - Performance monitoring displays
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use client::network::Client;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create client with server address and optional artificial latency for testing
//!     let mut client = Client::new("127.0.0.1:8080", 0).await?;
//!     
//!     // Run the client - this handles the complete game loop including:
//!     // - Network connection and packet processing
//!     // - Input capture and transmission to server  
//!     // - Client-side prediction and server reconciliation
//!     // - Rendering with interpolation and netcode visualization
//!     client.run().await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Design Philosophy
//!
//! ### Responsiveness First
//! Every design decision prioritizes immediate visual feedback to player actions.
//! The client never waits for server confirmation before showing the results
//! of player input, creating a responsive and engaging experience.
//!
//! ### Deterministic Simulation
//! The client runs the exact same physics simulation as the server, using
//! identical constants and algorithms from the shared library. This ensures
//! accurate prediction and smooth reconciliation.
//!
//! ### Graceful Degradation
//! The client is designed to handle various network conditions gracefully:
//! - High latency: Continues predicting with increased reconciliation
//! - Packet loss: Maintains gameplay with periodic re-synchronization
//! - Disconnection: Provides clear feedback and reconnection options
//!
//! ### Resource Efficiency
//! Careful attention to performance ensures smooth gameplay on a wide range
//! of hardware while minimizing battery usage on mobile devices.

pub mod game;
pub mod input;
pub mod network;
pub mod rendering;
