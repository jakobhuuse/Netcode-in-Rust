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