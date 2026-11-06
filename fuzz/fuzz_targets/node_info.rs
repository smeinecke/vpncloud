#![no_main]

use libfuzzer_sys::fuzz_target;
use vpncloud::messages::NodeInfo;

fuzz_target!(|data: &[u8]| {
    let _ = NodeInfo::decode(&mut &data[..]);
});
