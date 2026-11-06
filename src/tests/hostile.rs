// VpnCloud - Peer-to-Peer VPN
// Copyright (C) 2015-2021  Dennis Schwerdel
// This software is licensed under GPL-3 or newer (see LICENSE.md)

use super::common::*;

const PAYLOAD: &[u8] = &[2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 2, 3, 4, 5];

fn connected_pair() -> (TapSimulator, std::net::SocketAddr, std::net::SocketAddr) {
    let config = Config { device_type: Type::Tap, ..Config::default() };
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);
    sim.connect(node1, node2);
    sim.simulate_all_messages();
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
    (sim, node1, node2)
}

#[test]
fn tampered_init_rejected() {
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.tamper_message(); // corrupt the init ping
    sim.simulate_all_messages();

    assert!(!sim.is_connected(node1, node2));
    assert!(!sim.is_connected(node2, node1));

    // The sender retries, so healthy progress is still possible afterwards
    sim.simulate_time(120);
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
}

#[test]
fn forged_garbage_rejected() {
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);
    let attacker = "255.255.255.255:1".parse().unwrap();

    sim.inject_message(attacker, node1, vec![0xde, 0xad, 0xbe, 0xef]);
    sim.inject_message(attacker, node1, vec![0u8; 65535]);
    sim.simulate_all_messages();

    assert!(!sim.is_connected(node1, attacker));
    assert!(!sim.is_connected(node2, attacker));

    // A legitimate connect still works afterwards
    sim.connect(node1, node2);
    sim.simulate_all_messages();
    assert!(sim.is_connected(node1, node2));
}

#[test]
fn tampered_payload_not_delivered() {
    let (mut sim, node1, node2) = connected_pair();

    sim.put_payload(node1, PAYLOAD.to_vec());
    sim.tamper_message(); // corrupt the encrypted payload message
    sim.simulate_all_messages();

    assert_eq!(None, sim.pop_payload(node2));

    // Subsequent intact traffic is still delivered
    sim.put_payload(node1, PAYLOAD.to_vec());
    sim.simulate_all_messages();
    assert_eq!(Some(PAYLOAD.to_vec()), sim.pop_payload(node2));
}

#[test]
fn replayed_payload_not_delivered_twice() {
    let (mut sim, node1, node2) = connected_pair();

    sim.put_payload(node1, PAYLOAD.to_vec());
    sim.duplicate_message(); // replay the exact same encrypted message
    sim.simulate_all_messages();

    assert_eq!(Some(PAYLOAD.to_vec()), sim.pop_payload(node2));
    assert_eq!(None, sim.pop_payload(node2));
}

#[test]
fn duplicated_init_still_connects() {
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.duplicate_message(); // duplicate the init ping
    sim.simulate_all_messages();

    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
}
