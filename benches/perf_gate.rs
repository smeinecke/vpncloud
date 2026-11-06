// Performance regression gate. Measures key hot-path operations and
// compares against benches/perf_baseline.txt.
//
// Usage:
//   cargo run --release --bench perf_gate          # check vs baseline
//   PERF_UPDATE=1 cargo run --release --bench perf_gate   # rewrite baseline
//   PERF_TOLERANCE_PCT=80 ...                      # allowed regression %
//
// Exit code is 0 when all measurements are within tolerance of the
// baseline, 1 when any regressed beyond it.
#![allow(dead_code, unused_macros, unused_imports)]
#[macro_use]
extern crate serde;
#[macro_use]
extern crate log;

use smallvec::smallvec;
use ring::aead;

use std::env;
use std::fs;
use std::str::FromStr;
use std::time::{Duration, Instant};
use std::net::SocketAddr;
use std::collections::BTreeMap;
use std::path::Path;

include!(".code.rs");

pub use error::Error;
use util::{MockTimeSource, MsgBuffer};
use types::{Address, Range};
use table::ClaimTable;
use device::Type;
use config::Config;
use payload::{Packet, Frame, Protocol};
use crypto::core::{create_dummy_pair, EXTRA_LEN};
use tests::common::{TunSimulator, TapSimulator};

fn baseline_path() -> String {
    env::var("PERF_BASELINE")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/benches/perf_baseline.txt").to_string())
}
const SAMPLES: usize = 5;
const MIN_TIME: Duration = Duration::from_millis(100);

fn black_box<T>(v: T) -> T {
    std::hint::black_box(v)
}

/// Runs `f` for at least MIN_TIME and returns ns per iteration.
fn measure<F: FnMut()>(mut f: F) -> f64 {
    // calibrate iteration count
    let mut iters = 1usize;
    loop {
        let start = Instant::now();
        for _ in 0..iters {
            f();
        }
        if start.elapsed() >= MIN_TIME || iters > (1 << 30) {
            break;
        }
        iters *= 4;
    }
    // SAMPLES medians
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        for _ in 0..iters {
            f();
        }
        samples.push(start.elapsed().as_nanos() as f64 / iters as f64);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[SAMPLES / 2]
}

fn bench_crypto(algo: &'static aead::Algorithm) -> f64 {
    let mut buffer = MsgBuffer::new(EXTRA_LEN);
    buffer.set_length(1400);
    let (mut sender, mut receiver) = create_dummy_pair(algo);
    measure(|| {
        sender.encrypt(&mut buffer).unwrap();
        receiver.decrypt(&mut buffer).unwrap();
        black_box(&mut buffer);
    })
}

fn bench_lookup_warm() -> f64 {
    let mut table = ClaimTable::<MockTimeSource>::new(60, 60);
    let addr = Address::from_str("1.2.3.4").unwrap();
    table.cache(addr, SocketAddr::from_str("1.2.3.4:3210").unwrap());
    measure(|| {
        black_box(table.lookup(addr));
    })
}

fn bench_lookup_cold() -> f64 {
    let mut table = ClaimTable::<MockTimeSource>::new(60, 60);
    let addr = Address::from_str("1.2.3.4").unwrap();
    table.set_claims(SocketAddr::from_str("1.2.3.4:3210").unwrap(), smallvec![Range::from_str("1.2.3.4/32").unwrap()]);
    measure(|| {
        table.clear_cache();
        black_box(table.lookup(addr));
    })
}

fn bench_parse_ipv4() -> f64 {
    let data = [0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 192, 168, 1, 1, 192, 168, 1, 2];
    measure(|| {
        black_box(Packet::parse(black_box(&data)).unwrap());
    })
}

fn bench_parse_frame() -> f64 {
    let data = [6, 5, 4, 3, 2, 1, 1, 2, 3, 4, 5, 6, 1, 2, 3, 4, 5, 6, 7, 8];
    measure(|| {
        black_box(Frame::parse(black_box(&data)).unwrap());
    })
}

fn bench_full_tun() -> f64 {
    log::set_max_level(log::LevelFilter::Error);
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
    let mut sim = TunSimulator::new();
    let node1 = sim.add_node(false, &config1);
    let node2 = sim.add_node(false, &config2);
    sim.connect(node1, node2);
    sim.simulate_all_messages();
    let mut payload = vec![0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2];
    payload.append(&mut vec![0; 1400]);
    measure(|| {
        sim.put_payload(node1, black_box(payload.clone()));
        sim.simulate_all_messages();
        black_box(sim.pop_payload(node2));
    })
}

fn bench_full_tap() -> f64 {
    log::set_max_level(log::LevelFilter::Error);
    let config = Config { device_type: Type::Tap, ..Config::default() };
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);
    sim.connect(node1, node2);
    sim.simulate_all_messages();
    let mut payload = vec![2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 2, 3, 4, 5];
    payload.append(&mut vec![0; 1400]);
    measure(|| {
        sim.put_payload(node1, black_box(payload.clone()));
        sim.simulate_all_messages();
        black_box(sim.pop_payload(node2));
    })
}

fn main() {
    let mut results: BTreeMap<String, f64> = BTreeMap::new();
    results.insert("crypto_aes128".to_string(), bench_crypto(&aead::AES_128_GCM));
    results.insert("crypto_aes256".to_string(), bench_crypto(&aead::AES_256_GCM));
    results.insert("crypto_chacha20".to_string(), bench_crypto(&aead::CHACHA20_POLY1305));
    results.insert("parse_ipv4".to_string(), bench_parse_ipv4());
    results.insert("parse_ethernet".to_string(), bench_parse_frame());
    results.insert("table_lookup_warm".to_string(), bench_lookup_warm());
    results.insert("table_lookup_cold".to_string(), bench_lookup_cold());
    results.insert("full_tun_router".to_string(), bench_full_tun());
    results.insert("full_tap_switch".to_string(), bench_full_tap());

    let update = env::var("PERF_UPDATE").is_ok();
    let tolerance: f64 = env::var("PERF_TOLERANCE_PCT").ok().and_then(|v| v.parse().ok()).unwrap_or(60.0);

    if update {
        let mut out = String::new();
        for (name, ns) in &results {
            out.push_str(&format!("{} {:.1}\n", name, ns));
        }
        fs::write(baseline_path(), &out).expect("cannot write baseline");
        println!("baseline updated:\n{}", out);
        return;
    }

    let baseline_text = match fs::read_to_string(baseline_path()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("no baseline at {}: {}", baseline_path(), e);
            eprintln!("run with PERF_UPDATE=1 to create it");
            std::process::exit(2);
        }
    };
    let mut baseline = BTreeMap::new();
    for line in baseline_text.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(name), Some(val)) = (parts.next(), parts.next()) {
            if let Ok(v) = val.parse::<f64>() {
                baseline.insert(name.to_string(), v);
            }
        }
    }

    let mut failed = false;
    println!("{:>20} {:>12} {:>12} {:>8}", "benchmark", "base ns/op", "now ns/op", "change");
    for (name, now) in &results {
        match baseline.get(name) {
            Some(base) => {
                let change = (now - base) / base * 100.0;
                let mark = if change > tolerance { " REGRESSED" } else { "" };
                println!("{:>20} {:>12.1} {:>12.1} {:>+7.1}%{}", name, base, now, change, mark);
                if change > tolerance {
                    failed = true;
                }
            }
            None => println!("{:>20} {:>12} {:>12.1} (no baseline)", name, "-", now),
        }
    }
    if failed {
        eprintln!("\nPERF GATE FAILED: regression beyond {}%", tolerance);
        std::process::exit(1);
    }
    println!("\nperf gate passed (tolerance {}%)", tolerance);
}
