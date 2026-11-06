#![no_main]

use libfuzzer_sys::fuzz_target;
use ring::signature::{Ed25519KeyPair, KeyPair};
use vpncloud::crypto::init::InitMsg;

// Deterministic key pair shared with examples/gen_seeds.rs so that seeded
// messages carry a signature from a trusted key and parsing reaches the
// part decoders instead of stopping at the trust lookup.
const SEED_PKCS8: &[u8] = &[48,81,2,1,1,48,5,6,3,43,101,112,4,34,4,32,188,109,245,165,127,101,157,47,155,219,44,133,230,161,24,52,215,144,197,74,71,13,239,64,179,83,225,24,69,25,235,99,129,33,0,44,59,72,78,167,190,83,51,105,29,1,108,133,37,69,47,14,127,150,193,166,15,17,11,150,229,126,134,88,86,239,212,];

fuzz_target!(|data: &[u8]| {
    let key_pair = Ed25519KeyPair::from_pkcs8(SEED_PKCS8).unwrap();
    let mut public_key = [0; 32];
    public_key.clone_from_slice(key_pair.public_key().as_ref());
    let _ = InitMsg::read_from(data, &[public_key]);
});
