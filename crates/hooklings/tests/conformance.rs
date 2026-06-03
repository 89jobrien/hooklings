use crux_script::HandlerRegistry;
use hooklings::{config::Config, handlers};

#[allow(dead_code)]
fn full_registry() -> HandlerRegistry {
    let cfg = Config::default();
    let mut reg = HandlerRegistry::new();
    handlers::register_all(&mut reg, &cfg);
    reg
}

#[test]
fn all_hooklings_handlers_registered() {
    let reg = full_registry();
    let expected = [
        "detect_shell",
        "check_tools",
        "check_pwd",
        "op::auth_check",
        "ssh::reachable",
        "handoff::pending",
    ];
    for name in expected {
        assert!(
            reg.get_handler(name).is_some(),
            "handler not registered: {name}"
        );
    }
}

#[test]
fn crux_agentic_handlers_also_available() {
    let mut reg = HandlerRegistry::new();
    crux_agentic::register_all(&mut reg);
    let cfg = Config::default();
    handlers::register_all(&mut reg, &cfg);

    for name in &["git::status", "git::log", "sqlite::exec"] {
        assert!(
            reg.get_handler(name).is_some(),
            "cruxx-agentic handler missing: {name}"
        );
    }
}
