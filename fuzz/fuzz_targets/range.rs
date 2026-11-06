#![no_main]

use libfuzzer_sys::fuzz_target;
use vpncloud::types::{Address, Range};

fuzz_target!(|data: &[u8]| {
    let _ = Range::read_from(&mut &data[..]);
    let _ = Address::read_from(&mut &data[..]);
    if !data.is_empty() {
        let _ = Address::read_from_fixed(&mut &data[1..], data[0]);
    }
});
