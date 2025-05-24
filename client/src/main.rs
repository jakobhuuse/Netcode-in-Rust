//! Client application entry point for the multiplayer netcode game
//! 
//! This is the main executable for the game client, handling:
//! - Command-line argument parsing for server connection and latency simulation
//! - Window configuration and graphics initialization
//! - Client network instance creation and main game loop execution
//! 
//! The client connects to a game server and demonstrates various netcode
//! techniques including prediction, reconciliation, and interpolation.

mod game;
mod input;
mod network;
mod rendering;

use clap::Parser;
use log::info;
use macroquad::prelude::*;

/// Command-line arguments for client configuration
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Server address in format "host:port"
    #[arg(short = 's', long, default_value = "127.0.0.1:8080")]
    server: String,

    /// Artificial latency in milliseconds for netcode testing
    #[arg(short = 'l', long, default_value = "0")]
    fake_ping: u64,
}

/// Configures the game window properties
/// 
/// Sets up a resizable window with appropriate title and dimensions
/// for the multiplayer game client interface.
fn window_conf() -> Conf {
    Conf {
        window_title: "Netcode in Rust - Client".to_owned(),
        window_width: 800,
        window_height: 600,
        window_resizable: true,
        ..Default::default()
    }
}

/// Main client application entry point
/// 
/// Initializes logging, parses command-line arguments, and starts the game client.
/// The client connects to the specified server and runs the main game loop,
/// demonstrating real-time netcode techniques in a multiplayer environment.
/// 
/// Command-line usage:
/// - `-s/--server <address>`: Server to connect to (default: 127.0.0.1:8080)
/// - `-l/--fake-ping <ms>`: Artificial latency for testing (default: 0)
/// 
/// Interactive controls:
/// - A/D: Move player left/right
/// - Space: Jump
/// - 1/2/3: Toggle prediction/reconciliation/interpolation
/// - R: Reconnect to server
#[macroquad::main(window_conf)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging system
    env_logger::init();

    if std::env::var("RUST_LOG").is_err() {
        eprintln!("Set RUST_LOG=info for detailed logging");
    }

    // Parse command-line arguments
    let args = Args::parse();

    // Display startup information and controls
    info!("Starting client...");
    info!("Connecting to: {}", args.server);
    if args.fake_ping > 0 {
        info!("Simulating {}ms latency", args.fake_ping);
    }
    info!("Controls: A/D to move, Space to jump");
    info!("Press 1/2/3 to toggle Prediction/Reconciliation/Interpolation");
    info!("Press R to reconnect to server");

    // Create and run the client
    let mut client = network::Client::new(&args.server, args.fake_ping).await?;

    client.run().await?;

    Ok(())
}
