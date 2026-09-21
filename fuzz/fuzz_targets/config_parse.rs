//! Fuzzes TOML configuration parsing with arbitrary UTF-8 input.

#![no_main]
use hooklings::config::Config;
use libfuzzer_sys::fuzz_target;
use std::io::Write;
use tempfile::NamedTempFile;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let mut f = match NamedTempFile::new() {
        Ok(f) => f,
        Err(_) => return,
    };
    let _ = f.write_all(s.as_bytes());
    // Must not panic regardless of TOML content
    let _ = Config::load_from_file(f.path());
});
