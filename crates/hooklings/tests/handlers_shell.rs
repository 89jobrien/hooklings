//! Integration tests for the shell-backed `op::auth_check` and `ssh::reachable` handlers.
//!
//! Only the `enabled: false` skip path is exercised for `op::auth_check` — the enabled
//! path shells out to the real `op` CLI and its outcome depends on the host's 1Password
//! session state, which isn't something a test should assume. `ssh::reachable` is safe to
//! exercise enabled against a non-routable test address (RFC 5737 TEST-NET-3), which
//! fails fast without depending on any real host being configured.

use crux_script::HandlerRegistry;
use hooklings::handlers::{op, ssh};
use serde_json::json;

#[tokio::test]
async fn op_auth_check_disabled_is_skipped() {
    let mut reg = HandlerRegistry::new();
    op::register(&mut reg, false);

    let h = reg.get_handler("op::auth_check").unwrap().clone();
    let result = h(json!({})).await.unwrap();

    assert_eq!(result["status"], "skip");
}

#[tokio::test]
async fn ssh_reachable_disabled_is_skipped() {
    let mut reg = HandlerRegistry::new();
    ssh::register(&mut reg, false, "minibox");

    let h = reg.get_handler("ssh::reachable").unwrap().clone();
    let result = h(json!({})).await.unwrap();

    assert_eq!(result["status"], "skip");
}

#[tokio::test]
async fn ssh_reachable_enabled_warns_on_unreachable_host() {
    let mut reg = HandlerRegistry::new();
    // 203.0.113.1 is reserved for documentation (RFC 5737, TEST-NET-3) and never routes.
    ssh::register(&mut reg, true, "203.0.113.1");

    let h = reg.get_handler("ssh::reachable").unwrap().clone();
    let result = h(json!({})).await.unwrap();

    assert_eq!(result["status"], "warn");
    assert_eq!(result["reachable"], false);
    assert_eq!(result["host"], "203.0.113.1");
}
