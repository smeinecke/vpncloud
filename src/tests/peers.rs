// VpnCloud - Peer-to-Peer VPN
// Copyright (C) 2015-2021  Dennis Schwerdel
// This software is licensed under GPL-3 or newer (see LICENSE.md)

use super::common::*;

#[test]
fn direct_connect() {
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.simulate_all_messages();
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
}

#[test]
fn direct_connect_unencrypted() {
    let config = Config {
        crypto: CryptoConfig { algorithms: vec!["plain".to_string()], ..CryptoConfig::default() },
        ..Config::default()
    };
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.simulate_all_messages();
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
}

#[test]
fn cross_connect() {
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);
    let node3 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.connect(node1, node3);
    sim.simulate_all_messages();

    sim.simulate_time(120);

    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
    assert!(sim.is_connected(node1, node3));
    assert!(sim.is_connected(node3, node1));
    assert!(sim.is_connected(node2, node3));
    assert!(sim.is_connected(node3, node2));
}

#[test]
fn connect_via_beacons() {
    let mut sim = TapSimulator::new();
    let beacon_path = "target/.vpncloud_test";
    let config1 = Config { beacon_store: Some(beacon_path.to_string()), ..Default::default() };
    let node1 = sim.add_node(false, &config1);
    let config2 = Config { beacon_load: Some(beacon_path.to_string()), ..Default::default() };
    let node2 = sim.add_node(false, &config2);

    sim.set_time(100);
    sim.trigger_node_housekeep(node1);
    sim.trigger_node_housekeep(node2);
    sim.simulate_all_messages();

    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
}

#[test]
fn reconnect_after_timeout() {
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.simulate_all_messages();
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));

    sim.set_time(5000);
    sim.trigger_housekeep();
    assert!(!sim.is_connected(node1, node2));
    assert!(!sim.is_connected(node2, node1));

    sim.simulate_all_messages();
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
}

#[test]
fn lost_init_ping() {
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.drop_message(); // drop init ping

    sim.simulate_time(120);
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
}

#[test]
fn lost_init_pong() {
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.simulate_next_message(); // init ping
    sim.drop_message(); // drop init pong

    sim.simulate_time(120);
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
}

#[test]
fn lost_init_peng() {
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.simulate_next_message(); // init ping
    sim.simulate_next_message(); // init pong
    sim.drop_message(); // drop init peng

    sim.simulate_time(120);
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
}

#[test]
fn peer_exchange() {
    // node2 and node3 are never connected directly; they must discover
    // each other through the peer lists node1 sends in its node info
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);
    let node3 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.connect(node1, node3);
    sim.simulate_all_messages();
    sim.simulate_time(120);

    assert!(sim.is_connected(node2, node3));
    assert!(sim.is_connected(node3, node2));
}

#[test]
fn lost_peer_exchange() {
    // The peer lists are retransmitted periodically, so losing the first
    // exchange only delays discovery
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);
    let node3 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.connect(node1, node3);
    sim.simulate_all_messages();

    sim.trigger_housekeep();
    while sim.message_count() > 0 {
        sim.drop_message();
    }
    assert!(!sim.is_connected(node2, node3) || !sim.is_connected(node3, node2));

    sim.simulate_time(120);
    assert!(sim.is_connected(node2, node3));
    assert!(sim.is_connected(node3, node2));
}

#[test]
fn remove_dead_peers() {
    // node3 stops sending entirely; the live nodes must drop it while
    // keeping each other
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);
    let node3 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.connect(node1, node3);
    sim.simulate_all_messages();
    sim.simulate_time(120);
    assert!(sim.is_connected(node1, node3));

    // node3 dies; the live nodes must drop it while keeping each other
    sim.remove_node(node3);
    sim.simulate_time(500);
    assert!(!sim.is_connected(node1, node3));
    assert!(!sim.is_connected(node2, node3));
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
}

#[test]
#[ignore = "requires simulator support for rebinding a node's socket address"]
fn update_primary_address() {
    // TODO Test
    unimplemented!()
}

#[test]
fn automatic_peer_timeout() {
    // With no traffic at all, a node must drop a peer once its configured
    // peer timeout has passed - without waiting for any external event
    let config = Config::default();
    let mut sim = TapSimulator::new();
    let node1 = sim.add_node(false, &config);
    let node2 = sim.add_node(false, &config);

    sim.connect(node1, node2);
    sim.simulate_all_messages();
    assert!(sim.is_connected(node1, node2));

    sim.set_time(5000);
    // node1 housekeeps first and times node2 out; node2 still sees node1
    // until its own housekeep runs. The entries are dropped inside
    // housekeep, before the queued reconnect traffic is delivered.
    sim.trigger_node_housekeep(node1);
    assert!(!sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));

    sim.trigger_node_housekeep(node2);
    assert!(!sim.is_connected(node2, node1));

    // delivering the queued reconnect traffic re-establishes the link
    sim.simulate_all_messages();
    assert!(sim.is_connected(node1, node2));
    assert!(sim.is_connected(node2, node1));
}
