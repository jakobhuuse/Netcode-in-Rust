use crate::game::{ClientGameState, ServerStateConfig};
use crate::input::InputManager;
use crate::rendering::{RenderConfig, Renderer};
use bincode::{deserialize, serialize};
use log::{error, info, warn};
use macroquad::prelude::*;
use shared::{InputState, Packet};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

pub struct Client {
    socket: UdpSocket,
    server_addr: SocketAddr,
    client_id: Option<u32>,
    connected: bool,

    game_state: ClientGameState,
    input_manager: InputManager,
    renderer: Renderer,

    ping_ms: u64,
    fake_ping_ms: u64,
    _last_ping_time: Instant,

    prediction_enabled: bool,
    reconciliation_enabled: bool,
    interpolation_enabled: bool,
}

impl Client {
    pub async fn new(
        server_addr: &str,
        fake_ping_ms: u64,
        width: usize,
        height: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        let server_addr = server_addr.parse()?;

        let renderer = Renderer::new(width, height)?;

        Ok(Client {
            socket,
            server_addr,
            client_id: None,
            connected: false,
            game_state: ClientGameState::new(),
            input_manager: InputManager::new(),
            renderer,
            ping_ms: 0,
            fake_ping_ms,
            _last_ping_time: Instant::now(),
            prediction_enabled: true,
            reconciliation_enabled: true,
            interpolation_enabled: true,
        })
    }

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Connecting to server...");

        let packet = Packet::Connect { client_version: 1 };
        self.send_packet(&packet).await?;

        Ok(())
    }

    async fn send_packet(&self, packet: &Packet) -> Result<(), Box<dyn std::error::Error>> {
        let data = serialize(packet)?;

        if self.fake_ping_ms > 0 {
            let delay_ms = self.fake_ping_ms / 2;
            let start = get_time();
            while (get_time() - start) < (delay_ms as f64 / 1000.0) {
                next_frame().await;
            }
        }

        self.socket.send_to(&data, self.server_addr)?;
        Ok(())
    }

    async fn handle_packet(&mut self, packet: Packet, _receive_time: Instant) {
        match packet {
            Packet::Connected { client_id } => {
                info!("Connected! Client ID: {}", client_id);
                self.client_id = Some(client_id);
                self.connected = true;
            }

            Packet::GameState {
                tick,
                timestamp,
                last_processed_input,
                players,
            } => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(Duration::from_secs(0))
                    .as_millis() as u64;

                if timestamp > 0 {
                    self.ping_ms = now.saturating_sub(timestamp);
                }

                let config = ServerStateConfig {
                    client_id: self.client_id,
                    reconciliation_enabled: self.reconciliation_enabled,
                    interpolation_enabled: self.interpolation_enabled,
                };

                self.game_state.apply_server_state(
                    tick,
                    timestamp,
                    players,
                    last_processed_input,
                    config,
                );
            }

            Packet::Disconnected { reason } => {
                warn!("Disconnected: {}", reason);
                self.connected = false;
                self.client_id = None;
            }

            _ => {
                warn!("Unexpected packet type");
            }
        }
    }

    async fn send_input(&mut self, input: InputState) -> Result<(), Box<dyn std::error::Error>> {
        if !self.connected || self.client_id.is_none() {
            return Ok(());
        }

        let packet = Packet::Input {
            sequence: input.sequence,
            timestamp: input.timestamp,
            left: input.left,
            right: input.right,
            jump: input.jump,
        };

        self.send_packet(&packet).await?;

        if self.prediction_enabled {
            if let Some(client_id) = self.client_id {
                self.game_state.apply_prediction(client_id, &input);
            }
        }

        Ok(())
    }

    fn handle_toggles(&mut self, toggles: (bool, bool, bool)) {
        if toggles.0 {
            self.prediction_enabled = !self.prediction_enabled;
            info!("Client-side prediction: {}", self.prediction_enabled);
        }
        if toggles.1 {
            self.reconciliation_enabled = !self.reconciliation_enabled;
            info!("Server reconciliation: {}", self.reconciliation_enabled);
        }
        if toggles.2 {
            self.interpolation_enabled = !self.interpolation_enabled;
            info!("Interpolation: {}", self.interpolation_enabled);
        }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.connect().await?;

        let mut last_input_time = Instant::now();
        let mut last_render_time = Instant::now();
        let input_interval = Duration::from_millis(16);
        let render_interval = Duration::from_millis(16);

        let mut buffer = [0u8; 2048];

        loop {
            match self.socket.recv_from(&mut buffer) {
                Ok((len, _)) => {
                    let receive_time = Instant::now();
                    if self.fake_ping_ms > 0 {
                        let delay_ms = self.fake_ping_ms / 2;
                        let start = get_time();
                        while (get_time() - start) < (delay_ms as f64 / 1000.0) {
                            next_frame().await;
                        }
                    }

                    if let Ok(packet) = deserialize::<Packet>(&buffer[0..len]) {
                        self.handle_packet(packet, receive_time).await;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    error!("Error receiving packet: {}", e);
                }
            }

            if last_input_time.elapsed() >= input_interval {
                let (toggles, input_to_send) = self.input_manager.update();

                self.handle_toggles(toggles);

                if let Some(input) = input_to_send {
                    if let Err(e) = self.send_input(input).await {
                        error!("Error sending input: {}", e);
                    }
                }
                last_input_time = Instant::now();
            }

            if last_render_time.elapsed() >= render_interval {
                if !self.prediction_enabled {
                    let dt = 1.0 / 60.0;
                    self.game_state.update_physics(dt);
                }

                let players = self.game_state.get_render_players(
                    self.client_id,
                    self.prediction_enabled,
                    self.interpolation_enabled,
                );

                let render_config = RenderConfig {
                    client_id: self.client_id,
                    prediction_enabled: self.prediction_enabled,
                    reconciliation_enabled: self.reconciliation_enabled,
                    interpolation_enabled: self.interpolation_enabled,
                    ping_ms: self.ping_ms,
                    fake_ping_ms: self.fake_ping_ms,
                };

                self.renderer.render(&players, render_config);

                last_render_time = Instant::now();
                next_frame().await;
            }

            if is_quit_requested() {
                break;
            }
        }

        if self.connected {
            let _ = self.send_packet(&Packet::Disconnect).await;
        }

        Ok(())
    }
}
