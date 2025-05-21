mod client;
mod entity;
mod game;
mod network;
mod packets;
mod utils;

use std::sync::Arc;
use std::time::Duration;
use clap::Parser;
use log::info;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};

// Command line arguments
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Server IP address to bind to
    #[clap(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    /// WebSocket port to listen on
    #[clap(short = 'p', long, default_value = "8080")]
    port: u16,

    /// Tick rate (updates per second)
    #[clap(short = 't', long, default_value = "60")]
    tick_rate: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();
    
    // Print a message about setting RUST_LOG if not set
    if std::env::var("RUST_LOG").is_err() {
        eprintln!("Warning: RUST_LOG environment variable not set. Set it to display logs!");
        eprintln!("Recommended: RUST_LOG=info cargo run");
    }

    // Parse command line arguments
    let args = Args::parse();
    let ws_addr = format!("{}:{}", args.host, args.port);
    let tick_interval = Duration::from_secs_f32(1.0 / args.tick_rate as f32);

    info!("Starting game server on WebSocket: {}", ws_addr);
    info!("Tick rate: {} Hz ({:?} per tick)", args.tick_rate, tick_interval);

    // Create WebSocket listener
    let listener = TcpListener::bind(&ws_addr).await?;
    info!("WebSocket server listening on {}", ws_addr);

    // Create shared game state (800x600 game world)
    let game_state = Arc::new(Mutex::new(game::GameState::new(800.0, 600.0)));

    // Channel for communication between network and game threads
    let (event_tx, event_rx) = mpsc::channel::<packets::NetworkEvent>(100);

    // Spawn game loop task
    let game_state_clone = game_state.clone();
    tokio::spawn(async move {
        game::run_game_loop(game_state_clone, event_rx, tick_interval).await;
    });

    // Spawn task to check for timeouts
    let game_state_timeouts = game_state.clone();
    let timeout_duration = Duration::from_secs(5); // 5 seconds timeout
    let check_interval = Duration::from_secs(1);   // Check every second
    
    tokio::spawn(async move {
        game::timeout_checker(game_state_timeouts, check_interval, timeout_duration).await;
    });

    // Main WebSocket listener loop
    info!("WebSocket server started");
    while let Ok((stream, addr)) = listener.accept().await {
        let event_tx_clone = event_tx.clone();
        
        tokio::spawn(async move {
            network::handle_connection(stream, addr, event_tx_clone).await;
        });
    }

    Ok(())
}