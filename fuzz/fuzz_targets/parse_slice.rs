//! Whole-document parsing of arbitrary bytes must never panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = commerceml::from_slice(data);
    let _ = commerceml::ParseOptions::default().strict(true).parse_slice(data);
});
