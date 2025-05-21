mod network;
use clap::Parser;
use network::NetworkServer;
use std::{sync::Arc, thread, time::Duration};

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

    // Clone Arc for the thread
    let server_thread = Arc::clone(&server);

    // Start the nework-server on a new thread
    thread::spawn(move || {
        if let Err(e) = server_thread.start() {
            eprintln!("Failed to start WebSocket server: {}", e);
        }
    });

    loop {
        server.broadcast_message("message");
        thread::sleep(tick_interval);
    }
}
