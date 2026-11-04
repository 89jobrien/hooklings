//! Regression coverage for the `coalesce`/`handoff` config-fallback fix in `main.rs`.
//!
//! Before the fix, a missing `.hooklings.toml` fell through to a hand-rolled fallback
//! TOML string containing a literal, unsubstituted `{HOME}` token and an unterminated
//! string literal in `[pipeline]`. `toml::from_str` on that string always failed, so
//! both the `coalesce` and `handoff` subcommands unconditionally hit their "could not
//! parse" error branch any time no config file existed -- real defaults were never
//! reachable. The fix checks `cfg_path.exists()` first and falls back to
//! `config::Config::default()` (which resolves `{HOME}` correctly at construction
//! time) instead of a broken literal. These tests run the compiled binary against a
//! repo-less temp directory with no `.hooklings.toml` and assert the real default
//! path is taken, not the former always-broken parse-error path.

use std::process::Command;

fn hooklings_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hooklings"))
}

#[test]
fn handoff_show_without_config_file_reports_real_defaults_not_a_parse_error() {
    let dir = tempfile::tempdir().expect("temp dir");

    let output = hooklings_cmd()
        .args(["handoff", "--show", "--repo"])
        .arg(dir.path())
        .output()
        .expect("run hooklings handoff --show");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "handoff --show should succeed with no config file present, got: {output:?}"
    );
    assert!(
        !stdout.contains("could not parse"),
        "missing .hooklings.toml must not be treated as a parse error, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("handoff_pending.default_limit = 20"),
        "expected the real HandoffConfig default (20) to be reported, got stdout: {stdout}"
    );
}

#[test]
fn coalesce_without_config_file_reports_real_mode_not_a_parse_error() {
    let dir = tempfile::tempdir().expect("temp dir");

    let output = hooklings_cmd()
        .args(["coalesce", "--mode", "auto", "--repo"])
        .arg(dir.path())
        .output()
        .expect("run hooklings coalesce --mode auto");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "coalesce should succeed with no config file present, got: {output:?}"
    );
    assert!(
        !stdout.contains("could not parse"),
        "missing .hooklings.toml must not be treated as a parse error, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("checkpoint coalescing mode: auto"),
        "expected the real auto-mode message (checkpoint disabled by default), got stdout: {stdout}"
    );
}
