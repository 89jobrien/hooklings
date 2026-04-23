use cruxx_script::HandlerRegistry;
use hooklings::handlers::env;
use serde_json::json;

fn registry() -> HandlerRegistry {
    let mut r = HandlerRegistry::new();
    env::register(&mut r);
    r
}

#[tokio::test]
async fn detect_shell_returns_shell_field() {
    let reg = registry();
    let h = reg.get_handler("detect_shell").unwrap().clone();
    let result = h(json!({})).await.unwrap();
    assert!(result["shell"].is_string());
    assert!(result["path"].is_string());
}

#[tokio::test]
async fn check_tools_pass_for_known_tools() {
    let reg = registry();
    let h = reg.get_handler("check_tools").unwrap().clone();
    let result = h(json!({"args": {"tools": ["cargo"]}})).await.unwrap();
    let tools = result["tools"].as_array().unwrap();
    assert!(!tools.is_empty());
    let cargo = tools.iter().find(|t| t["name"] == "cargo").unwrap();
    assert_eq!(cargo["status"], "pass");
}

#[tokio::test]
async fn check_tools_warn_for_missing_tools() {
    let reg = registry();
    let h = reg.get_handler("check_tools").unwrap().clone();
    let result = h(json!({"args": {"tools": ["this-tool-definitely-does-not-exist-xyz"]}}))
        .await
        .unwrap();
    let tools = result["tools"].as_array().unwrap();
    let missing = tools
        .iter()
        .find(|t| t["name"] == "this-tool-definitely-does-not-exist-xyz")
        .unwrap();
    assert_eq!(missing["status"], "warn");
}

#[tokio::test]
async fn check_tools_empty_list_returns_empty_array() {
    let reg = registry();
    let h = reg.get_handler("check_tools").unwrap().clone();
    let result = h(json!({"args": {"tools": []}})).await.unwrap();
    let tools = result["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 0);
}

#[tokio::test]
async fn check_pwd_returns_cwd_project_workspace() {
    let reg = registry();
    let h = reg.get_handler("check_pwd").unwrap().clone();
    let result = h(json!({})).await.unwrap();
    assert!(result["cwd"].is_string());
    assert!(result.get("project").is_some());
    assert!(result.get("workspace").is_some());
}
