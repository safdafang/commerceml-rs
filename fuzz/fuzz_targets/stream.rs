//! Streaming over arbitrary bytes must never panic and must stop after an error.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let options = commerceml::ParseOptions::default().max_raw_size(256).max_warnings(16);
    let Ok(reader) = options.stream_reader(data) else { return };
    let mut failed = false;
    for event in reader {
        assert!(!failed, "events after an error");
        failed = event.is_err();
    }
});
