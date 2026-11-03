# Agent Harness Hooks: Complete Inventory & Validation Guide

Reference documentation for all hook scripts across the workspace agent harnesses, organized by system and event type.

**Note**: This is a reference table of existing hooks, NOT a hooklings configuration. Hooklings is a Git preflight check system (separate tool). Hook validation is per-system (see "Hook Validation by System" section below).

## Summary

- **Claude Code Hooks**: 9 scripts (PreToolUse, PostToolUse, SessionEnd)
- **Joe Project Hooks**: 6 scripts (SessionStart, PostToolUse, Cron)
- **CRS Rule Engine**: Rule config (pre-tool-use, post-tool-use)
- **Godmode Plugin**: Auto-discovered hooks

**Total Hooks**: 15+ active hooks + rule engine

---

## Claude Code Hooks

**Location**: `/Users/joe/.claude/hooks/`  
**Wiring**: `~/.claude/settings.json` → `hooks` section

| Hook                            | Event Type           | Purpose                                                                     | Status     | Test Coverage |
| ------------------------------- | -------------------- | --------------------------------------------------------------------------- | ---------- | ------------- |
| `enforce-autojoe-delegation.nu` | PreToolUse           | Block direct `cargo clippy`/`test`/`check`/`deny` runs; delegate to autojoe | ✓ Working  | ✓ 5 fixtures  |
| `scope-disambiguation-gate.nu`  | PreToolUse           | Require explicit project path for workspace CLAUDE.md edits                 | ✓ Fixed    | ✓ 4 fixtures  |
| `op-conflict-resolver.py`       | PreToolUse           | Detect environment variables shadowing 1Password secret injection           | ✓ Working  | ✓ 2 fixtures  |
| `godmode-trace.nu`              | PostToolUse          | Record tool.use events to `.ctx/godmode/traces/trace.jsonl`                 | ✓ Working  | ✓ 1 fixture   |
| `rust-gate-pre-commit.nu`       | PreToolUse (via crs) | Run `cargo xtask pre-commit` gate before commits                            | ⚠️ Complex | —             |
| `session-archive.nu`            | SessionEnd           | Archive Claude Code session logs and context                                | ⚠️ Complex | —             |
| `session-cost-ledger.mjs`       | SessionEnd           | Record session token usage and costs                                        | ✓ Working  | —             |
| `herdr-agent-state.sh`          | PostToolUse          | Herald agent state synchronization (cross-project)                          | ⚠️ Complex | —             |
| `insights-daily-nudge.sh`       | SessionStart         | Daily prompt for insights/usage report generation                           | ⚠️ Complex | —             |

### Claude Code Hook Events

**PreToolUse**: Fires before tool execution

- `enforce-autojoe-delegation.nu` — Matcher: `Bash`
- `scope-disambiguation-gate.nu` — Matcher: `Edit|Write`
- `op-conflict-resolver.py` — Matcher: `*`
- `crs hook pre-tool-use` — Matcher: `*` (via `~/.cargo/bin/crs`)

**PostToolUse**: Fires after tool completion

- `godmode-trace.nu` — Matcher: `*`
- `crs hook post-tool-use` — Matcher: `*`
- `herdr-agent-state.sh` — Matcher: `*` (experimental)
- `session-cost-ledger.mjs` — Matcher: `*`

**SessionEnd**: Fires when Claude Code session ends

- `session-archive.nu`
- `session-cost-ledger.mjs`

**SessionStart**: Fires when Claude Code session begins

- `insights-daily-nudge.sh`

---

## Joe Project Hooks

**Location**: `/Users/joe/dev/joe/hooks/`  
**Wiring**: `~/.claude/settings.json` → `hooks` section (SessionStart/PostToolUse)  
**Cron**: `CronCreate` durable job `62085f15` (daily 6:23am)

| Hook                   | Event Type   | Purpose                                                   | Status    | Frequency         |
| ---------------------- | ------------ | --------------------------------------------------------- | --------- | ----------------- |
| `session-refresh.nu`   | SessionStart | Full state refresh on session start                       | ✓ Working | Per session       |
| `post-skill-update.nu` | PostToolUse  | Partial state refresh after skill invocation              | ✓ Working | Per skill call    |
| `cron-refresh.nu`      | Cron         | Full state refresh (daily)                                | ✓ Working | Daily 6:23am      |
| `state-lib.nu`         | Helper       | State file schema & validation library                    | ✓ Working | Imported by hooks |
| `refresh-lib.nu`       | Helper       | State refresh logic (repo scan, GitHub scan, artifacts)   | ✓ Working | Imported by hooks |
| `trace-lib.nu`         | Helper       | Observability event emission (skill.start/complete/error) | ✓ Working | Imported by hooks |

### Joe Project Hook: State Management

**State File**: `joe/state.json` (populated by hooks + manual updates)  
**Schema**: `joe/state-schema.json` (JSON Schema draft-07, required fields)  
**Fields**:

- `repo_scan` — Local ~/dev/ repo metadata
- `gh_repos` — GitHub 89jobrien repo metadata
- `artifacts` — Generated artifact index
- `pieces` — Persona and recent activity
- `usage` — Session stats and work streams
- `meta` — Timestamps and last artifact path

---

## Course-Correct Rule Engine (crs)

**Binary**: `~/.cargo/bin/crs`  
**Config**:

- `~/.config/crs/plugins.d/*.toml` (rule definitions)
- `~/.config/coursers/course-correct-rules.json` (rule registry)

**Events**: PreToolUse, PostToolUse

| Component                | Purpose                                                          | Status    |
| ------------------------ | ---------------------------------------------------------------- | --------- |
| `crs hook pre-tool-use`  | RTK token optimization, guardrail enforcement, command rewriting | ✓ Working |
| `crs hook post-tool-use` | Output redaction, secrets filtering, failure tracking            | ✓ Working |
| `crs validate-hooks`     | Validate rule syntax                                             | ✓ Working |
| `crs log --limit N`      | Inspect recent hook activity                                     | ✓ Working |

---

## Godmode Plugin Hooks

**Location**: `~/.claude/plugins/local-marketplace/plugins/godmode/`  
**Config**: `hooks/hooks.json` (auto-discovered by Claude Code)

| Hook                  | Event Type          | Purpose                         | Status    |
| --------------------- | ------------------- | ------------------------------- | --------- |
| `pre-skill-trace.nu`  | PreToolUse (Skill)  | Log skill.start events          | ✓ Working |
| `post-skill-trace.nu` | PostToolUse (Skill) | Log skill.complete/error events | ✓ Working |

---

## Hook Validation by System

Each hook system has its own testing/validation approach. **Hooklings is NOT used for hook testing** — it's a Git preflight tool with built-in checks (git status, op auth, ssh reachability, etc.). Hook validation is separate and per-system.

### Claude Code Hooks: Fixture-Based Testing

**Method**: Dedicated test runner with fixture payloads  
**Location**: `/Users/joe/dev/.ctx/hooks/test-runner.nu` + fixtures  
**Coverage**: 12 fixtures across 4 hooks

```bash
# Test all Claude Code hooks
nu /Users/joe/dev/.ctx/hooks/test-runner.nu --verbose

# Test single hook
nu /Users/joe/dev/.ctx/hooks/test-runner.nu --hook enforce-autojoe-delegation --verbose
```

### Joe Project Hooks: State Schema Validation

**Method**: Manual state.json validation + schema conformance  
**Files**: `joe/state-schema.json` (JSON Schema draft-07) + `joe/state.json`

```bash
# Validate state schema
open /Users/joe/dev/joe/state-schema.json | type

# Check current state
open /Users/joe/dev/joe/state.json | keys
```

### CRS Rules: Built-In Validation Tool

**Method**: `crs validate-hooks` (crs built-in command)

```bash
# Validate rule syntax
crs validate-hooks

# Inspect recent hook activity
crs log --limit 20
```

### Godmode Hooks: Plugin Auto-Discovery

**Method**: Claude Code plugin system (auto-discovers `hooks.json`)  
Hooks auto-discovered when plugin is installed. No separate validation needed.

### Hooklings (Git Preflight Tool)

**What it does**: Runs pre-commit checks (git status, tool availability, 1Password auth, ssh reachability, handoff pending)  
**What it does NOT do**: Validate agent harness hooks  
**Usage**: `hooklings preflight` before commits  
**Config**: `~/.config/hooklings/default.crux` (Crux DSL)

---

## Hook Development Checklist

When adding a new hook:

- [ ] Create script at appropriate location (`.claude/hooks/`, `joe/hooks/`, etc.)
- [ ] Document in HOOK_CONTRACTS.md (input schema, output contract, block/allow conditions)
- [ ] Create fixtures with happy path, block cases, edge cases
- [ ] Register in hook config (settings.json or CLAUDE.md)
- [ ] Add to this inventory table
- [ ] Test in isolation with fixtures (not real tool calls)
- [ ] **Restart Claude Code** to load new settings
- [ ] Verify hook fires with test tool call

---

## References

- **Claude Code Hooks**: `/Users/joe/dev/.ctx/hooks/HOOK_CONTRACTS.md` (formal specs)
- **Hook Test Suite**: `/Users/joe/dev/.ctx/hooks/test-runner.nu` + `/Users/joe/dev/.ctx/hooks/README.md`
- **Joe Project**: `/Users/joe/dev/joe/CLAUDE.md` (hooks documentation)
- **CRS Rules**: `~/.config/coursers/course-correct-rules.json`
