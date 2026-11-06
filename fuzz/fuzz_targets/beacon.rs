#![no_main]

use libfuzzer_sys::fuzz_target;
use vpncloud::{beacon::BeaconSerializer, util::SystemTimeSource};

fuzz_target!(|data: &[u8]| {
    let ser = BeaconSerializer::<SystemTimeSource>::new(b"fuzzkey");
    let text = String::from_utf8_lossy(data);
    let _ = ser.decode(&text, None);
    let _ = ser.decode(&text, Some(u16::MAX));
});
