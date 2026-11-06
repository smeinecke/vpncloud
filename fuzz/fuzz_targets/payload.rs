#![no_main]

use libfuzzer_sys::fuzz_target;
use vpncloud::payload::{Frame, Packet, Protocol};

fuzz_target!(|data: &[u8]| {
    let _ = <Frame as Protocol>::parse(data);
    let _ = <Packet as Protocol>::parse(data);
});
