//! # Netcode Game Server
//!
//! This is the entry point for the authoritative game server that manages multiplayer
//! gameplay for a networked physics-based game. The server maintains the definitive
//! game state and handles client connections, input processing, and state synchronization.
//!
//! ## Command Line Interface
//!
//! The server accepts various configuration options via command line arguments:
//! - **Host/Port**: Network binding configuration  
//! - **Tick Rate**: Server simulation frequency (impacts responsiveness vs performance)
//! - **Max Clients**: Connection capacity limit
//!
//! ## Architecture Overview
//!
//! The server implements a concurrent, event-driven architecture:
//! - **Main Thread**: Coordinates game simulation and state broadcasting
//! - **Network Tasks**: Handle packet I/O asynchronously
//! - **Client Manager**: Tracks connections and input queues
//! - **Game State**: Authoritative physics simulation
//!
//! ## Usage Examples
//!
//! ```bash
//! # Start server on default settings (localhost:8080, 60Hz, 16 clients)
//! ./server
//!
//! # Custom configuration
//! ./server --host 0.0.0.0 --port 9999 --tick-rate 30 --max-clients 32
//!
//! # High-performance setup for competitive play
//! ./server --tick-rate 128 --max-clients 8
//! ```

mod client_manager;
mod game;
mod network;

use clap::Parser;
use log::info;
use std::time::Duration;

/// Command line argument configuration for the game server.
///
/// Uses the `clap` crate for parsing and validation of command line arguments,
/// providing a user-friendly interface for server configuration.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Server IP address to bind to.
    ///
    /// Use "127.0.0.1" for local testing, "0.0.0.0" for public access.
    /// Default: "127.0.0.1" (localhost only)
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    /// Server port to listen on.
    ///
    /// Choose an available port above 1024 for non-privileged operation.
    /// Default: 8080
    #[arg(short = 'p', long, default_value = "8080")]
    port: u16,

    /// Server tick rate in updates per second.
    ///
    /// Higher rates provide more responsive gameplay but increase CPU usage.
    /// Common values: 20 (casual), 60 (standard), 128 (competitive)
    /// Default: 60 Hz
    #[arg(short = 't', long, default_value = "60")]
    tick_rate: u32,

    /// Maximum number of concurrent client connections.
    ///
    /// Higher limits require more memory and CPU resources.
    /// Consider network bandwidth when setting this value.
    /// Default: 16 clients
    #[arg(short = 'm', long, default_value = "16")]
    max_clients: usize,
}

/// Main entry point for the game server.
///
/// This function orchestrates server initialization and startup:
///
/// 1. **Logging Setup**: Configures structured logging for debugging and monitoring
/// 2. **Argument Parsing**: Processes command line configuration options
/// 3. **Server Creation**: Initializes the network server with specified parameters
/// 4. **Main Loop**: Starts the server's main execution loop
///
/// ## Error Handling
///
/// The function propagates errors up to the runtime, which will log them and exit
/// with an appropriate error code. Common failure points:
/// - Port already in use (address binding failure)
/// - Invalid network interface (host binding failure)
/// - System resource limits (too many file descriptors)
///
/// ## Logging Configuration
///
/// The server uses the `env_logger` crate for structured logging. Set the `RUST_LOG`
/// environment variable to control verbosity:
/// - `RUST_LOG=error`: Only critical errors
/// - `RUST_LOG=warn`: Warnings and errors  
/// - `RUST_LOG=info`: Standard operational logging (recommended)
/// - `RUST_LOG=debug`: Detailed debugging output
/// - `RUST_LOG=trace`: Extremely verbose tracing (performance impact)
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging system
    env_logger::init();

    // Provide helpful hint if logging isn't configured
    if std::env::var("RUST_LOG").is_err() {
        eprintln!("Set RUST_LOG=info for detailed logging");
    }

    // Parse command line arguments
    let args = Args::parse();
    let addr = format!("{}:{}", args.host, args.port);
    let tick_duration = Duration::from_secs_f32(1.0 / args.tick_rate as f32);

    // Log server configuration
    info!("Starting game server on {}", addr);
    info!(
        "Tick rate: {} Hz ({:?} per tick)",
        args.tick_rate, tick_duration
    );
    info!("Max clients: {}", args.max_clients);

    // Initialize and start the server
    let mut server = network::Server::new(&addr, tick_duration, args.max_clients).await?;
    server.run().await?;

    Ok(())
}
