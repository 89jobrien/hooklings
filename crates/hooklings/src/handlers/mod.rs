//! Built-in handlers for environment, authentication, SSH, and handoff checks.

pub mod env;
pub mod handoff;
pub mod op;
pub mod ssh;

use crux_script::HandlerRegistry;

/// Register all hooklings handlers into the given registry.
pub fn register_all(registry: &mut HandlerRegistry, config: &crate::config::Config) {
    env::register(registry);
    op::register(registry, config.checks.op_auth.enabled);
    ssh::register(
        registry,
        config.checks.ssh_reachable.enabled,
        &config.checks.ssh_reachable.host,
    );
    handoff::register(registry, &config.checks.handoff_pending.db);
    // doob::pending is not registered here — pipelines use sqlite::query_many directly
    // against a user-configured SQLite DB path (see [sqlite.todos] in hooklings.toml).
}
