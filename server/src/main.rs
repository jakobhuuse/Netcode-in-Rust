mod game;
mod network;
use clap::Parser;
use game::GameState;
use network::{GameCommand, NetworkServer};
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
        #[clap(short, long, default_value = "1")]
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
                GameCommand::Move { id, dx, dy } => {
                    game_state.move_player(id, (dx, dy));
                }
            }
        }

        // Broadcast game state to all clients
        let positions = game_state.get_player_positions();
        let msg = format!("{:?}", positions);
        server.broadcast_message(&msg);

        thread::sleep(tick_interval);
    }
}
