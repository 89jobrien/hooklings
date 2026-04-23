//! Environment check handlers: detect_shell, check_tools, check_pwd.

use cruxx_core::prelude::CruxErr;
use cruxx_script::HandlerRegistry;
use serde_json::{Value, json};
use tokio::process::Command;

pub fn register(registry: &mut HandlerRegistry) {
    registry.handler_value("detect_shell", |_input: Value| async move {
        let shell_env = std::env::var("SHELL").unwrap_or_default();
        let candidates = ["nu", "zsh", "bash"];
        let mut detected = shell_env.clone();
        let mut path = String::new();

        for candidate in candidates {
            if let Ok(out) = Command::new("which").arg(candidate).output().await {
                if out.status.success() {
                    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !p.is_empty() {
                        detected = candidate.to_string();
                        path = p;
                        break;
                    }
                }
            }
        }

        if path.is_empty() {
            path = shell_env.clone();
        }

        Ok(json!({ "shell": detected, "path": path, "SHELL": shell_env }))
    });

    registry.handler_value("check_tools", |input: Value| async move {
        let tools: Vec<String> = input
            .get("args")
            .and_then(|a| a.get("tools"))
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let mut results = vec![];
        for tool in tools {
            let out = Command::new("which").arg(&tool).output().await;
            let (status, location) = match out {
                Ok(o) if o.status.success() => {
                    let loc = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    ("pass", loc)
                }
                _ => ("warn", String::new()),
            };
            results.push(json!({
                "name": tool,
                "status": status,
                "path": location,
            }));
        }

        Ok(json!({ "tools": results }))
    });

    registry.handler_value("check_pwd", |_input: Value| async move {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let home = std::env::var("HOME").unwrap_or_default();
        let dev_dir = format!("{home}/dev");

        let (workspace, project) = if cwd.starts_with(&dev_dir) {
            let remainder = cwd[dev_dir.len()..].trim_start_matches('/');
            let parts: Vec<&str> = remainder.splitn(2, '/').collect();
            match parts.as_slice() {
                [ws] if !ws.is_empty() => (Some(ws.to_string()), None),
                [ws, proj] => (Some(ws.to_string()), Some(proj.to_string())),
                _ => (None, None),
            }
        } else {
            (None, None)
        };

        Ok::<Value, CruxErr>(json!({
            "cwd": cwd,
            "workspace": workspace,
            "project": project,
        }))
    });
}
