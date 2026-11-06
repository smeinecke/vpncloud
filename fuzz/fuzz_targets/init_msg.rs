#![no_main]

use libfuzzer_sys::fuzz_target;
use vpncloud::crypto::init::InitMsg;

fuzz_target!(|data: &[u8]| {
    let _ = InitMsg::read_from(data, &[]);
});
