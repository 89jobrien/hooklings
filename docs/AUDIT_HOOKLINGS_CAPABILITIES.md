# Hooklings Capability Audit Report

**Date**: 2026-10-21  
**Scope**: Reconcile agent harness hook documentation against hooklings actual capabilities  
**Status**: ✓ REMEDIATED

## Executive Summary

A comprehensive inventory of agent harness hooks (15+ scripts across Claude Code, Joe project, CRS, and Godmode) was created with the assumption that hooklings could validate them. **Audit revealed this is incorrect**: Hooklings is a Git preflight tool, not a hook test runner. Invalid TOML configs were deleted, documentation was corrected, and per-system validation methods were documented.

---

## What Hooklings Actually Is

**Hooklings**: Crux-based Git preflight check system  
**Commands**: `preflight` (run checks), `check <name>` (single check), `record`/`checkpoint` (track changes), `status` (checkpoint state), `config show` (print config)

**Built-in Checks** (6 total, provided by hooklings itself):

- `detect_shell` — Shell environment detection
- `check_tools` — Required tools availability
- `check_pwd` — Working directory validation
- `op::auth_check` — 1Password auth status
- `ssh::reachable` — SSH host reachability
- `handoff::pending` — Pending handoff items

Pipelines can also use `git::status` and `git::log`, but these are provided by
`crux-stdlib` (`crates/crux-stdlib/src/git.rs` in the `crux` repo), not by
hooklings — hooklings never registers a `git` handler of its own.

**Configuration**: Crux DSL (`.crux` files in `~/.config/hooklings/`)  
**Does NOT support**: TOML task runners, arbitrary hook testing, fixture runners

---

## Audit Findings

### Finding 1: Invalid Hooklings TOML Files

**What was created**:

- `/Users/joe/dev/hooklings.toml`
- `/Users/joe/dev/.ctx/hooks/hooklings.toml`

**Reality**: Hooklings doesn't use TOML task configs. It uses Crux DSL.

**Status**: ✓ **DELETED** both files

### Finding 2: Hooklings Can't Test Agent Harness Hooks

**What was claimed**: Hooklings validates all hook systems

**Reality**: Hooklings runs **Git preflight checks**, not hook testing. It has no mechanism for:

- Testing PreToolUse hook behavior
- Testing PostToolUse hook behavior
- Running fixture-based hook tests
- Validating hook contracts

**Status**: ✓ **CORRECTED** — Documentation now clarifies per-system validation

### Finding 3: Hook Inventory Table Is Accurate

**Status**: ✓ **KEPT** — The table accurately documents all 15+ hooks; only the scope framing was wrong

---

## Misalignments Corrected

| What Was Wrong                   | What It Should Be                                                  | Status         |
| -------------------------------- | ------------------------------------------------------------------ | -------------- |
| Hooklings validates all hooks    | Hooklings is Git preflight only; hooks are validated per-system    | ✓ Fixed        |
| hooklings.toml configs           | Hooklings uses Crux DSL (`.crux` files)                            | ✓ Deleted TOML |
| Claude Code hooks use hooklings  | Claude Code hooks use `.ctx/hooks/test-runner.nu` (fixture runner) | ✓ Documented   |
| Joe hooks validated by hooklings | Joe hooks validated by state.json schema conformance               | ✓ Documented   |
| CRS validated by hooklings       | CRS uses `crs validate-hooks` (built-in)                           | ✓ Documented   |
| Godmode validated by hooklings   | Godmode hooks auto-discovered; no test needed                      | ✓ Documented   |

---

## Accurate Hook Validation Methods

Each hook system has its own validation approach. Hooklings **is not involved** in any of them.

### Claude Code Hooks: Fixture-Based Testing

```bash
nu /Users/joe/dev/.ctx/hooks/test-runner.nu --verbose
```

**Fixtures**: 12 across 4 hooks (enforce-autojoe-delegation, scope-disambiguation-gate, op-conflict-resolver, godmode-trace)

### Joe Project Hooks: State Schema Validation

```bash
open /Users/joe/dev/joe/state-schema.json | type
open /Users/joe/dev/joe/state.json | keys
```

**Validation**: state.json must have 6 required fields

### CRS Rules: Built-In Validation

```bash
crs validate-hooks      # Validate rule syntax
crs log --limit 20      # Inspect recent activity
```

### Godmode Hooks: Plugin Auto-Discovery

No manual testing required. Hooks auto-discovered by Claude Code plugin system.

### Hooklings: Git Preflight (Separate Concern)

```bash
hooklings preflight     # Run pre-commit checks
```

**Purpose**: General git preflight, not hook testing

---

## What Hooklings Could Do (Future)

If hooklings were extended to support custom shell commands in Crux pipelines, it could orchestrate hook validation:

```crux
pipeline: agent_harness_validation

steps:
    # Standard preflight
    - step: detect_shell
      allow_failure: true

    # Hook validation (if hooklings supported custom commands)
    - step: claude_code_hooks
      command: "nu /Users/joe/dev/.ctx/hooks/test-runner.nu"
      allow_failure: false

    - step: crs_rules
      command: "crs validate-hooks"
      allow_failure: true
```

**Current state**: Not possible with existing hooklings; validation is per-system and manual

---

## Corrections Applied

### Files Deleted ✓

- ✓ `/Users/joe/dev/hooklings.toml`
- ✓ `/Users/joe/dev/.ctx/hooks/hooklings.toml`

### Files Updated ✓

- ✓ `hooklings-docs.md` — Clarified scope and validation methods
- ✓ `CLAUDE.md` — Updated hook validation instructions
- ✓ Created this audit report

### Files Created in Hooklings Repo ✓

- ✓ `/Users/joe/dev/hooklings/docs/AGENT_HARNESS_HOOKS.md` — Complete hook inventory
- ✓ `/Users/joe/dev/hooklings/docs/AUDIT_HOOKLINGS_CAPABILITIES.md` — This audit

---

## Status: REMEDIATED

**Before**: Documentation incorrectly suggested hooklings could validate agent harness hooks  
**After**: Documentation accurately describes:

1. Hooklings as Git preflight tool
2. Agent harness hooks as separate systems
3. Per-system validation methods for each

**Hook Validation Methods Now Documented**:

1. Claude Code → `nu .ctx/hooks/test-runner.nu` (fixtures)
2. Joe Project → state.json schema validation (manual)
3. CRS → `crs validate-hooks` (built-in)
4. Godmode → auto-discovered (no test)
5. Git Preflight → `hooklings preflight` (separate)

---

## References

- **Agent Harness Hooks**: `AGENT_HARNESS_HOOKS.md` (complete inventory)
- **Claude Code Hook Tests**: `/Users/joe/dev/.ctx/hooks/test-runner.nu` (fixture runner)
- **Claude Code Hook Specs**: `/Users/joe/dev/.ctx/hooks/HOOK_CONTRACTS.md` (formal contracts)
- **Joe Project State**: `/Users/joe/dev/joe/state-schema.json` (validation schema)
- **CRS Validation**: `crs validate-hooks` (built-in command)
