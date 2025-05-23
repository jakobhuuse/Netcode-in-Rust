mod game;
mod network;
mod physics;

use clap::Parser;
use game::GameState;
use network::{GameCommand, NetworkServer};
use physics::Vector2;
use serde_json;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration, Instant, MissedTickBehavior};

/// Main-method of the application.
/// Parses command-line arguments, then creates a thread for the network-server and game-server.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Command line arguments
    #[derive(Parser, Debug)]
    #[clap(author, version, about)]
    struct Args {
        /// Server IP address to bind to
        #[clap(short = 'H', long, default_value = "127.0.0.1")]
        host: String,
        /// Server port to listen on
        #[clap(short, long, default_value = "8080")]
        port: u16,
        /// Tick rate (updates per second)
        #[clap(short, long, default_value = "30")]
        tick_rate: u32,
    }

    // Parse command line arguments
    let args = Args::parse();

    // Create shared game state with read-write lock
    let game_state = Arc::new(RwLock::new(GameState::new()));

    // Create bounded channel for game commands
    let (cmd_sender, cmd_receiver) = mpsc::channel::<GameCommand>(1000);

    // Initialize game state
    {
        let mut state = game_state.write().await;
        state.add_object(Vector2 { x: 0.0, y: -7.5 }, 200.0, 10.0);
    }

    // Create network server
    let address = format!("{}:{}", args.host, args.port);
    let server = Arc::new(NetworkServer::new(&address));

    // Spawn network thread
    let server_handle = {
        let server = Arc::clone(&server);
        tokio::spawn(async move {
            if let Err(e) = server.start(cmd_sender).await {
                eprintln!("Failed to start WebSocket server: {}", e);
            }
        })
    };

    // Spawn game loop thread
    let game_handle = {
        let game_state = Arc::clone(&game_state);
        let server = Arc::clone(&server);

        tokio::spawn(async move {
            run_game_loop(game_state, server, cmd_receiver, args.tick_rate).await;
        })
    };

    // Handle shutdown gracefully
    tokio::select! {
        result = server_handle => {
            if let Err(e) = result {
                eprintln!("Network task panicked: {}", e);
            }
        }
        result = game_handle => {
            if let Err(e) = result {
                eprintln!("Game loop task panicked: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("Received Ctrl+C, shutting down gracefully...");
        }
    }

    Ok(())
}

/// Starts the game-loop.
async fn run_game_loop(
    game_state: Arc<RwLock<GameState>>,
    server: Arc<NetworkServer>,
    mut cmd_receiver: mpsc::Receiver<GameCommand>,
    tick_rate: u32,
) {
    let mut interval_timer = interval(Duration::from_secs_f32(1.0 / tick_rate as f32));
    interval_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut last_update = Instant::now();

    // Cap the maximum delta time to 50ms
    let max_delta_time = 1.0 / 20.0;

    // Skip the first tick since it fires immediately
    interval_timer.tick().await;

    loop {
        interval_timer.tick().await;

        let current_time = Instant::now();
        let mut delta_time = (current_time - last_update).as_secs_f32();
        last_update = current_time;

        // Cap delta time to prevent instability
        if delta_time > max_delta_time {
            println!(
                "Warning: Large delta time detected ({:.3}s), capping to {:.3}s",
                delta_time, max_delta_time
            );
            delta_time = max_delta_time;
        }

        // Handle incoming commands
        let mut commands_to_process = Vec::new();
        while let Ok(cmd) = cmd_receiver.try_recv() {
            commands_to_process.push(cmd);
        }

        // Process commands
        if !commands_to_process.is_empty() {
            let mut state = game_state.write().await;
            for cmd in commands_to_process {
                process_game_command(&mut state, cmd);
            }
        }

        // Update game state
        {
            let mut state = game_state.write().await;
            state.process_input();
            state.update_positions(delta_time);
        }

        // Send updates to clients
        {
            let state = game_state.read().await;

            // Send last sequence numbers
            for (id, seq) in state.get_players_last_seqs() {
                if let Ok(message) = serde_json::to_string(&seq) {
                    server.send_message_to_client(id, &message).await;
                }
            }

            // Broadcast world state
            if let Ok(objects_json) = serde_json::to_string(&state.get_objects()) {
                server.broadcast_message(&objects_json).await;
            }
        }
    }
}

/// Calls a function in the game state based on the given GameCommand
fn process_game_command(game_state: &mut GameState, cmd: GameCommand) {
    match cmd {
        GameCommand::AddPlayer { id } => {
            game_state.add_player(id);
        }
        GameCommand::RemovePlayer { id } => {
            game_state.remove_player(id);
        }
        GameCommand::SetPlayerGravity { id, gravity } => {
            game_state.set_player_gravity(id, gravity);
        }
        GameCommand::SetPlayerMaxSpeed { id, max_speed } => {
            game_state.set_player_max_speed(id, max_speed);
        }
        GameCommand::SetPlayerAccelerationSpeed {
            id,
            acceleration_speed,
        } => {
            game_state.set_player_acceleration_speed(id, acceleration_speed);
        }
        GameCommand::SetPlayerJumpSpeed { id, jump_speed } => {
            game_state.set_player_jump_speed(id, jump_speed);
        }
        GameCommand::PlayerInput { id, input } => {
            game_state.update_player_input(id, input);
        }
    }
}
