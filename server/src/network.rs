use std::{
    collections::HashMap,
    io::Error,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use crate::game::PlayerInputState;
use base64::{engine::general_purpose::STANDARD, Engine};
use sha1::{Digest, Sha1};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, RwLock},
};

// Type alias for the client map, now storing Senders of Vec<u8>
type ClientMap = Arc<RwLock<HashMap<usize, mpsc::Sender<Vec<u8>>>>>; // MODIFIED: Vec<u8>

/// GameCommand enum for handling game-related commands
pub enum GameCommand {
    AddPlayer { id: usize },
    RemovePlayer { id: usize },
    SetPlayerGravity { id: usize, gravity: f32 },
    SetPlayerMaxSpeed { id: usize, max_speed: f32 },
    SetPlayerAccelerationSpeed { id: usize, acceleration_speed: f32 },
    SetPlayerJumpSpeed { id: usize, jump_speed: f32 },
    PlayerInput { id: usize, input: PlayerInputState },
}

/// A simple async WebSocket server implementation
pub struct NetworkServer {
    addr: String,
    clients: ClientMap,
    next_id: AtomicUsize,
}

impl NetworkServer {
    /// Create a new NetworkServer instance
    pub fn new(addr: &str) -> Self {
        NetworkServer {
            addr: addr.to_string(),
            clients: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicUsize::new(0),
        }
    }

    /// Start the async WebSocket server with a channel for game commands
    pub async fn start(&self, cmd_sender: mpsc::Sender<GameCommand>) -> Result<(), Error> {
        let listener = TcpListener::bind(&self.addr).await?;
        println!("WebSocket server listening on ws://{}", self.addr);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    println!("New connection from: {}", addr);
                    let id = self.next_id.fetch_add(1, Ordering::Relaxed);

                    // Create a channel for sending messages
                    let (client_sender, client_receiver) = mpsc::channel::<Vec<u8>>(100);

                    // Store the sender in the clients map
                    self.clients.write().await.insert(id, client_sender);

                    let server_ref = self.clone_refs();
                    let cmd_sender_clone = cmd_sender.clone();

                    // Spawn a new task for each client
                    tokio::spawn(async move {
                        server_ref
                            .handle_client(stream, id, cmd_sender_clone, client_receiver)
                            .await;
                    });
                }
                Err(e) => {
                    eprintln!("Connection failed: {}", e);
                }
            }
        }
    }

    /// Create a lightweight clone containing only reference-counted fields
    fn clone_refs(&self) -> Self {
        NetworkServer {
            addr: self.addr.clone(),
            clients: Arc::clone(&self.clients),
            next_id: AtomicUsize::new(self.next_id.load(Ordering::Relaxed)),
        }
    }

    /// Broadcast a message to all connected clients asynchronously
    pub async fn broadcast_message(&self, message: &str) {
        // create_websocket_frame already returns Vec<u8>
        let frame = self.create_websocket_frame(message.to_string());
        let clients = self.clients.read().await;

        // Collect all client tasks to run concurrently
        let mut send_tasks = Vec::new();

        for (&id, client_sender) in clients.iter() {
            let frame_clone = frame.clone();
            let client_sender_clone = client_sender.clone();

            let task = tokio::spawn(async move {
                if let Err(e) = client_sender_clone.send(frame_clone).await {
                    eprintln!("Failed to send message to client {} via channel: {}", id, e);
                }
            });

            send_tasks.push(task);
        }

        for task in send_tasks {
            let _ = task.await;
        }
    }

    /// Send a message to a specific client by id asynchronously
    pub async fn send_message_to_client(&self, id: usize, message: &str) {
        let frame = self.create_websocket_frame(message.to_string());
        let clients = self.clients.read().await;

        if let Some(client_sender) = clients.get(&id) {
            // Send the raw byte vector directly
            if let Err(e) = client_sender.send(frame).await {
                eprintln!("Failed to send message to client {} via channel: {}", id, e);
            }
        }
    }

    /// Handle an individual client connection with a channel for game commands
    async fn handle_client(
        &self,
        mut stream: TcpStream,
        id: usize,
        cmd_sender: mpsc::Sender<GameCommand>,
        mut client_receiver: mpsc::Receiver<Vec<u8>>,
    ) {
        let mut buffer = [0; 1024];

        // Perform WebSocket handshake
        let handshake_success = {
            match stream.read(&mut buffer).await {
                Ok(size) => {
                    let request = String::from_utf8_lossy(&buffer[..size]);

                    if let Some(key) = self.extract_websocket_key(&request) {
                        let accept_key = self.generate_accept_key(&key);
                        let response = format!(
                            "HTTP/1.1 101 Switching Protocols\r\n\
                             Upgrade: websocket\r\n\
                             Connection: Upgrade\r\n\
                             Sec-WebSocket-Accept: {}\r\n\r\n",
                            accept_key
                        );

                        match stream.write_all(response.as_bytes()).await {
                            Ok(_) => {
                                println!("Handshake completed for client {}!", id);
                                true
                            }
                            Err(e) => {
                                eprintln!("Failed to complete handshake for client {}: {}", id, e);
                                false
                            }
                        }
                    } else {
                        eprintln!("Invalid WebSocket handshake from client {}", id);
                        false
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read handshake from client {}: {}", id, e);
                    false
                }
            }
        };

        if !handshake_success {
            self.cleanup_client(id).await;
            return;
        }

        // Notify game that player joined
        let _ = cmd_sender.send(GameCommand::AddPlayer { id }).await;

        // Main client loop
        loop {
            let mut frame = [0; 1024];
            tokio::select! {
                // Select between reading from stream and receiving internal messages
                read_result = stream.read(&mut frame) => {
                    match read_result {
                        Ok(size) if size == 0 => {
                            println!("Client {} disconnected abruptly.", id);
                            self.cleanup_client(id).await;
                            let _ = cmd_sender.send(GameCommand::RemovePlayer { id }).await;
                            break;
                        }
                        Ok(size) => {
                            let opcode = frame[0] & 0x0F;

                            if opcode == 0x8 {
                                println!("Client {} sent close frame. Sending close response...", id);

                                // Send close response
                                let close_frame = vec![0x88, 0x00];
                                let _ = stream.write_all(&close_frame).await;

                                self.cleanup_client(id).await;
                                let _ = cmd_sender.send(GameCommand::RemovePlayer { id }).await;
                                break;
                            }

                            let message = self.parse_websocket_frame(&frame[..size]).to_lowercase();
                            println!("Received from client {}: {}", id, message);

                            self.process_client_message(&message, id, &cmd_sender).await;
                        }
                        Err(e) => {
                            eprintln!("Error reading from client {}: {}", id, e);
                            self.cleanup_client(id).await;
                            let _ = cmd_sender.send(GameCommand::RemovePlayer { id }).await;
                            break;
                        }
                    }
                },
                // Handle messages from the game loop
                Some(frame_to_send) = client_receiver.recv() => {
                    if let Err(e) = stream.write_all(&frame_to_send).await {
                        eprintln!("Failed to send outgoing message to client {}: {}", id, e);
                        self.cleanup_client(id).await;
                        let _ = cmd_sender.send(GameCommand::RemovePlayer { id }).await;
                        break;
                    }
                },
                else => {
                    break;
                }
            }
        }
    }

    /// Process a message from a client and send appropriate game commands
    async fn process_client_message(
        &self,
        message: &str,
        id: usize,
        cmd_sender: &mpsc::Sender<GameCommand>,
    ) {
        let parts: Vec<&str> = message.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "set_gravity" if parts.len() == 2 => {
                if let Ok(gravity) = parts[1].parse::<f32>() {
                    let _ = cmd_sender
                        .send(GameCommand::SetPlayerGravity { id, gravity })
                        .await;
                }
            }
            "set_max_speed" if parts.len() == 2 => {
                if let Ok(max_speed) = parts[1].parse::<f32>() {
                    let _ = cmd_sender
                        .send(GameCommand::SetPlayerMaxSpeed { id, max_speed })
                        .await;
                }
            }
            "set_acceleration_speed" if parts.len() == 2 => {
                if let Ok(acceleration_speed) = parts[1].parse::<f32>() {
                    let _ = cmd_sender
                        .send(GameCommand::SetPlayerAccelerationSpeed {
                            id,
                            acceleration_speed,
                        })
                        .await;
                }
            }
            "set_jump_speed" if parts.len() == 2 => {
                if let Ok(jump_speed) = parts[1].parse::<f32>() {
                    let _ = cmd_sender
                        .send(GameCommand::SetPlayerJumpSpeed { id, jump_speed })
                        .await;
                }
            }
            _ if parts.iter().any(|p| p.contains('=')) => {
                if let Ok(input) = message.parse::<PlayerInputState>() {
                    let _ = cmd_sender
                        .send(GameCommand::PlayerInput { id, input })
                        .await;
                }
            }
            _ => {
                println!("Unknown command from client {}: {}", id, message);
            }
        }
    }

    /// Clean up a disconnected client
    async fn cleanup_client(&self, id: usize) {
        let mut clients = self.clients.write().await;
        clients.remove(&id);
    }

    /// Create a WebSocket frame from a message
    fn create_websocket_frame(&self, message: String) -> Vec<u8> {
        let mut frame = vec![0x81];
        let payload = message.as_bytes();

        if payload.len() < 126 {
            frame.push(payload.len() as u8);
        } else if payload.len() < 65536 {
            frame.push(126);
            frame.push(((payload.len() >> 8) & 0xFF) as u8);
            frame.push((payload.len() & 0xFF) as u8);
        } else {
            frame.push(127);
            let len = payload.len() as u64;
            for i in (0..8).rev() {
                frame.push(((len >> (i * 8)) & 0xFF) as u8);
            }
        }

        frame.extend_from_slice(payload);
        frame
    }

    /// Parse a WebSocket frame into a string message
    fn parse_websocket_frame(&self, frame: &[u8]) -> String {
        if frame.len() < 6 {
            return String::new();
        }

        let payload_length = (frame[1] & 127) as usize;

        // Handle different payload length encodings
        let (actual_length, payload_start) = if payload_length < 126 {
            (payload_length, 6)
        } else if payload_length == 126 {
            if frame.len() < 8 {
                return String::new();
            }
            let len = ((frame[2] as usize) << 8) | (frame[3] as usize);
            (len, 8)
        } else {
            // payload_length == 127, 64-bit length
            if frame.len() < 14 {
                return String::new();
            }
            // For simplicity, we'll assume the length fits in a usize
            let len = ((frame[6] as usize) << 24)
                | ((frame[7] as usize) << 16)
                | ((frame[8] as usize) << 8)
                | (frame[9] as usize);
            (len, 14)
        };

        if frame.len() < payload_start + actual_length {
            return String::new();
        }

        let mask_key = &frame[payload_start - 4..payload_start];
        let payload = &frame[payload_start..payload_start + actual_length];

        let decoded: Vec<u8> = payload
            .iter()
            .enumerate()
            .map(|(i, byte)| byte ^ mask_key[i % 4])
            .collect();

        String::from_utf8_lossy(&decoded).to_string()
    }

    /// Extract the WebSocket key from the HTTP request
    fn extract_websocket_key(&self, request: &str) -> Option<String> {
        request
            .lines()
            .find(|line| line.starts_with("Sec-WebSocket-Key:"))
            .map(|line| line.split(": ").nth(1).unwrap().trim().to_string())
    }

    /// Generate the WebSocket accept key from the client key
    fn generate_accept_key(&self, key: &str) -> String {
        let magic_string = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
        let combined = format!("{}{}", key, magic_string);

        let mut hasher = Sha1::new();
        hasher.update(combined.as_bytes());
        let result = hasher.finalize();

        STANDARD.encode(result)
    }
}
