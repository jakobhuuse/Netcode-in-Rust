use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use std::net::SocketAddr;

use crate::packets::{Packet, NetworkEvent};

// Process a received packet
pub async fn process_packet(
    packet: Packet,
    client_id: u32,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    match packet {
        Packet::ConnectionRequest { client_version } => {
            debug!("Connection request from client {} (version: {})", client_id, client_version);
            
            // Connection is already established at this point, so we can just acknowledge
            // The actual client creation happens in the game loop
        },
        
        Packet::PlayerInput { sequence, timestamp, input_vector } => {
            debug!("PlayerInput from client {}: seq {}, ts {}, vec {:?}", 
                   client_id, sequence, timestamp, input_vector);
            
            // Forward input to game loop
            if let Err(e) = event_tx.send(NetworkEvent::PlayerInput {
                client_id,
                sequence,
                timestamp,
                input_vector,
            }).await {
                error!("Failed to send PlayerInput to game thread: {}", e);
            }
        },
        
        Packet::Heartbeat { timestamp } => {
            debug!("Heartbeat from client {}: ts {}", client_id, timestamp);
            // Heartbeats are handled automatically by WebSockets
        },
        
        Packet::Disconnect { reason } => {
            info!("Disconnect request from client {}: {}", client_id, reason);
            
            // Forward disconnect to game loop
            if let Err(e) = event_tx.send(NetworkEvent::ClientDisconnect { client_id }).await {
                error!("Failed to send ClientDisconnect to game thread: {}", e);
            }
        },
        
        Packet::ConnectionAccepted { .. } | Packet::GameState { .. } => {
            warn!("Received server-sent type packet from client {}", client_id);
        }
    }
}

// Handle WebSocket connection
pub async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    event_tx: mpsc::Sender<NetworkEvent>,
) {
    info!("New WebSocket connection from {}", addr);
    
    // Accept WebSocket connection
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            error!("Error during WebSocket handshake: {}", e);
            return;
        }
    };
    
    info!("WebSocket connection established: {}", addr);
    
    // Split WebSocket stream
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    
    // Create channel for sending messages to client
    let (client_tx, mut client_rx) = mpsc::channel(100);
    
    // Assign a client ID (this will be set properly when client connects)
    let client_id = rand::random::<u32>() % 1000 + 1;
    
    // Send the connection event to the game loop
    match event_tx.send(NetworkEvent::NewConnection { 
        client_id, 
        sender: client_tx.clone() 
    }).await {
        Ok(_) => {},
        Err(e) => {
            error!("Failed to send connection event: {}", e);
            return;
        }
    }
    
    // Task for forwarding messages from game to client
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = client_rx.recv().await {
            if let Err(e) = ws_sender.send(msg).await {
                error!("Error sending WebSocket message: {}", e);
                break;
            }
        }
    });
    
    // Main receive loop for client messages
    while let Some(result) = ws_receiver.next().await {
        match result {
            Ok(msg) => {
                match msg {
                    Message::Text(text) => {
                        // Parse JSON message
                        match serde_json::from_str::<Packet>(&text) {
                            Ok(packet) => {
                                process_packet(packet, client_id, &event_tx).await;
                            },
                            Err(e) => {
                                warn!("Failed to parse JSON from {}: {}", addr, e);
                            }
                        }
                    },
                    Message::Binary(data) => {
                        // Parse binary message
                        match bincode::deserialize::<Packet>(&data) {
                            Ok(packet) => {
                                process_packet(packet, client_id, &event_tx).await;
                            },
                            Err(e) => {
                                warn!("Failed to deserialize binary data from {}: {}", addr, e);
                            }
                        }
                    },
                    Message::Close(_) => {
                        info!("WebSocket closed by client: {}", addr);
                        break;
                    },
                    _ => {} // Ignore other message types (ping/pong, etc.)
                }
            },
            Err(e) => {
                warn!("WebSocket error from {}: {}", addr, e);
                break;
            }
        }
    }
    
    // Handle disconnection
    info!("WebSocket client {} disconnected", addr);
    if let Err(e) = event_tx.send(NetworkEvent::ClientDisconnect { client_id }).await {
        error!("Failed to send disconnect event: {}", e);
    }
    
    // Cancel sender task
    sender_task.abort();
}