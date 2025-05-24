use crate::game::ClientGameState;
use crate::input::InputManager;
use crate::rendering::Renderer;
use bincode::{deserialize, serialize};
use log::{error, info, warn};
use shared::{InputState, Packet};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::{interval, sleep};

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
    last_ping_time: Instant,

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
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
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
            last_ping_time: Instant::now(),
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
        if self.fake_ping_ms > 0 {
            sleep(Duration::from_millis(self.fake_ping_ms / 2)).await;
        }

        let data = serialize(packet)?;
        self.socket.send_to(&data, self.server_addr).await?;
        Ok(())
    }

    async fn handle_packet(&mut self, packet: Packet, receive_time: Instant) {
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

                self.game_state.apply_server_state(
                    tick,
                    timestamp,
                    players,
                    last_processed_input,
                    self.client_id,
                    self.reconciliation_enabled,
                    self.interpolation_enabled,
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

        let mut input_interval = interval(Duration::from_millis(16));
        let mut physics_interval = interval(Duration::from_millis(16));
        let mut render_interval = interval(Duration::from_millis(16));

        let mut buffer = [0u8; 2048];

        while self.renderer.is_open() {
            tokio::select! {
                result = self.socket.recv_from(&mut buffer) => {
                    let receive_time = Instant::now();
                    match result {
                        Ok((len, _)) => {
                            if self.fake_ping_ms > 0 {
                                sleep(Duration::from_millis(self.fake_ping_ms / 2)).await;
                            }

                            if let Ok(packet) = deserialize::<Packet>(&buffer[0..len]) {
                                self.handle_packet(packet, receive_time).await;
                            }
                        },
                        Err(e) => error!("Error receiving packet: {}", e),
                    }
                },

                _ = input_interval.tick() => {
                    let (toggles, input_to_send) = self.input_manager.update(&self.renderer.window);

                    self.handle_toggles(toggles);

                    if let Some(input) = input_to_send {
                        if let Err(e) = self.send_input(input).await {
                            error!("Error sending input: {}", e);
                        }
                    }
                },

                _ = physics_interval.tick() => {
                    let dt = 1.0 / 60.0;
                    self.game_state.update_physics(dt);
                },

                _ = render_interval.tick() => {
                    let players = self.game_state.get_render_players(
                        self.client_id,
                        self.interpolation_enabled,
                    );

                    self.renderer.render(
                        &players,
                        self.client_id,
                        self.prediction_enabled,
                        self.reconciliation_enabled,
                        self.interpolation_enabled,
                        self.ping_ms,
                        self.fake_ping_ms,
                    );
                },
            }
        }

        if self.connected {
            let _ = self.send_packet(&Packet::Disconnect).await;
        }

        Ok(())
    }
}
