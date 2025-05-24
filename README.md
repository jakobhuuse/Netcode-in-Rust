# Netcode in Rust

[![CI/CD](https://github.com/jakobhuuse/Netcode-in-Rust/actions/workflows/ci.yml/badge.svg)](https://github.com/jakobhuuse/Netcode-in-Rust/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)

A comprehensive implementation of modern netcode techniques for real-time multiplayer games, written in Rust. This project demonstrates client-side prediction, server reconciliation, and lag compensation in a 2D physics-based multiplayer environment.

## Introduction

This project implements advanced networking techniques commonly used in competitive multiplayer games to provide responsive gameplay despite network latency and packet loss. The implementation follows industry best practices and includes comprehensive testing for all netcode components.

The game features a simple 2D platformer where multiple players can move around, jump, and collide with each other. While the gameplay is minimal, the networking implementation is production-quality and demonstrates sophisticated techniques for maintaining consistent game state across distributed clients.

**🎮 Try it now!** A live demo server is hosted at `game.conrados.dev:8080` for immediate multiplayer testing without local setup.

**Key Features:**

-   **Client-Side Prediction**: Immediate input response for responsive gameplay
-   **Server Reconciliation**: Rollback and replay to correct prediction errors
-   **Lag Compensation**: Smooth gameplay experience despite network delays
-   **Interpolation**: Smooth animation of remote players
-   **Deterministic Physics**: Identical simulation across client and server
-   **Artificial Latency Simulation**: Built-in tools for testing network conditions

## Implemented Functionality

### Core Netcode Features

-   ✅ **UDP-based networking** with custom reliability layer for critical packets
-   ✅ **Client-side prediction** with input buffering and replay mechanisms
-   ✅ **Server reconciliation** using rollback and replay techniques
-   ✅ **Temporal interpolation** for smooth remote player movement
-   ✅ **Lag compensation** with configurable artificial latency for testing
-   ✅ **Connection management** with automatic timeout detection and reconnection
-   ✅ **Input validation** and anti-cheat foundations on the server

### Game Systems

-   ✅ **2D Physics simulation** with gravity, collision detection, and response
-   ✅ **Player movement** with responsive controls (WASD + Space)
-   ✅ **Real-time multiplayer** supporting up to 50+ concurrent players
-   ✅ **Visual debugging tools** including velocity vectors and netcode status
-   ✅ **Performance monitoring** with frame rate and latency visualization

### Development Tools

-   ✅ **Comprehensive test suite** with unit, integration, and benchmark tests
-   ✅ **Artificial latency simulation** for testing various network conditions
-   ✅ **Runtime netcode toggling** for comparing different techniques
-   ✅ **Docker containerization** for easy deployment
-   ✅ **CI/CD pipeline** with automated testing and deployment

## Future Work

### Current Limitations and Planned Improvements

**High Priority:**

-   🔄 **World persistence**: Game state currently resets on server restart
-   🔄 **Player authentication**: Basic connection model needs identity management
-   🔄 **Spectator mode**: Support for non-playing observers
-   🔄 **Game lobby system**: Match-making and private room creation

**Medium Priority:**

-   🔄 **Advanced physics**: More complex collision shapes and physics interactions
-   🔄 **Game modes**: Objectives, scoring, and win conditions
-   🔄 **Enhanced security**: Encryption and advanced anti-cheat measures
-   🔄 **Performance optimization**: Support for 100+ concurrent players

**Low Priority:**

-   🔄 **Audio system**: Networked sound effects and voice chat
-   🔄 **Mobile clients**: Cross-platform support for mobile devices
-   🔄 **Advanced graphics**: Sprites, animations, and visual effects
-   🔄 **Modding support**: Plugin system for custom game modes

## External Dependencies

### Core Libraries

-   **[tokio](https://tokio.rs/)** `1.28.0` - Asynchronous runtime for network operations and concurrent task management
-   **[serde](https://serde.rs/)** `1.0` - Serialization framework for network packet encoding/decoding
-   **[bincode](https://github.com/bincode-org/bincode)** `1.3.3` - Efficient binary serialization format for minimal network overhead
-   **[macroquad](https://macroquad.rs/)** `0.4` - Cross-platform graphics and input library for client rendering

### Development and Testing

-   **[clap](https://clap.rs/)** `4.2.1` - Command-line argument parsing for server and client configuration
-   **[log](https://docs.rs/log/)** `0.4` + **[env_logger](https://docs.rs/env_logger/)** `0.10.0` - Structured logging with configurable output levels
-   **[rand](https://docs.rs/rand/)** `0.8` - Random number generation for game physics and testing
-   **[assert_approx_eq](https://docs.rs/assert_approx_eq/)** `1.1.0` - Floating-point comparison utilities for physics testing

### Optional Dependencies (Development)

-   **[tokio-test](https://docs.rs/tokio-test/)** `0.4` - Testing utilities for async code validation

All dependencies are carefully chosen for production stability, performance, and minimal attack surface. Regular updates ensure security and compatibility with the latest Rust ecosystem.

## Installation

### Quick Start (No Installation Required)

Want to try the netcode immediately? Connect to our live demo server:

```bash
# Clone and run - connects to game.conrados.dev:8080
git clone https://github.com/yourusername/netcode-rust-workspace.git
cd netcode-rust-workspace
cargo run -p client -- --server game.conrados.dev:8080
```

### Prerequisites

-   **Rust 1.70 or later** - Install from [rustup.rs](https://rustup.rs/)
-   **Git** - For cloning the repository
-   **Docker** (optional) - For containerized deployment

### Building from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/netcode-rust-workspace.git
cd netcode-rust-workspace

# Build all components (server, client, shared library)
make build

# Or build with optimizations for better performance
make build-release

# Verify installation by running tests
make test
```

### Docker Deployment

```bash
# Build server container
docker build -t netcode-server .

# Run server with default configuration
docker run -p 8080:8080/udp netcode-server

# Run with custom configuration
docker run -p 9999:9999/udp netcode-server \
  server --host 0.0.0.0 --port 9999 --tick-rate 128 --max-clients 32
```

### Platform-Specific Notes

**Linux:** All features supported, recommended for server deployment

```bash
# Install additional dependencies for graphics (Ubuntu/Debian)
sudo apt-get install libgl1-mesa-dev libasound2-dev
```

**macOS:** Full client and server support

```bash
# No additional dependencies required
```

**Windows:** Client and server supported, PowerShell recommended

```bash
# No additional dependencies required for basic functionality
```

## Using the Solution

### Running the Server

The game server manages authoritative game state and handles client connections:

```bash
# Basic server (localhost:8080, 60Hz, max 16 clients)
cargo run -p server

# Production server (all interfaces, custom settings)
cargo run -p server -- --host 0.0.0.0 --port 8080 --tick-rate 60 --max-clients 50

# High-performance competitive server
cargo run -p server -- --tick-rate 128 --max-clients 8
```

**Server Configuration Options:**

-   `--host <IP>`: Bind address (127.0.0.1 for local, 0.0.0.0 for public)
-   `--port <PORT>`: UDP port to listen on
-   `--tick-rate <HZ>`: Simulation frequency (20-128 Hz recommended)
-   `--max-clients <N>`: Maximum concurrent players

### Running the Client

The game client connects to a server and provides the player interface:

```bash
# Quick start: Connect to live demo server
cargo run -p client -- --server game.conrados.dev:8080

# Connect to local server
cargo run -p client

# Connect to custom remote server
cargo run -p client -- --server 192.168.1.100:8080

# Test with artificial latency (50ms simulated ping)
cargo run -p client -- --fake-ping 50

# Connect to demo server with artificial latency for testing
cargo run -p client -- --server game.conrados.dev:8080 --fake-ping 100
```

**Client Configuration Options:**

-   `--server <ADDRESS>`: Server to connect to (format: host:port)
-   `--fake-ping <MS>`: Artificial latency for netcode testing

**🌐 Live Demo Server:** `game.conrados.dev:8080` - No setup required, just connect and play with others!

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
-   Colored dots: Player ID indicators
-   UI indicators: Connection status, latency bars, player count

### Development Workflow

```bash
# Format code
make format

# Run linting
make lint

# Run comprehensive tests
make test

# Run performance benchmarks
make bench

# Clean build artifacts
make clean
```

## Tests

The project includes comprehensive testing to ensure reliability and performance:

### Running Tests

```bash
# Run all tests (unit, integration, benchmarks)
make test

# Run specific test categories
make test-shared      # Test shared library components
make test-server      # Test server-specific functionality
make test-client      # Test client-specific functionality
make test-integration # Test cross-component integration

# Run performance benchmarks
make bench           # Standard benchmarks
make bench-release   # Optimized benchmarks (more accurate)
```

### Test Categories

**Unit Tests (`cargo test --lib`)**

-   Collision detection and resolution algorithms
-   Player state management and physics simulation
-   Network packet serialization and deserialization
-   Input processing and validation logic

**Integration Tests (`cargo test --test integration_tests`)**

-   Complete client-server communication scenarios
-   Network protocol compliance and error handling
-   Real UDP socket communication validation
-   Game logic integration across components

**Benchmark Tests (`cargo test --test benchmark_tests`)**

-   Collision system performance (< 1μs per check target)
-   Physics simulation scaling (100 players in < 5ms target)
-   Network serialization efficiency (< 200μs round-trip target)
-   Input processing under high load (1000 inputs in < 100ms target)

### Continuous Integration

The project uses GitHub Actions for automated testing:

-   **Format Check**: Ensures consistent code style with `rustfmt`
-   **Lint Check**: Validates code quality with `clippy`
-   **Test Suite**: Runs all tests on every push and pull request
-   **Build Verification**: Confirms successful compilation across components
-   **Deployment**: Automatically deploys server to production on main branch

## API Documentation

### Network Protocol

The game uses a custom UDP-based protocol with the following packet types:

```rust
// Client -> Server
Connect { client_version: u32 }                    // Initial connection request
Input { sequence: u32, timestamp: u64, ... }       // Player input with sequencing
Disconnect                                          // Graceful disconnection

// Server -> Client
Connected { client_id: u32 }                       // Connection acknowledgment
GameState { tick: u32, players: Vec<Player>, ... } // Authoritative state update
Disconnected { reason: String }                    // Connection termination
```

### Server API

**Configuration:**

```rust
Server::new(addr: &str, tick_duration: Duration, max_clients: usize)
```

**Main Loop:**

```rust
server.run().await  // Starts the main server event loop
```

### Client API

**Configuration:**

```rust
Client::new(server_addr: &str, fake_ping_ms: u64)
```

**Main Loop:**

```rust
client.run().await  // Starts the main client game loop
```

For detailed API documentation, run:

```bash
cargo doc --open --no-deps
```

---

## Contributing

Contributions are welcome! Please read our contributing guidelines and submit pull requests for any improvements.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

-   Inspired by [Gabriel Gambetta's excellent articles](https://www.gabrielgambetta.com/client-side-prediction-live-demo.html) on client-side prediction
-   Built with the amazing Rust ecosystem
