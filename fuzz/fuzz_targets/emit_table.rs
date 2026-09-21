//! Fuzzes markdown table emission with bounded arbitrary check results.

#![no_main]
use hooklings::emit::{self, CheckResult, Status};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let results: Vec<CheckResult> = s
        .lines()
        .take(32)
        .map(|line| {
            let mut parts = line.splitn(2, '|');
            CheckResult {
                name: parts.next().unwrap_or("x").chars().take(64).collect(),
                status: Status::Pass,
                detail: parts.next().unwrap_or("").chars().take(128).collect(),
                data: None,
            }
        })
        .collect();
    // Must not panic
    let _ = emit::markdown_table(&results);
});
