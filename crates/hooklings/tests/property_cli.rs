//! Property-based tests for hooklings CLI invariants.

//! These tests verify that CLI subcommands produce consistent
//! output regardless of input variations, using temp directories
//! and the compiled binary.

use std::process::Command;
use tempfile::TempDir;

fn hooklings_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hooklings"))
}

#[test]
fn handoff_show_always_reports_real_defaults_not_a_parse_error() {
    let dir = tempfile::tempdir().expect("temp dir");

    let output = hooklings_cmd()
        .args(["handoff", "--show", "--repo"])
        .arg(dir.path())
        .output()
        .expect("run hooklings handoff --show");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Command should succeed
    assert!(
        output.status.success(),
        "command should succeed, got: {output:?}"
    );

    // Never a parse error
    assert!(
        !stdout.contains("could not parse"),
        "should not be a parse error, got: {stdout}"
    );

    // Always reports the real default
    assert!(
        stdout.contains("handoff_pending.default_limit = 20"),
        "should report default_limit = 20, got: {stdout}"
    );
}

#[test]
fn coalesce_auto_always_reports_auto_mode_not_a_parse_error() {
    let dir = tempfile::tempdir().expect("temp dir");

    let output = hooklings_cmd()
        .args(["coalesce", "--mode", "auto", "--repo"])
        .arg(dir.path())
        .output()
        .expect("run hooklings coalesce --mode auto");

    // Command should succeed
    assert!(
        output.status.success(),
        "command should succeed, got: {output:?}"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Never a parse error
    assert!(
        !stdout.contains("could not parse"),
        "should not be a parse error, got: {stdout}"
    );

    // Always reports auto mode
    assert!(
        stdout.contains("checkpoint coalescing mode: auto"),
        "should report auto mode, got: {stdout}"
    );
}

#[test]
fn config_validate_handles_valid_toml() {
    // config validate reads from current directory's .hooklings.toml
    // Write valid config to temp dir and cd there
    let dir = tempfile::tempdir().expect("temp dir");
    std::env::set_current_dir(&dir).expect("set current dir");

    let valid_toml = "[checks]\nhandoff_pending = { db = \"/tmp/test.db\", default_limit = 20 }\n\n[checkpoint]\nenabled = false\nmax_files = 40\nmax_lines = 2000\n\n[pipeline]\ndefault = \"/tmp/test.crux\"\n\n[emit]\njson_path = \".ctx/last-preflight.json\"\n";

    std::fs::write(".hooklings.toml", valid_toml).expect("write valid toml");

    let output = hooklings_cmd()
        .args(["config", "validate"])
        .output()
        .expect("run hooklings config validate");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("valid"),
        "valid config should be reported as valid, got: {stdout}"
    );
}
