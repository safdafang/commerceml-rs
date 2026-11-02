//! Anything that parses can be written, the output parses again, and writing
//! is a fixed point after the first normalization.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(first) = commerceml::from_slice(data) else { return };
    let out1 = commerceml::to_string(&first.info).expect("writing a parsed document");
    let second = commerceml::from_str(&out1).expect("written output must parse");
    let out2 = commerceml::to_string(&second.info).expect("writing again");
    assert_eq!(out1, out2, "writing is not a fixed point");
});
