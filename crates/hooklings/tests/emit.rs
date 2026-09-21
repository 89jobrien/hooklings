//! Integration tests for markdown and JSON result emission.

use hooklings::emit::{self, CheckResult, Emitter, Status};
use serde_json::json;

#[allow(dead_code)]
fn sample_results() -> Vec<CheckResult> {
    vec![
        CheckResult {
            name: "detect_shell".into(),
            status: Status::Pass,
            detail: "nu 0.102.0".into(),
            data: Some(json!({ "shell": "nu" })),
        },
        CheckResult {
            name: "check_tools".into(),
            status: Status::Warn,
            detail: "handoff-detect not found".into(),
            data: None,
        },
        CheckResult {
            name: "op::auth_check".into(),
            status: Status::Skip,
            detail: "disabled in config".into(),
            data: None,
        },
        CheckResult {
            name: "git::status".into(),
            status: Status::Fail,
            detail: "dirty tree".into(),
            data: None,
        },
    ]
}

#[test]
fn markdown_table_contains_all_check_names() {
    let table = emit::markdown_table(&sample_results());
    assert!(table.contains("detect_shell"));
    assert!(table.contains("check_tools"));
    assert!(table.contains("op::auth_check"));
    assert!(table.contains("git::status"));
}

#[test]
fn markdown_table_contains_status_labels() {
    let table = emit::markdown_table(&sample_results());
    assert!(table.contains("PASS"));
    assert!(table.contains("WARN"));
    assert!(table.contains("SKIP"));
    assert!(table.contains("FAIL"));
}

#[test]
fn markdown_table_has_header_row() {
    let table = emit::markdown_table(&sample_results());
    assert!(table.contains("| Check"));
    assert!(table.contains("| Status"));
    assert!(table.contains("| Detail"));
}

#[test]
fn json_output_contains_results_array() {
    let emitter = Emitter::new("preflight".into());
    let json = emitter.to_json(&sample_results());
    assert!(json["results"].is_array());
    assert_eq!(json["results"].as_array().unwrap().len(), 4);
    assert_eq!(json["pipeline"], "preflight");
    assert!(json["timestamp"].is_string());
}

#[test]
fn json_result_has_required_fields() {
    let emitter = Emitter::new("preflight".into());
    let json = emitter.to_json(&sample_results());
    let first = &json["results"][0];
    assert_eq!(first["name"], "detect_shell");
    assert_eq!(first["status"], "pass");
    assert!(first["detail"].is_string());
}

#[test]
fn write_json_creates_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.json");
    let emitter = Emitter::new("preflight".into());
    emitter.write_json(&sample_results(), &path).unwrap();
    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(parsed["results"].is_array());
}

#[test]
fn status_display() {
    assert_eq!(Status::Pass.as_str(), "pass");
    assert_eq!(Status::Warn.as_str(), "warn");
    assert_eq!(Status::Fail.as_str(), "fail");
    assert_eq!(Status::Skip.as_str(), "skip");
    assert_eq!(Status::Error.as_str(), "error");
}
