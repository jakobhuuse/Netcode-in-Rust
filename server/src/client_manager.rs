use log::info;
use shared::InputState;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Client {
    pub id: u32,
    pub addr: SocketAddr,
    pub last_seen: Instant,
    pub last_processed_input: u32,
    pub pending_inputs: Vec<InputState>,
}

impl Client {
    pub fn new(id: u32, addr: SocketAddr) -> Self {
        Self {
            id,
            addr,
            last_seen: Instant::now(),
            last_processed_input: 0,
            pending_inputs: Vec::new(),
        }
    }

    pub fn add_input(&mut self, input: InputState) {
        self.last_seen = Instant::now();
        self.pending_inputs.push(input);
        self.pending_inputs.sort_by_key(|i| i.sequence);
    }

    pub fn get_next_input(&mut self) -> Option<InputState> {
        if let Some(input) = self.pending_inputs.first() {
            if input.sequence <= self.last_processed_input {
                return self.pending_inputs.remove(0).into();
            }

            if input.sequence == self.last_processed_input + 1 {
                let input = self.pending_inputs.remove(0);
                self.last_processed_input = input.sequence;
                return Some(input);
            }
        }
        None
    }

    pub fn is_timed_out(&self, timeout: Duration) -> bool {
        self.last_seen.elapsed() > timeout
    }
}

pub struct ClientManager {
    clients: HashMap<u32, Client>,
    next_client_id: u32,
    max_clients: usize,
}

impl ClientManager {
    pub fn new(max_clients: usize) -> Self {
        Self {
            clients: HashMap::new(),
            next_client_id: 1,
            max_clients,
        }
    }

    pub fn add_client(&mut self, addr: SocketAddr) -> Option<u32> {
        if self.clients.len() >= self.max_clients {
            return None;
        }

        let client_id = self.next_client_id;
        self.next_client_id += 1;

        let client = Client::new(client_id, addr);
        info!("Client {} connected from {}", client_id, addr);
        self.clients.insert(client_id, client);

        Some(client_id)
    }

    pub fn remove_client(&mut self, client_id: &u32) -> bool {
        if let Some(client) = self.clients.remove(client_id) {
            info!("Client {} disconnected", client.id);
            true
        } else {
            false
        }
    }

    pub fn find_client_by_addr(&self, addr: SocketAddr) -> Option<u32> {
        self.clients
            .iter()
            .find(|(_, client)| client.addr == addr)
            .map(|(id, _)| *id)
    }

    pub fn add_input(&mut self, client_id: u32, input: InputState) -> bool {
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.add_input(input);
            true
        } else {
            false
        }
    }

    pub fn process_inputs<F>(&mut self, mut apply_fn: F)
    where
        F: FnMut(u32, &InputState, f32),
    {
        let dt = 1.0 / 60.0;

        for (client_id, client) in &mut self.clients {
            while let Some(input) = client.get_next_input() {
                apply_fn(*client_id, &input, dt);
            }
        }
    }

    pub fn get_last_processed_inputs(&self) -> HashMap<u32, u32> {
        self.clients
            .iter()
            .map(|(id, client)| (*id, client.last_processed_input))
            .collect()
    }

    pub fn check_timeouts(&mut self) -> Vec<u32> {
        let timeout = Duration::from_secs(5);
        let timed_out: Vec<u32> = self
            .clients
            .iter()
            .filter(|(_, client)| client.is_timed_out(timeout))
            .map(|(id, _)| *id)
            .collect();

        for client_id in &timed_out {
            self.remove_client(client_id);
        }

        timed_out
    }

    pub fn get_client_addrs(&self) -> Vec<(u32, SocketAddr)> {
        self.clients
            .iter()
            .map(|(id, client)| (*id, client.addr))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.clients.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
}
