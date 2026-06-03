//! Emit preflight results as JSON to disk and a markdown table to stdout.
//!
//! [`Emitter`] handles JSON serialisation and file output. Use the free function
//! [`markdown_table`] for text rendering — it requires no instance state.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Warn,
    Fail,
    Skip,
    Error,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Warn => "warn",
            Status::Fail => "fail",
            Status::Skip => "skip",
            Status::Error => "error",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Status::Pass => "PASS",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
            Status::Skip => "SKIP",
            Status::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub status: Status,
    pub detail: String,
    pub data: Option<Value>,
}

/// Render a slice of [`CheckResult`] values as a markdown table string.
pub fn markdown_table(results: &[CheckResult]) -> String {
    let mut out = String::new();
    out.push_str("| Check | Status | Detail |\n");
    out.push_str("|---|---|---|\n");
    for r in results {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            r.name,
            r.status.label(),
            r.detail
        ));
    }
    out
}

pub struct Emitter {
    pipeline: String,
}

impl Emitter {
    pub fn new(pipeline: String) -> Self {
        Self { pipeline }
    }

    pub fn to_json(&self, results: &[CheckResult]) -> Value {
        let items: Vec<Value> = results
            .iter()
            .map(|r| {
                let mut obj = json!({
                    "name": r.name,
                    "status": r.status.as_str(),
                    "detail": r.detail,
                });
                if let Some(data) = &r.data {
                    obj["data"] = data.clone();
                }
                obj
            })
            .collect();

        json!({
            "timestamp": Utc::now().to_rfc3339(),
            "pipeline": self.pipeline,
            "results": items,
        })
    }

    pub fn write_json(&self, results: &[CheckResult], path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = self.to_json(results);
        let serialized = serde_json::to_string_pretty(&json)?;
        std::fs::write(path, serialized)
    }
}
