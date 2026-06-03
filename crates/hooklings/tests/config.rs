use hooklings::config::{ChecksConfig, Config, OpAuthConfig, SshConfig};
use std::io::Write;
use tempfile::NamedTempFile;

#[allow(dead_code)]
fn write_toml(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

#[test]
fn default_config_has_expected_values() {
    let cfg = Config::default();
    assert!(!cfg.checks.op_auth.enabled);
    assert!(!cfg.checks.ssh_reachable.enabled);
    assert_eq!(cfg.checks.ssh_reachable.host, "minibox");
    assert!(cfg.emit.json_path.contains(".ctx"));
}

#[test]
fn load_from_toml_overrides_defaults() {
    let f = write_toml(
        r#"
[checks.op_auth]
enabled = true

[checks.ssh_reachable]
enabled = true
host = "my-vps"
"#,
    );
    let cfg = Config::load_from_file(f.path()).unwrap();
    assert!(cfg.checks.op_auth.enabled);
    assert!(cfg.checks.ssh_reachable.enabled);
    assert_eq!(cfg.checks.ssh_reachable.host, "my-vps");
}

#[test]
fn merge_project_over_global() {
    let global = write_toml(
        r#"
[checks.op_auth]
enabled = false

[checks.ssh_reachable]
host = "global-host"
"#,
    );
    let project = write_toml(
        r#"
[checks.op_auth]
enabled = true
"#,
    );
    let base = Config::load_from_file(global.path()).unwrap();
    let overlay = Config::load_from_file(project.path()).unwrap();
    let merged = base.merge(overlay);
    assert!(merged.checks.op_auth.enabled);
    // project didn't override ssh host — global value preserved
    assert_eq!(merged.checks.ssh_reachable.host, "global-host");
}

#[test]
fn invalid_toml_returns_error() {
    let f = write_toml("this is not toml ][[[");
    let result = Config::load_from_file(f.path());
    assert!(result.is_err());
}

#[test]
fn missing_file_returns_error() {
    let result = Config::load_from_file(std::path::Path::new("/nonexistent/path/hooklings.toml"));
    assert!(result.is_err());
}

#[test]
fn pipeline_path_override() {
    let f = write_toml(
        r#"
[pipeline]
default = "/tmp/my.crux"
"#,
    );
    let cfg = Config::load_from_file(f.path()).unwrap();
    assert_eq!(cfg.pipeline.default, "/tmp/my.crux");
}

// Task 3: property tests
use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(128))]

    #[test]
    fn load_never_panics_on_any_toml(content in "[^\x00]{0,512}") {
        let mut f = NamedTempFile::new().unwrap();
        let _ = f.write_all(content.as_bytes());
        let _ = Config::load_from_file(f.path());
    }

    #[test]
    fn merge_never_panics(
        op_enabled_a in proptest::bool::ANY,
        op_enabled_b in proptest::bool::ANY,
        host_a in "[a-z]{1,20}",
        host_b in "[a-z]{1,20}",
    ) {
        let a = Config {
            checks: ChecksConfig {
                op_auth: OpAuthConfig { enabled: op_enabled_a },
                ssh_reachable: SshConfig { enabled: false, host: host_a },
                ..Default::default()
            },
            ..Default::default()
        };
        let b = Config {
            checks: ChecksConfig {
                op_auth: OpAuthConfig { enabled: op_enabled_b },
                ssh_reachable: SshConfig { enabled: false, host: host_b },
                ..Default::default()
            },
            ..Default::default()
        };
        let _ = a.merge(b);
    }

    #[test]
    fn merge_identical_is_identity(enabled in proptest::bool::ANY) {
        let a = Config {
            checks: ChecksConfig {
                op_auth: OpAuthConfig { enabled },
                ..Default::default()
            },
            ..Default::default()
        };
        let b = a.clone();
        let merged = a.clone().merge(b);
        prop_assert_eq!(merged.checks.op_auth.enabled, a.checks.op_auth.enabled);
    }
}
