mod game;
mod input;
mod network;
mod rendering;

use clap::Parser;
use log::info;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Server address to connect to
    #[arg(short = 's', long, default_value = "127.0.0.1:8080")]
    server: String,

    /// Simulate network latency in milliseconds
    #[arg(short = 'l', long, default_value = "0")]
    fake_ping: u64,

    /// Window width
    #[arg(short = 'w', long, default_value = "800")]
    width: usize,

    /// Window height (no short flag to avoid conflict with --help)
    #[arg(long, default_value = "600")]
    height: usize,
}

#[tokio::main]
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

    let mut client =
        network::Client::new(&args.server, args.fake_ping, args.width, args.height).await?;

    client.run().await?;

    Ok(())
}
