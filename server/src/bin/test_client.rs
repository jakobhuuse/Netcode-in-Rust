use bincode::{deserialize, serialize};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use tokio::net::UdpSocket;
use tokio::time::sleep;

#[derive(Debug, Serialize, Deserialize, Clone)]
enum Packet {
    ConnectionRequest { client_version: u32 },
    ConnectionAccepted { client_id: u32 },
    Heartbeat { timestamp: u64 },
    Disconnect { reason: String },
    PlayerInput {
        sequence: u32,
        timestamp: u64,
        input_vector: (f32, f32),
    },
    GameState {
        timestamp: u64,
        last_processed_input: u32,
        entities: Vec<Entity>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Entity {
    id: u32,
    entity_type: EntityType,
    position: (f32, f32),
    velocity: (f32, f32),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum EntityType {
    Player,
}

// Get current timestamp in milliseconds
fn get_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create local socket
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    println!("Client socket bound to {}", socket.local_addr()?);
    
    // Server address
    let server_addr = "127.0.0.1:8080".parse::<SocketAddr>()?;
    
    // Prepare connection request
    let connect_packet = Packet::ConnectionRequest { client_version: 1 };
    let connect_data = serialize(&connect_packet)?;
    
    // Send connection request
    println!("Sending connection request to {}", server_addr);
    socket.send_to(&connect_data, server_addr).await?;
    
    // Buffer for receiving data
    let mut buf = [0u8; 2048];
    
    // Wait for response
    println!("Waiting for server response...");
    let (len, addr) = socket.recv_from(&mut buf).await?;
    println!("Received {} bytes from {}", len, addr);
    
    // Try to deserialize response
    match deserialize::<Packet>(&buf[0..len]) {
        Ok(packet) => {
            println!("Received packet: {:?}", packet);
            
            // Check if connection was accepted
            if let Packet::ConnectionAccepted { client_id } = packet {
                println!("Connection accepted with client ID: {}", client_id);
                
                // Now let's send some input
                let mut sequence = 1;
                
                // Send input every second for 10 seconds
                for i in 0..10 {
                    // Create input packet with changing direction
                    let input_x = (i as f32 / 5.0).sin();
                    let input_y = (i as f32 / 5.0).cos();
                    
                    let input_packet = Packet::PlayerInput {
                        sequence,
                        timestamp: get_timestamp(),
                        input_vector: (input_x, input_y),
                    };
                    
                    let input_data = serialize(&input_packet)?;
                    println!("Sending input: {:?}", input_packet);
                    socket.send_to(&input_data, server_addr).await?;
                    
                    sequence += 1;
                    
                    // Wait for GameState response
                    match socket.recv_from(&mut buf).await {
                        Ok((len, _)) => {
                            match deserialize::<Packet>(&buf[0..len]) {
                                Ok(Packet::GameState { timestamp, last_processed_input, entities }) => {
                                    println!("Game state update - time: {}, last input: {}, entities: {}", 
                                             timestamp, last_processed_input, entities.len());
                                    
                                    // Print entity positions
                                    for e in entities {
                                        println!("  Entity {}: pos={:?}, vel={:?}", e.id, e.position, e.velocity);
                                    }
                                },
                                Ok(other) => println!("Unexpected packet: {:?}", other),
                                Err(e) => println!("Failed to deserialize game state: {}", e),
                            }
                        },
                        Err(e) => println!("Error receiving game state: {}", e),
                    }
                    
                    // Wait a second between inputs
                    sleep(Duration::from_secs(1)).await;
                }
                
                // Send disconnect when done
                let disconnect_packet = Packet::Disconnect { 
                    reason: "Client test complete".to_string() 
                };
                let disconnect_data = serialize(&disconnect_packet)?;
                println!("Sending disconnect request");
                socket.send_to(&disconnect_data, server_addr).await?;
                
                println!("Test client finished");
            } else {
                println!("Expected ConnectionAccepted but got: {:?}", packet);
            }
        },
        Err(e) => println!("Failed to deserialize response: {}", e),
    }
    
    Ok(())
}