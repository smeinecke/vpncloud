// VpnCloud - Peer-to-Peer VPN
// Copyright (C) 2015-2021  Dennis Schwerdel
// This software is licensed under GPL-3 or newer (see LICENSE.md)

use super::common::*;

#[test]
fn switch_delivers() {
    let config = Config { device_type: Type::Tap, ..Config::default() };
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.simulate_all_messages();
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));

    let payload = vec![2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 2, 3, 4, 5];

    sim.put_payload(node1, payload.clone());
    sim.simulate_all_messages();

    assert_eq!(Some(payload), sim.pop_payload(node2));
}

#[test]
fn switch_learns() {
    let config = Config { device_type: Type::Tap, ..Config::default() };
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);
    let node3 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.connect(node1, node3);
    sim.connect(node2, node3);
    sim.simulate_all_messages();
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
    assert!(sim.is_connected(node1, node3));
    assert!(sim.is_connected(node3, node1));
    assert!(sim.is_connected(node2, node3));
    assert!(sim.is_connected(node3, node2));

    let payload = vec![2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 2, 3, 4, 5];

    // Nothing learnt so far, node1 broadcasts

    sim.put_payload(node1, payload.clone());
    sim.simulate_all_messages();

    assert_eq!(Some(payload.clone()), sim.pop_payload(node2));
    assert_eq!(Some(payload), sim.pop_payload(node3));

    let payload = vec![1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 5, 4, 3, 2, 1];

    // Node 2 learned the address by receiving it, does not broadcast

    sim.put_payload(node2, payload.clone());
    sim.simulate_all_messages();

    assert_eq!(Some(payload), sim.pop_payload(node1));
    assert_eq!(None, sim.pop_payload(node3));
}

#[test]
fn switch_honours_vlans() {
    let config = Config { device_type: Type::Tap, ..Config::default() };
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);
    let node3 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.connect(node1, node3);
    sim.connect(node2, node3);
    sim.simulate_all_messages();
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
    assert!(sim.is_connected(node1, node3));
    assert!(sim.is_connected(node3, node1));
    assert!(sim.is_connected(node2, node3));
    assert!(sim.is_connected(node3, node2));

    let payload = vec![2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 0x81, 0, 0, 0x67, 1, 2, 3, 4, 5];

    // Nothing learnt so far, node1 broadcasts

    sim.put_payload(node1, payload.clone());
    sim.simulate_all_messages();

    assert_eq!(Some(payload.clone()), sim.pop_payload(node2));
    assert_eq!(Some(payload), sim.pop_payload(node3));

    let payload = vec![1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 0x81, 0, 0, 0x67, 5, 4, 3, 2, 1];

    // Node 2 learned the address by receiving it, does not broadcast

    sim.put_payload(node2, payload.clone());
    sim.simulate_all_messages();

    assert_eq!(Some(payload), sim.pop_payload(node1));
    assert_eq!(None, sim.pop_payload(node3));

    let payload = vec![1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 0x81, 0, 0, 0x68, 5, 4, 3, 2, 1];

    // Different VLANs, node 2 does not learn, still broadcasts

    sim.put_payload(node2, payload.clone());
    sim.simulate_all_messages();

    assert_eq!(Some(payload.clone()), sim.pop_payload(node1));
    assert_eq!(Some(payload), sim.pop_payload(node3));
}

#[test]
fn switch_forgets() {
    // A learned MAC->peer entry must expire after the switch timeout so
    // that a moved or gone address falls back to broadcast
    let config = Config { device_type: Type::Tap, ..Config::default() };
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);
    let node3 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.connect(node1, node3);
    sim.simulate_all_messages();
    sim.simulate_time(120);

    // MAC_A behind node2 announces itself to everyone
    let mut hello = vec![0xff; 6];
    hello.extend_from_slice(&[0x02, 0, 0, 0, 0, 0xaa]);
    hello.extend_from_slice(&[0x08, 0x00, 1]);
    sim.put_payload(node2, hello.clone());
    sim.simulate_all_messages();
    assert_eq!(Some(hello.clone()), sim.pop_payload(node1));
    assert_eq!(Some(hello), sim.pop_payload(node3));

    // A reply to MAC_A is unicasted to node2 only
    let mut reply = vec![0x02, 0, 0, 0, 0, 0xaa];
    reply.extend_from_slice(&[0x02, 0, 0, 0, 0, 0xcc]);
    reply.extend_from_slice(&[0x08, 0x00, 2]);
    sim.put_payload(node1, reply.clone());
    sim.simulate_all_messages();
    assert_eq!(Some(reply.clone()), sim.pop_payload(node2));
    assert_eq!(None, sim.pop_payload(node3));

    // After the switch timeout the entry is forgotten and the same frame
    // is broadcast again
    sim.set_time(2000);
    sim.trigger_housekeep();
    sim.simulate_all_messages();
    sim.put_payload(node1, reply.clone());
    sim.simulate_all_messages();
    assert_eq!(Some(reply.clone()), sim.pop_payload(node2));
    assert_eq!(Some(reply), sim.pop_payload(node3));
}

#[test]
fn router_delivers() {
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
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));

    let payload = vec![0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2];

    sim.put_payload(node1, payload.clone());
    sim.simulate_all_messages();

    assert_eq!(Some(payload), sim.pop_payload(node2));
}

#[test]
fn router_drops_unknown_dest() {
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
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));

    let payload = vec![0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 3, 3, 3, 3];

    sim.put_payload(node1, payload);
    sim.simulate_all_messages();

    assert_eq!(None, sim.pop_payload(node2));
}
