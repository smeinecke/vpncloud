#![no_main]

use libfuzzer_sys::fuzz_target;
use ring::aead::{LessSafeKey, UnboundKey, AES_128_GCM};
use vpncloud::{crypto::core::CryptoCore, util::MsgBuffer};

fuzz_target!(|data: &[u8]| {
    if data.len() > MsgBuffer::new(0).buffer().len() {
        return;
    }
    let key = LessSafeKey::new(UnboundKey::new(&AES_128_GCM, &[7u8; 16]).unwrap());
    let mut core = CryptoCore::new(key, false);
    let mut buffer = MsgBuffer::new(100);
    buffer.buffer()[..data.len()].copy_from_slice(data);
    buffer.set_length(data.len());
    let _ = core.decrypt(&mut buffer);
});
