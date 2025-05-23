mod game;
mod network;
mod physics;
use clap::Parser;
use game::GameState;
use network::{GameCommand, NetworkServer};
use physics::Vector2;
use serde_json;
use std::sync::{mpsc, Arc};
use std::{thread, time::Duration};

fn main() {
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
    let addr = format!("{}:{}", args.host, args.port);
    let tick_interval = Duration::from_secs_f32(1.0 / args.tick_rate as f32);

    // Create a new WebSocket server instance wrapped in Arc
    let server = Arc::new(NetworkServer::new(&addr));

    // Create a new game state instance wrapped in Arc and Mutex
    let mut game_state = GameState::new();

    // Channel for network -> main (game) commands
    let (cmd_sender, cmd_reciver) = mpsc::channel::<GameCommand>();

    // Clone Arc for the thread and pass sender to network thread
    let server_thread = Arc::clone(&server);
    thread::spawn(move || {
        if let Err(e) = server_thread.start(cmd_sender) {
            eprintln!("Failed to start WebSocket-server: {}", e)
        }
    });

    game_state.add_object(Vector2 { x: 0.0, y: -7.5 }, 200.0, 10.0);
    loop {
        // Handle all pending commands from network
        while let Ok(cmd) = cmd_reciver.try_recv() {
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
        // Process the player input
        game_state.process_input();
        // Update the gamestate
        game_state.update_positions(tick_interval.as_secs_f32());

        for (id, seq) in game_state.get_players_last_seqs() {
            server.send_message_to_client(id, &serde_json::to_string(&seq).unwrap());
        }

        // Gets the objects in the gamestate, serializes, then broadcasts them.
        server.broadcast_message(&serde_json::to_string(&game_state.get_objects()).unwrap());

        // Sleep game-thread
        thread::sleep(tick_interval);
    }
}
