pub mod doob;
pub mod env;
pub mod handoff;
pub mod op;
pub mod ssh;

use cruxx_script::HandlerRegistry;

pub fn register_all(registry: &mut HandlerRegistry, config: &crate::config::Config) {
    env::register(registry);
    op::register(registry, config.checks.op_auth.enabled);
    ssh::register(
        registry,
        config.checks.ssh_reachable.enabled,
        &config.checks.ssh_reachable.host,
    );
    handoff::register(registry, &config.checks.handoff_pending.db);
    doob::register(registry, &config.checks.doob_pending.db);
}
