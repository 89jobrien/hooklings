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
hooklings record <path>...       # record paths an agent harness just touched
hooklings checkpoint             # commit everything recorded since the last one
hooklings status                 # show the recorded checkpoint state
```

## Checkpoint

Coalesced commits for agent-driven edits. A harness records each path it touches
while it works; nothing is committed at that point. At a turn boundary, one
checkpoint commit collects everything recorded since the last commit, so a
five-file refactor lands as one coherent commit instead of five broken ones.

```toml
# .hooklings.toml — opt-in per repo
[checkpoint]
enabled = true
```

```bash
hooklings record src/a.rs src/b.rs   # or: hooklings record --stdin < hook.json
hooklings checkpoint --dry-run       # preview without committing
hooklings checkpoint                 # commit
```

Commits carry an inferred Conventional Commit subject plus a per-file stat body:

```text
feat(src): add 2 files

Files:
  A src/alpha.rs  +1 -0
  M src/beta.rs   +8 -2

Coalesced checkpoint: 14 edit(s) since 2026-10-02T21:37:00Z.
```

Type inference is deliberately conservative. A single-file changeset classifies by
that file's role (tests, docs, manifests, lockfiles); all-added becomes `feat`,
all-removed or renamed becomes `refactor`. Everything else is `chore`. `fix` is
**never** inferred, because nothing in a diff distinguishes a repair from any other
edit — pass `--type fix --message "fix(parser): handle empty input"` when it matters.

### Guardrails

A checkpoint refuses rather than guesses:

| Refusal                                     | Why                                                                                                                                                                                                           |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `main` / `master` (or `protected_branches`) | `main` moves only by explicit merge                                                                                                                                                                           |
| Foreign staged paths                        | another writer already staged something; committing would sweep it in                                                                                                                                         |
| More than `max_files` / `max_lines`         | a checkpoint that big is a real commit, not a checkpoint                                                                                                                                                      |
| Executable files and symlinks               | gix's tree editor accepts an `EntryKind`, not a mode, so it can only write `0o100644`. Staging those through it would silently drop `+x` or commit a symlink as a regular file, so they are left to the agent |

It also drops paths that are gitignored, outside the worktree, or contain `..`, and
only ever stages the recorded paths — never `add -A`.

### Known limitation

The executable/symlink refusal is a real gap: editing a shell script in an
opted-in repo means that path never lands in a checkpoint. The fix belongs upstream
in gix (`Editor::upsert` taking a mode rather than an `EntryKind`), not in a
workaround here.

### Harness wiring

Driven by `crs` rules, not inline shell. `record --stdin` on `PostToolUse`
`Edit|Write`, and `checkpoint` on `stop` and `pre-compact`, defined alongside the
other rules in `~/.config/crs/plugins.d/session-lifecycle.toml`. All three are
inert for repos that have not set `[checkpoint] enabled = true`.

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

[checkpoint]
enabled = true
max_files = 40
max_lines = 2000
protected_branches = ["develop", "release"]
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
