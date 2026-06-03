//! `op::auth_check` — verify 1Password CLI is authenticated.

use crux_runtime::prelude::CruxErr;
use crux_script::HandlerRegistry;
use serde_json::{Value, json};
use tokio::process::Command;

pub fn register(registry: &mut HandlerRegistry, enabled: bool) {
    registry.handler_value("op::auth_check", move |_input: Value| async move {
        if !enabled {
            return Ok(json!({ "status": "skip", "detail": "disabled in config" }));
        }
        let out = Command::new("op")
            .args(["account", "list", "--format=json"])
            .output()
            .await
            .map_err(|e| CruxErr::step_failed("op::auth_check", format!("spawn failed: {e}")))?;

        if out.status.success() {
            let accounts: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
            Ok(json!({
                "status": "pass",
                "authenticated": true,
                "accounts": accounts.len(),
            }))
        } else {
            Ok(json!({
                "status": "fail",
                "authenticated": false,
                "detail": String::from_utf8_lossy(&out.stderr).trim().to_string(),
            }))
        }
    });
}
