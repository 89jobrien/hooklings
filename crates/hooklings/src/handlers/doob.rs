//! `doob::pending` — list pending todos via the `doob` CLI.
//!
//! doob uses SurrealDB (not SQLite), so we invoke the CLI rather than reading
//! the DB directly. If `doob` is not on PATH, this check is skipped.

use cruxx_core::prelude::CruxErr;
use cruxx_script::HandlerRegistry;
use serde_json::{Value, json};
use tokio::process::Command;

pub fn register(registry: &mut HandlerRegistry, _db_path: &str) {
    registry.handler_value("doob::pending", move |_input: Value| async move {
        // Skip gracefully if doob is not installed
        let which = Command::new("which").arg("doob").output().await;
        let available = which.map(|o| o.status.success()).unwrap_or(false);
        if !available {
            return Ok(json!({
                "status": "skip",
                "detail": "doob not on PATH",
                "todos": [],
                "count": 0,
            }));
        }

        let out = Command::new("doob")
            .args(["todo", "list", "--status", "pending", "--json"])
            .output()
            .await
            .map_err(|e| CruxErr::step_failed("doob::pending", format!("spawn: {e}")))?;

        if !out.status.success() {
            return Ok(json!({
                "status": "warn",
                "detail": String::from_utf8_lossy(&out.stderr).trim().to_string(),
                "todos": [],
                "count": 0,
            }));
        }

        let parsed: Value =
            serde_json::from_slice(&out.stdout).unwrap_or(json!({ "todos": [], "count": 0 }));

        let count = parsed.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
        Ok(json!({
            "todos": parsed.get("todos").cloned().unwrap_or(json!([])),
            "count": count,
            "detail": format!("{count} pending"),
        }))
    });
}
