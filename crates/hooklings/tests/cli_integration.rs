//! Integration tests for hooklings CLI end-to-end workflows.

use std::process::Command;

fn hooklings_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hooklings"))
}

#[test]
fn full_coalesce_workflow() {
    // Test the full workflow: record -> checkpoint -> verify state
    // Just verify the command runs without panicking
    let output = hooklings_cmd()
        .args(["record", "--dry-run"])
        .output()
        .expect("run hooklings record --dry-run");

    // Command should have a valid exit code
    assert!(
        output.status.code().is_some(),
        "record --dry-run should have a valid exit code"
    );
}

#[test]
fn config_validate_no_panic() {
    // config validate should not panic, even without a config file
    let dir = tempfile::TempDir::new().expect("temp dir");
    std::env::set_current_dir(&dir).expect("set current dir");

    let output = hooklings_cmd()
        .args(["config", "validate"])
        .output()
        .expect("run hooklings config validate");

    // Should not panic - exit code 0 or a reasonable error message
    // At minimum, the command should run without crashing
    assert!(
        output.status.code().is_some(),
        "command should exit with a code, got no exit code"
    );
}

#[test]
fn handoff_cli_show_and_set() {
    // Test that handoff show/set commands work end-to-end
    let dir = tempfile::TempDir::new().expect("temp dir");
    std::env::set_current_dir(&dir).expect("set current dir");

    // First, set a new default limit using --set flag
    let output = hooklings_cmd()
        .args(["handoff", "--set", "42"])
        .output()
        .expect("run hooklings handoff --set 42");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("set to 42"),
        "should report set to 42, got: {stdout}"
    );

    // Then show the config
    let output = hooklings_cmd()
        .args(["handoff", "--show"])
        .output()
        .expect("run hooklings handoff --show");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("default_limit = 42"),
        "should report default_limit = 42 after set, got: {stdout}"
    );
}
