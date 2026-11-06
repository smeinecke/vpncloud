// Generates seed corpus files for the fuzz targets under fuzz/corpus/.

use std::{env, fs, str::FromStr, sync::Arc};

use ring::{
    aead::{AES_128_GCM, AES_256_GCM, CHACHA20_POLY1305},
    signature::{Ed25519KeyPair, KeyPair},
};

use smallvec::smallvec;
use vpncloud::{
    crypto::{core::CryptoCore, init::InitState, Algorithms, Payload},
    messages::NodeInfo,
    types::Range,
    util::MsgBuffer,
};

fn mk_info() -> NodeInfo {
    NodeInfo {
        node_id: [3; 16],
        peers: smallvec![],
        claims: smallvec![
            Range::from_str("10.0.0.0/8").unwrap(),
            Range::from_str("fd00::/64").unwrap()
        ],
        peer_timeout: Some(300),
        addrs: smallvec!["1.2.3.4:3210".parse().unwrap()],
    }
}

// Deterministic key pair shared with fuzz/fuzz_targets/init_msg.rs so that
// seeded init messages carry a signature from a trusted key.
const SEED_PKCS8: &[u8] = &[48,81,2,1,1,48,5,6,3,43,101,112,4,34,4,32,188,109,245,165,127,101,157,47,155,219,44,133,230,161,24,52,215,144,197,74,71,13,239,64,179,83,225,24,69,25,235,99,129,33,0,44,59,72,78,167,190,83,51,105,29,1,108,133,37,69,47,14,127,150,193,166,15,17,11,150,229,126,134,88,86,239,212,];

fn main() {
    let out_dir = env::args().nth(1).expect("usage: gen_seeds <dir>");
    fs::create_dir_all(&out_dir).expect("failed to create dir");

    let pkcs8 = SEED_PKCS8;
    let key_pair = Arc::new(Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap());
    let trusted = Arc::new([{
        let mut pk = [0; 32];
        pk.clone_from_slice(key_pair.public_key().as_ref());
        pk
    }]);
    let algorithms = Algorithms {
        algorithm_speeds: smallvec![(&AES_128_GCM, 600.0), (&AES_256_GCM, 500.0), (&CHACHA20_POLY1305, 400.0)],
        allow_unencrypted: true,
    };

    let mut state_a = InitState::new([1; 16], mk_info(), key_pair.clone(), trusted.clone(), algorithms.clone());
    let mut state_b = InitState::new([2; 16], mk_info(), key_pair, trusted, algorithms);

    let mut out = MsgBuffer::new(100);
    state_a.send_ping(&mut out).unwrap();
    fs::write(format!("{}/init_ping", out_dir), out.message()).unwrap();

    state_b.handle_init(&mut out).unwrap();
    fs::write(format!("{}/init_pong", out_dir), out.message()).unwrap();

    state_a.handle_init(&mut out).unwrap();
    fs::write(format!("{}/init_peng", out_dir), out.message()).unwrap();

    // Encrypted payload between the now-established peers
    let mut core = CryptoCore::new(
        ring::aead::LessSafeKey::new(ring::aead::UnboundKey::new(&AES_128_GCM, &[7u8; 16]).unwrap()),
        false,
    );
    out.buffer()[..16].copy_from_slice(b"\x01\x02\x03hello world\x00\x00");
    out.set_length(16);
    core.encrypt(&mut out).unwrap();
    fs::write(format!("{}/decrypt_msg", out_dir), out.message()).unwrap();

    // NodeInfo wire format
    let mut msg = MsgBuffer::new(100);
    mk_info().write_to(&mut msg).unwrap();
    fs::write(format!("{}/node_info", out_dir), msg.message()).unwrap();
}
