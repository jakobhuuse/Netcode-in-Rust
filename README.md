# Netcode in Rust

[![CI/CD](https://github.com/jakobhuuse/Netcode-in-Rust/actions/workflows/ci.yml/badge.svg)](https://github.com/jakobhuuse/Netcode-in-Rust/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)

A comprehensive implementation of modern netcode techniques for real-time multiplayer games, written in Rust. This project demonstrates client-side prediction, server reconciliation, and lag compensation in a 2D physics-based multiplayer environment.

**🎮 Try it now!** A live demo server is hosted at `game.conrados.dev:8080` for immediate multiplayer testing.

## Implemented Functionality

### Core Netcode Features

-   ✅ **UDP-based networking** with custom reliability layer
-   ✅ **Client-side prediction** with input buffering and replay
-   ✅ **Server reconciliation** using rollback and replay techniques
-   ✅ **Temporal interpolation** for smooth remote player movement
-   ✅ **Lag compensation** with configurable artificial latency for testing
-   ✅ **Connection management** with timeout detection and reconnection
-   ✅ **Input validation** and anti-cheat foundations

### Game Systems

-   ✅ **2D Physics simulation** with gravity, collision detection, and response
-   ✅ **Player movement** with responsive controls (WASD + Space)
-   ✅ **Real-time multiplayer** supporting up to 50+ concurrent players
-   ✅ **Visual debugging tools** including velocity vectors and netcode status
-   ✅ **Performance monitoring** with frame rate and latency visualization

### Development Tools

-   ✅ **Comprehensive test suite** with unit, integration, and benchmark tests
-   ✅ **Artificial latency simulation** for testing network conditions
-   ✅ **Runtime netcode toggling** for comparing techniques
-   ✅ **Docker containerization** for easy deployment
-   ✅ **CI/CD pipeline** with automated testing and deployment

## Future Work

### Current Limitations

-   **World persistence**: Game state resets on server restart
-   **Player authentication**: Basic connection model needs identity management
-   **Game modes**: Objectives, scoring, and win conditions
-   **Advanced physics**: More complex collision shapes and interactions

## External Dependencies

### Core Libraries

-   **[tokio](https://tokio.rs/)** `1.28.0` - Asynchronous runtime for network operations
-   **[serde](https://serde.rs/)** `1.0` - Serialization framework for network packets
-   **[bincode](https://github.com/bincode-org/bincode)** `1.3.3` - Binary serialization for minimal network overhead
-   **[macroquad](https://macroquad.rs/)** `0.4` - Cross-platform graphics and input library

### Development and Testing

-   **[clap](https://clap.rs/)** `4.2.1` - Command-line argument parsing
-   **[log](https://docs.rs/log/)** `0.4` + **[env_logger](https://docs.rs/env_logger/)** `0.10.0` - Structured logging
-   **[rand](https://docs.rs/rand/)** `0.8` - Random number generation
-   **[assert_approx_eq](https://docs.rs/assert_approx_eq/)** `1.1.0` - Floating-point comparison utilities

## Installation

### Quick Start (No Installation Required)

Connect to our live demo server:

```bash
git clone https://github.com/jakobhuuse/Netcode-in-Rust.git
cd Netcode-in-Rust
cargo run -p client -- --server game.conrados.dev:8080
```

### Prerequisites

-   **Rust 1.70 or later** - Install from [rustup.rs](https://rustup.rs/)
-   **Git** - For cloning the repository
-   **Docker** (optional) - For containerized deployment

### Building from Source

```bash
# Clone and build
git clone https://github.com/jakobhuuse/Netcode-in-Rust.git
cd Netcode-in-Rust
make build

# Run tests
make test
```

### Docker Deployment

```bash
# Build and run server
docker build -t netcode-server .
docker run -p 8080:8080/udp netcode-server
```

## Using the Solution

### Running the Server

```bash
# Basic server (localhost:8080, 60Hz, max 16 clients)
cargo run -p server

# Production server
cargo run -p server -- --host 0.0.0.0 --port 8080 --tick-rate 60 --max-clients 50

# High-performance setup
cargo run -p server -- --tick-rate 128 --max-clients 8
```

**Configuration Options:**

-   `--host <IP>`: Bind address (127.0.0.1 for local, 0.0.0.0 for public)
-   `--port <PORT>`: UDP port to listen on
-   `--tick-rate <HZ>`: Simulation frequency (20-128 Hz recommended)
-   `--max-clients <N>`: Maximum concurrent players

### Running the Client

```bash
# Connect to live demo server
cargo run -p client -- --server game.conrados.dev:8080

# Connect to local server
cargo run -p client

# Test with artificial latency
cargo run -p client -- --fake-ping 50
```

**Configuration Options:**

-   `--server <ADDRESS>`: Server to connect to (format: host:port)
-   `--fake-ping <MS>`: Artificial latency for netcode testing

### Gameplay Controls

**Movement:**

-   `A` / `←`: Move left
-   `D` / `→`: Move right
-   `Space`: Jump

**Debug Controls:**

-   `1`: Toggle client-side prediction on/off
-   `2`: Toggle server reconciliation on/off
-   `3`: Toggle interpolation on/off
-   `R`: Reconnect to server

**Visual Elements:**

-   Green square: Your player
-   Red squares: Other players
-   Yellow arrows: Velocity vectors (your player only)
-   UI indicators: Connection status, latency bars, player count

## Tests

### Running Tests

```bash
# Run all tests
make test

# Run specific categories
make test-shared      # Test shared library
make test-server      # Test server functionality
make test-client      # Test client functionality
make test-integration # Test cross-component integration

# Run benchmarks
make bench
```

### Test Categories

**Unit Tests** - Core component functionality:

-   Collision detection and resolution algorithms
-   Player state management and physics simulation
-   Network packet serialization and deserialization
-   Input processing and validation logic

**Integration Tests** - Cross-component validation:

-   Complete client-server communication scenarios
-   Network protocol compliance and error handling
-   Real UDP socket communication validation
-   Game logic integration across components

**Benchmark Tests** - Performance validation:

-   Collision system performance (< 1μs per check target)
-   Physics simulation scaling (100 players in < 5ms target)
-   Network serialization efficiency (< 200μs round-trip target)
-   Input processing under high load (1000 inputs in < 100ms target)

## API Documentation

### Network Protocol

The game uses a custom UDP-based protocol:

```rust
// Client -> Server
Connect { client_version: u32 }
Input { sequence: u32, timestamp: u64, left: bool, right: bool, jump: bool }
Disconnect

// Server -> Client
Connected { client_id: u32 }
GameState { tick: u32, players: Vec<Player>, ... }
Disconnected { reason: String }
```

### Server API

```rust
// Configuration
Server::new(addr: &str, tick_duration: Duration, max_clients: usize)

// Main loop
server.run().await  // Starts the main server event loop
```

### Client API

```rust
// Configuration
Client::new(server_addr: &str, fake_ping_ms: u64)

// Main loop
client.run().await  // Starts the main client game loop
```

For detailed API documentation:

```bash
cargo doc --open --no-deps
```

## Contributing

Contributions are welcome! Please submit pull requests for any improvements.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

-   Inspired by [Gabriel Gambetta's articles](https://www.gabrielgambetta.com/client-side-prediction-live-demo.html) on client-side prediction
-   Built with the Rust ecosystem
