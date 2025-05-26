//! Game server entry point

mod client_manager;
mod game;
mod network;

use clap::Parser;
use log::info;
use std::time::Duration;

/// Command line arguments for the game server
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Server IP address to bind to
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    /// Server port to listen on
    #[arg(short = 'p', long, default_value = "8080")]
    port: u16,

    /// Server tick rate in updates per second
    #[arg(short = 't', long, default_value = "60")]
    tick_rate: u32,

    /// Maximum number of concurrent client connections
    #[arg(short = 'm', long, default_value = "16")]
    max_clients: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    if std::env::var("RUST_LOG").is_err() {
        eprintln!("Set RUST_LOG=info for detailed logging");
    }

    let args = Args::parse();
    let addr = format!("{}:{}", args.host, args.port);
    let tick_duration = Duration::from_secs_f32(1.0 / args.tick_rate as f32);

    info!("Starting game server on {}", addr);
    info!(
        "Tick rate: {} Hz ({:?} per tick)",
        args.tick_rate, tick_duration
    );
    info!("Max clients: {}", args.max_clients);

    let mut server = network::Server::new(&addr, tick_duration, args.max_clients).await?;
    server.run().await?;

    Ok(())
}
