//! Smoke target that verifies the libFuzzer harness accepts arbitrary bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Input consumption alone exercises the minimal harness integration.
});
