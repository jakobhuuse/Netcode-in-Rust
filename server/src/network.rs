use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::Sender,
        Arc, Mutex,
    },
    thread,
};

use crate::game::PlayerInputState;
use base64::{engine::general_purpose::STANDARD, Engine};
use sha1::{Digest, Sha1};

type ClientMap = Arc<Mutex<HashMap<usize, TcpStream>>>;

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

/// A simple WebSocket server implementation
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
            clients: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicUsize::new(0),
        }
    }

    /// Start the WebSocket server with a channel for game commands
    pub fn start(&self, cmd_sender: Sender<GameCommand>) -> Result<(), std::io::Error> {
        let listener = TcpListener::bind(&self.addr)?;
        println!("WebSocket server listening on ws://{}", self.addr);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let clients = Arc::clone(&self.clients);
                    let id = self.next_id.fetch_add(1, Ordering::Relaxed);

                    clients
                        .lock()
                        .unwrap()
                        .insert(id, stream.try_clone().unwrap());

                    let server_ref = self.clone_refs();
                    let cmd_sender_clone = cmd_sender.clone();
                    thread::spawn(move || {
                        server_ref.handle_client(stream, id, cmd_sender_clone);
                    });
                }
                Err(e) => {
                    eprintln!("Connection failed: {}", e);
                }
            }
        }
        Ok(())
    }

    /// Create a lightweight clone containing only reference-counted fields
    fn clone_refs(&self) -> Self {
        NetworkServer {
            addr: self.addr.clone(),
            clients: Arc::clone(&self.clients),
            next_id: AtomicUsize::new(self.next_id.load(Ordering::Relaxed)),
        }
    }

    /// Broadcast a message to all connected clients except the sender
    pub fn broadcast_message(&self, message: &str) {
        let frame = self.create_websocket_frame(message.to_string());
        let clients = self.clients.lock().unwrap();

        for (&id, stream) in clients.iter() {
            if let Err(e) = stream.try_clone().unwrap().write_all(&frame) {
                eprintln!("Failed to send to client {}: {}", id, e);
            }
        }
    }

    /// Send a message to a specific client by id
    pub fn send_message_to_client(&self, id: usize, message: &str) {
        let frame = self.create_websocket_frame(message.to_string());
        let clients = self.clients.lock().unwrap();
        if let Some(stream) = clients.get(&id) {
            // Try to clone the stream to avoid locking issues
            if let Ok(mut stream) = stream.try_clone() {
                if let Err(e) = stream.write_all(&frame) {
                    eprintln!("Failed to send to client {}: {}", id, e);
                }
            }
        }
    }

    /// Handle an individual client connection with a channel for game commands
    fn handle_client(&self, mut stream: TcpStream, id: usize, cmd_sender: Sender<GameCommand>) {
        let mut buffer = [0; 1024];

        if let Ok(size) = stream.read(&mut buffer) {
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

                stream.write_all(response.as_bytes()).unwrap();
                println!("Handshake completed for client {}!", id);

                let _ = cmd_sender.send(GameCommand::AddPlayer { id });

                loop {
                    let mut frame = [0; 1024];
                    match stream.read(&mut frame) {
                        Ok(size) if size == 0 => {
                            println!("Client {} disconnected abruptly.", id);
                            self.clients.lock().unwrap().remove(&id);
                            let _ = cmd_sender.send(GameCommand::RemovePlayer { id });
                            break;
                        }
                        Ok(size) => {
                            let opcode = frame[0] & 0x0F;

                            if opcode == 0x8 {
                                println!(
                                    "Client {} sent close frame. Sending close response...",
                                    id
                                );
                                self.clients.lock().unwrap().remove(&id);
                                let _ = cmd_sender.send(GameCommand::RemovePlayer { id });
                                let close_frame = vec![0x88, 0x00];
                                stream.write_all(&close_frame).unwrap();
                                break;
                            }

                            let message = self.parse_websocket_frame(&frame[..size]).to_lowercase();
                            println!("Received from client {}: {}", id, message);

                            let parts: Vec<&str> = message.split_whitespace().collect();
                            if parts.is_empty() {
                                continue;
                            }
                            match parts[0] {
                                "set_gravity" if parts.len() == 2 => {
                                    if let Ok(gravity) = parts[1].parse::<f32>() {
                                        let _ = cmd_sender
                                            .send(GameCommand::SetPlayerGravity { id, gravity });
                                    }
                                }
                                "set_max_speed" if parts.len() == 2 => {
                                    if let Ok(max_speed) = parts[1].parse::<f32>() {
                                        let _ = cmd_sender
                                            .send(GameCommand::SetPlayerMaxSpeed { id, max_speed });
                                    }
                                }
                                "set_acceleration_speed" if parts.len() == 2 => {
                                    if let Ok(acceleration_speed) = parts[1].parse::<f32>() {
                                        let _ = cmd_sender.send(
                                            GameCommand::SetPlayerAccelerationSpeed {
                                                id,
                                                acceleration_speed,
                                            },
                                        );
                                    }
                                }
                                "set_jump_speed" if parts.len() == 2 => {
                                    if let Ok(jump_speed) = parts[1].parse::<f32>() {
                                        let _ = cmd_sender.send(GameCommand::SetPlayerJumpSpeed {
                                            id,
                                            jump_speed,
                                        });
                                    }
                                }

                                _ if parts.iter().any(|p| p.contains('=')) => {
                                    if let Ok(input) = message.parse::<PlayerInputState>() {
                                        let _ =
                                            cmd_sender.send(GameCommand::PlayerInput { id, input });
                                    }
                                }
                                _ => {
                                    println!("Unknown command!")
                                }
                            }
                        }
                        Err(_) => {
                            println!("Error reading from client {}. Disconnecting...", id);
                            self.clients.lock().unwrap().remove(&id);
                            let _ = cmd_sender.send(GameCommand::RemovePlayer { id });
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Create a WebSocket frame from a message
    fn create_websocket_frame(&self, message: String) -> Vec<u8> {
        let mut frame = vec![0x81];
        let payload = message.as_bytes();

        if payload.len() < 126 {
            frame.push(payload.len() as u8);
        } else {
            frame.push(126);
            frame.push(((payload.len() >> 8) & 0xFF) as u8);
            frame.push((payload.len() & 0xFF) as u8);
        }

        frame.extend_from_slice(payload);
        frame
    }

    /// Parse a WebSocket frame into a string message
    fn parse_websocket_frame(&self, frame: &[u8]) -> String {
        let payload_length = (frame[1] & 127) as usize;
        let mask_key = &frame[2..6];
        let payload = &frame[6..6 + payload_length];

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
