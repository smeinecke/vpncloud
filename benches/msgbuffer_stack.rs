#![allow(dead_code, unused_macros, unused_imports)]
#[macro_use] extern crate serde;
#[macro_use] extern crate log;

use criterion::{criterion_group, criterion_main, Criterion, Throughput, BatchSize};

include!(".code.rs");

use std::net::SocketAddr;

pub use error::Error;
use util::{MockTimeSource, MsgBuffer};
use types::Address;
use device::Type;
use config::Config;
use payload::Packet;
use crypto::core::{create_dummy_pair, EXTRA_LEN};
use tests::common::TunSimulator;

/// Simulates deep call stack by recursing and creating local frame data.
/// At the deepest level, creates a MsgBuffer and performs crypto work
/// to stress test stack vs heap allocation strategies.
fn deep_stack_work<const FRAME_SIZE: usize>(
    depth: usize,
    payload: &[u8],
) -> Vec<u8> {
    if depth > 0 {
        // Simulate stack frame for crypto/parsing/routing work
        // Each frame consumes ~1KB of stack to model realistic call chains
        let stack_consumer: [u8; FRAME_SIZE] = [0u8; FRAME_SIZE];
        let result = deep_stack_work::<FRAME_SIZE>(depth - 1, payload);
        // Prevent optimizer from eliminating the array
        if stack_consumer[0] == 0 {
            result
        } else {
            result
        }
    } else {
        // At leaf: this is where MsgBuffer::new() gets called
        // This tests whether 64KB stack allocation causes issues
        let mut buffer = MsgBuffer::new(EXTRA_LEN);
        buffer.clone_from(payload);
        
        // Simulate crypto encryption/decrypt work
        let (mut sender, mut receiver) = create_dummy_pair(&ring::aead::AES_128_GCM);
        sender.encrypt(&mut buffer);
        receiver.decrypt(&mut buffer).unwrap();
        
        buffer.message().to_vec()
    }
}

/// Benchmark stack pressure with varying recursion depths
fn bench_msgbuffer_stack_pressure(c: &mut Criterion) {
    let mut g = c.benchmark_group("msgbuffer_stack_pressure");
    
    // 1400 byte payload = typical MTU-sized packet
    let payload: Vec<u8> = vec![0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2]
        .into_iter()
        .chain(std::iter::repeat(0).take(1380))
        .collect();
    
    g.throughput(Throughput::Bytes(1400));
    
    // Test different recursion depths to simulate call chain complexity
    // Each level adds FRAME_SIZE bytes to stack
    for depth in [16, 32, 64, 128] {
        g.bench_function(format!("depth_{}_frame_1k", depth), |b| {
            b.iter_batched(
                || payload.clone(),
                |p| deep_stack_work::<1024>(depth, &p),
                BatchSize::SmallInput,
            );
        });
    }
    
    // Test with larger frame size (2KB per level) - more aggressive stack pressure
    for depth in [32, 64] {
        g.bench_function(format!("depth_{}_frame_2k", depth), |b| {
            b.iter_batched(
                || payload.clone(),
                |p| deep_stack_work::<2048>(depth, &p),
                BatchSize::SmallInput,
            );
        });
    }
    
    g.finish();
}

/// Benchmark with realistic vpncloud packet flow through simulator
fn bench_full_path_stack_pressure(c: &mut Criterion) {
    let mut g = c.benchmark_group("msgbuffer_full_path");
    
    // Setup simulator once outside benchmark
    let mut sim = TunSimulator::new();
    let config1 = Config {
        device_type: Type::Tun,
        auto_claim: false,
        claims: vec!["1.1.1.1/32".to_string()],
        ..Config::default()
    };
    let config2 = Config {
        device_type: Type::Tun,
        auto_claim: false,
        claims: vec!["2.2.2.2/32".to_string()],
        ..Config::default()
    };
    
    let node1: SocketAddr = sim.add_node(false, &config1);
    let node2: SocketAddr = sim.add_node(false, &config2);
    sim.connect(node1, node2);
    sim.simulate_all_messages();
    
    let payload: Vec<u8> = vec![0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2]
        .into_iter()
        .chain(std::iter::repeat(0).take(1380))
        .collect();
    
    g.throughput(Throughput::Bytes(1400));
    
    // Baseline: single packet through established connection
    g.bench_function("baseline_single_packet", |b| {
        b.iter(|| {
            sim.put_payload(node1, payload.clone());
            sim.simulate_all_messages();
            assert!(sim.pop_payload(node2).is_some());
        });
    });
    
    g.finish();
}

/// Measure raw allocation cost of MsgBuffer::new()
fn bench_msgbuffer_creation(c: &mut Criterion) {
    let mut g = c.benchmark_group("msgbuffer_creation");
    
    // Benchmark creating and dropping MsgBuffers
    g.bench_function("create_and_drop", |b| {
        b.iter(|| {
            let _buffer = MsgBuffer::new(100);
        });
    });
    
    // Benchmark creating, writing to, and dropping
    let data = vec![0u8; 1400];
    g.bench_function("create_write_drop", |b| {
        b.iter(|| {
            let mut buffer = MsgBuffer::new(100);
            buffer.clone_from(&data);
            black_box(buffer.message());
        });
    });
    
    g.finish();
}

// Prevent optimizer from eliminating code
fn black_box<T>(dummy: T) -> T {
    unsafe {
        let ret = std::ptr::read_volatile(&dummy);
        std::mem::forget(dummy);
        ret
    }
}

criterion_group!(benches, 
    bench_msgbuffer_stack_pressure,
    bench_full_path_stack_pressure,
    bench_msgbuffer_creation
);
criterion_main!(benches);
