# hooklings

YAML-driven developer preflight checks via [crux](https://github.com/89jobrien/crux) pipelines.

## Install

```bash
cargo install --path crates/hooklings
```

## Usage

```bash
hooklings preflight              # run all checks, emit JSON + markdown table
hooklings preflight --emit table # markdown table only
hooklings check detect_shell     # run a single check
hooklings config show            # print merged config
```

## Config

Global: `~/.config/hooklings/hooklings.toml`
Project: `.hooklings.toml` in repo root (merged over global)

```toml
[checks.op_auth]
enabled = true

[checks.ssh_reachable]
enabled = true
host = "minibox"

[checks.doob_pending]
db = "~/.local/share/doob/doob.db"
```

## Pipeline

Checks are defined as `.crux` YAML pipelines. The default pipeline runs at:
`~/.config/hooklings/default.crux`

Override per-project:

```toml
[pipeline]
default = ".hooklings/ci.crux"
```

## atelier Integration

atelier's SessionStart hook calls `hooklings preflight --emit both` when hooklings is on PATH.
