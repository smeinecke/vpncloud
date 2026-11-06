#![no_main]

use libfuzzer_sys::fuzz_target;
use vpncloud::config::{Config, ConfigFile};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Ok(file) = serde_yaml::from_str::<ConfigFile>(&text) {
        let mut config = Config::default();
        config.merge_file(file);
    }
});
