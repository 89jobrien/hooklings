//! `ssh::reachable` — check SSH reachability with a 3-second timeout.

use crux_runtime::prelude::CruxErr;
use crux_script::HandlerRegistry;
use serde_json::{Value, json};
use tokio::process::Command;

/// Registers the optional SSH reachability check for `host`.
pub fn register(registry: &mut HandlerRegistry, enabled: bool, host: &str) {
    let host = host.to_string();
    registry.handler_value("ssh::reachable", move |_input: Value| {
        let h = host.clone();
        async move {
            if !enabled {
                return Ok(json!({ "status": "skip", "detail": "disabled in config" }));
            }
            let out = Command::new("ssh")
                .args([
                    "-o",
                    "ConnectTimeout=3",
                    "-o",
                    "BatchMode=yes",
                    "-o",
                    "StrictHostKeyChecking=no",
                    &h,
                    "exit",
                ])
                .output()
                .await
                .map_err(|e| {
                    CruxErr::step_failed("ssh::reachable", format!("spawn failed: {e}"))
                })?;

            if out.status.success() {
                Ok(json!({ "status": "pass", "reachable": true, "host": h }))
            } else {
                Ok(json!({
                    "status": "warn",
                    "reachable": false,
                    "host": h,
                    "detail": String::from_utf8_lossy(&out.stderr).trim().to_string(),
                }))
            }
        }
    });
}
