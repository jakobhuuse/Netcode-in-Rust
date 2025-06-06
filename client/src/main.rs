//! Client application entry point

mod game;
mod input;
mod network;
mod network_graph;
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

/// Configures the game window
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
#[macroquad::main(window_conf)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    if std::env::var("RUST_LOG").is_err() {
        eprintln!("Set RUST_LOG=info for detailed logging");
    }

    let args = Args::parse();

    info!("Starting client...");
    info!("Connecting to: {}", args.server);
    if args.fake_ping > 0 {
        info!("Simulating {}ms latency", args.fake_ping);
    }
    info!("Controls: A/D to move, Space to jump");
    info!("Press 1/2/3 to toggle Prediction/Reconciliation/Interpolation");
    info!("Press G to toggle Network Graph");
    info!("Press R to reconnect to server");

    let mut client = network::Client::new(&args.server, args.fake_ping).await?;
    client.run().await?;

    Ok(())
}
