//! Performance benchmarks for critical game systems

use shared::{check_collision, resolve_collision, InputState, Player, GRAVITY, PLAYER_SPEED};
use std::time::Instant;

/// Benchmarks collision detection performance
#[test]
fn benchmark_collision_detection() {
    let player1 = Player::new(1, 100.0, 100.0);
    let player2 = Player::new(2, 110.0, 110.0);

    let iterations = 100_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let _ = check_collision(&player1, &player2);
    }

    let duration = start.elapsed();
    println!(
        "Collision detection: {} iterations in {:?} ({:.2} ns/iter)",
        iterations,
        duration,
        duration.as_nanos() as f64 / iterations as f64
    );

    // Should complete in under 100ms for 100k iterations
    assert!(duration.as_millis() < 100);
}

/// Benchmarks collision resolution performance
#[test]
fn benchmark_collision_resolution() {
    let iterations = 10_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let mut player1 = Player::new(1, 100.0, 100.0);
        let mut player2 = Player::new(2, 110.0, 110.0);

        player1.vel_x = PLAYER_SPEED;
        player2.vel_x = -PLAYER_SPEED;

        resolve_collision(&mut player1, &mut player2);
    }

    let duration = start.elapsed();
    println!(
        "Collision resolution: {} iterations in {:?} ({:.2} μs/iter)",
        iterations,
        duration,
        duration.as_micros() as f64 / iterations as f64
    );

    // Should complete in under 1 second
    assert!(duration.as_millis() < 1000);
}

/// Benchmarks physics simulation with multiple players
#[test]
fn benchmark_physics_simulation() {
    let mut players: Vec<Player> = (0..100)
        .map(|i| Player::new(i, (i as f32) * 10.0, 100.0))
        .collect();

    let dt = 1.0 / 60.0;
    let iterations = 1000;
    let start = Instant::now();

    for _ in 0..iterations {
        for player in &mut players {
            if !player.on_ground {
                player.vel_y += GRAVITY * dt;
            }

            player.x += player.vel_x * dt;
            player.y += player.vel_y * dt;

            // Simple floor collision
            if player.y + 32.0 >= 550.0 {
                player.y = 550.0 - 32.0;
                player.vel_y = 0.0;
                player.on_ground = true;
            }
        }
    }

    let duration = start.elapsed();
    println!(
        "Physics simulation: {} players × {} frames in {:?} ({:.2} μs/frame)",
        players.len(),
        iterations,
        duration,
        duration.as_micros() as f64 / iterations as f64
    );

    // Should complete in under 5 seconds
    assert!(duration.as_millis() < 5000);
}

/// Benchmarks network packet serialization performance
#[test]
fn benchmark_packet_serialization() {
    use bincode::{deserialize, serialize};
    use shared::Packet;
    use std::collections::HashMap;

    let mut last_processed_input = HashMap::new();
    for i in 0..50 {
        last_processed_input.insert(i, i * 10);
    }

    let players: Vec<Player> = (0..50)
        .map(|i| Player::new(i, (i as f32) * 10.0, 100.0))
        .collect();

    let packet = Packet::GameState {
        tick: 12345,
        timestamp: 1234567890,
        last_processed_input,
        players,
    };

    let iterations = 10_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let serialized = serialize(&packet).unwrap();
        let _deserialized: Packet = deserialize(&serialized).unwrap();
    }

    let duration = start.elapsed();
    println!(
        "Packet serialization: {} iterations in {:?} ({:.2} μs/iter)",
        iterations,
        duration,
        duration.as_micros() as f64 / iterations as f64
    );

    // Should complete in under 2 seconds
    assert!(duration.as_millis() < 2000);
}

/// Stress tests input processing under high load
#[test]
fn stress_test_many_inputs() {
    let inputs: Vec<InputState> = (0..1000)
        .map(|i| InputState {
            sequence: i,
            timestamp: i as u64 * 16,
            left: i % 3 == 0,
            right: i % 3 == 1,
            jump: i % 7 == 0,
        })
        .collect();

    let start = Instant::now();

    let mut sorted_inputs = inputs.clone();
    sorted_inputs.sort_by_key(|input| input.timestamp);

    // Verify sorting worked correctly
    for i in 1..sorted_inputs.len() {
        assert!(sorted_inputs[i].timestamp >= sorted_inputs[i - 1].timestamp);
    }

    let duration = start.elapsed();
    println!(
        "Input processing: {} inputs in {:?}",
        inputs.len(),
        duration
    );

    // Should complete in under 100ms
    assert!(duration.as_millis() < 100);
}

/// Benchmarks client-side prediction performance
#[test]
fn benchmark_client_prediction() {
    use client::game::ClientGameState;
    use shared::InputState;

    let mut client_state = ClientGameState::new();
    client_state
        .predicted_state
        .players
        .insert(1, Player::new(1, 100.0, 100.0));

    let iterations = 1_000;
    let start = Instant::now();

    for i in 0..iterations {
        let input = InputState {
            sequence: i,
            timestamp: i as u64 * 16,
            left: i % 2 == 0,
            right: i % 3 == 0,
            jump: i % 7 == 0,
        };

        client_state.apply_prediction(1, &input);
    }

    let duration = start.elapsed();
    println!(
        "Client prediction: {} predictions in {:?} ({:.2} μs/prediction)",
        iterations,
        duration,
        duration.as_micros() as f64 / iterations as f64
    );

    // Should handle 1000 predictions in under 50ms
    assert!(duration.as_millis() < 50);
}

/// Benchmarks server input processing performance
#[test]
fn benchmark_server_input_processing() {
    use server::client_manager::ClientManager;
    use shared::InputState;

    let mut client_manager = ClientManager::new(50);

    // Add clients and inputs
    for i in 1..=10 {
        let addr = format!("127.0.0.1:{}", 8000 + i).parse().unwrap();
        client_manager.add_client(addr);

        // Add many inputs per client
        for j in 1..=100 {
            let input = InputState {
                sequence: j,
                timestamp: j as u64 * 16,
                left: j % 2 == 0,
                right: j % 3 == 0,
                jump: j % 5 == 0,
            };
            client_manager.add_input(i, input);
        }
    }

    let start = Instant::now();

    // Get chronological inputs (this is the expensive operation)
    let chronological_inputs = client_manager.get_chronological_inputs();

    let duration = start.elapsed();
    println!(
        "Input processing: {} inputs processed in {:?}",
        chronological_inputs.len(),
        duration
    );

    // Should process 1000 inputs in under 10ms
    assert!(duration.as_millis() < 10);
}

/// Benchmarks network graph update performance
#[test]
fn benchmark_network_graph_updates() {
    use client::network_graph::NetworkGraph;

    let mut network_graph = NetworkGraph::new();
    let iterations = 1_000;
    let start = Instant::now();

    for i in 0..iterations {
        let ping_ms = 50.0 + (i as f32 % 100.0); // Varying ping
        network_graph.record_packet_received(ping_ms);
    }

    let duration = start.elapsed();
    println!(
        "Network graph updates: {} updates in {:?} ({:.2} μs/update)",
        iterations,
        duration,
        duration.as_micros() as f64 / iterations as f64
    );

    // Should handle 1000 updates in under 10ms
    assert!(duration.as_millis() < 10);
}

/// Benchmarks large packet serialization/deserialization
#[test]
fn benchmark_large_packet_processing() {
    use bincode::{deserialize, serialize};
    use shared::Packet;
    use std::collections::HashMap;

    // Create large game state packet
    let mut players = Vec::new();
    let mut last_processed = HashMap::new();

    for i in 0..50 {
        // 50 players
        players.push(Player::new(i, i as f32 * 10.0, 100.0));
        last_processed.insert(i, i * 10);
    }

    let packet = Packet::GameState {
        tick: 12345,
        timestamp: 1234567890,
        last_processed_input: last_processed,
        players,
    };

    let iterations = 1_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let serialized = serialize(&packet).unwrap();
        let _deserialized: Packet = deserialize(&serialized).unwrap();
    }

    let duration = start.elapsed();
    println!(
        "Large packet processing: {} roundtrips in {:?} ({:.2} μs/roundtrip)",
        iterations,
        duration,
        duration.as_micros() as f64 / iterations as f64
    );

    // Should handle 1000 large packet roundtrips in under 100ms
    assert!(duration.as_millis() < 100);
}

/// Benchmarks reconciliation performance under load
#[test]
fn benchmark_reconciliation_performance() {
    use client::game::{ClientGameState, ServerStateConfig};
    use std::collections::HashMap;

    let mut client_state = ClientGameState::new();

    // Set up state with input history
    client_state
        .predicted_state
        .players
        .insert(1, Player::new(1, 100.0, 100.0));

    // Add input history
    for i in 1..=100 {
        let input = InputState {
            sequence: i,
            timestamp: i as u64 * 16,
            left: i % 2 == 0,
            right: i % 3 == 0,
            jump: i % 7 == 0,
        };
        client_state.input_history.push(input);
    }

    let config = ServerStateConfig {
        client_id: Some(1),
        reconciliation_enabled: true,
        interpolation_enabled: false,
    };

    let iterations = 100;
    let start = Instant::now();

    for _ in 0..iterations {
        let players = vec![Player::new(1, 150.0, 100.0)]; // Different position to trigger reconciliation
        let mut last_processed = HashMap::new();
        last_processed.insert(1u32, 50u32); // Half the inputs processed

        client_state.apply_server_state(100, 12345, players, last_processed, config.clone());
    }

    let duration = start.elapsed();
    println!(
        "Reconciliation: {} reconciliations in {:?} ({:.2} ms/reconciliation)",
        iterations,
        duration,
        duration.as_millis() as f64 / iterations as f64
    );

    // Should handle 100 reconciliations in under 50ms
    assert!(duration.as_millis() < 50);
}
