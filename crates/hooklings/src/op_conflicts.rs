//! Detects environment variables that would shadow `op run`'s 1Password
//! secret injection.
//!
//! Ported from `~/.claude/hooks/op-conflict-resolver.py`, which ran as a
//! PreToolUse hook on `Bash` calls. When `op run` injects secrets into a
//! command's environment, any variable already set in the current shell
//! environment wins the race and 1Password's value is silently never used.
//! This module finds two sources of such conflicts:
//!
//! - a variable already in the environment whose value is an unresolved
//!   `op://` reference (direnv or a dotfile exported it without resolving),
//! - a variable named in an `--env-file <path>` passed to `op run`, with an
//!   `op://` value in that file, that is *also* already set in the
//!   environment.
//!
//! Variables already unset via an `env -u VAR` prefix in the command are
//! excluded, so a corrected command is not flagged again.

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;

/// One command's conflicting environment variables, in sorted, deduplicated
/// order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflicts {
    pub vars: Vec<String>,
}

impl Conflicts {
    /// The corrected command: `original` prefixed with `env -u VAR ...` for
    /// every conflicting variable.
    pub fn corrected_command(&self, original: &str) -> String {
        let unset_flags = self
            .vars
            .iter()
            .map(|v| format!("-u {v}"))
            .collect::<Vec<_>>()
            .join(" ");
        format!("env {unset_flags} {original}")
    }
}

/// Returns the names of environment variables in `env_vars` whose value is
/// an unresolved `op://` reference.
fn direct_env_conflicts(env_vars: &HashMap<String, String>) -> Vec<String> {
    env_vars
        .iter()
        .filter(|(_, value)| value.starts_with("op://"))
        .map(|(name, _)| name.clone())
        .collect()
}

/// Extracts the path argument of `--env-file` from a command string, if
/// present. Handles `--env-file path`, `--env-file=path`, and single- or
/// double-quoted paths.
fn env_file_path(command: &str) -> Option<String> {
    let idx = command.find("--env-file")?;
    let after = &command[idx + "--env-file".len()..];
    let boundary_ok = after
        .chars()
        .next()
        .is_none_or(|c| c == '=' || c.is_whitespace());
    if !boundary_ok {
        return None;
    }
    let rest = after.trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim_start();

    let mut chars = rest.chars();
    match chars.next() {
        Some(quote @ ('\'' | '"')) => {
            let end = rest[1..].find(quote)?;
            Some(rest[1..1 + end].to_string())
        }
        Some(_) => {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            Some(rest[..end].to_string())
        }
        None => None,
    }
}

/// Reads an `--env-file`-style file (`KEY=value` lines, `#` comments) and
/// returns names defined there with an `op://` value that are *also*
/// present in `env_vars`.
fn env_file_conflicts(
    command: &str,
    env_vars: &HashMap<String, String>,
    read_file: impl Fn(&Path) -> Option<String>,
) -> Vec<String> {
    let Some(path) = env_file_path(command) else {
        return Vec::new();
    };
    let expanded = shellexpand_home(&path);
    let Some(content) = read_file(Path::new(&expanded)) else {
        return Vec::new();
    };

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (name, value) = line.split_once('=')?;
            let name = name.trim();
            let value = value.trim().trim_matches(['\'', '"']);
            if value.starts_with("op://") && env_vars.contains_key(name) {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Expands a leading `~` to `$HOME`, mirroring Python's `Path.expanduser()`.
fn shellexpand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix('~') {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}{rest}");
        }
    }
    path.to_string()
}

/// Parses `env -u VAR` prefixes already present in `command` and returns the
/// set of variable names already being unset.
fn already_unset_vars(command: &str) -> BTreeSet<String> {
    let mut unset = BTreeSet::new();
    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "env" {
            let mut j = i + 1;
            while j + 1 < tokens.len() && tokens[j] == "-u" {
                unset.insert(tokens[j + 1].to_string());
                j += 2;
            }
        }
        i += 1;
    }
    unset
}

/// Finds all `op run` conflicts in `command`, given the current process
/// environment. Returns `None` if the command does not mention `op run` or
/// has no conflicts.
pub fn resolve(command: &str, env_vars: &HashMap<String, String>) -> Option<Conflicts> {
    if !command.contains("op run") {
        return None;
    }

    let already_unset = already_unset_vars(command);

    let mut found: Vec<String> = direct_env_conflicts(env_vars);
    found.extend(env_file_conflicts(command, env_vars, |path| {
        std::fs::read_to_string(path).ok()
    }));

    let unique: BTreeSet<String> = found
        .into_iter()
        .filter(|v| !already_unset.contains(v))
        .collect();

    if unique.is_empty() {
        None
    } else {
        Some(Conflicts {
            vars: unique.into_iter().collect(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn no_op_run_is_ignored() {
        let env = HashMap::new();
        assert_eq!(resolve("echo hello", &env), None);
    }

    #[test]
    fn direct_env_conflict_is_found() {
        let mut env = HashMap::new();
        env.insert(
            "ANTHROPIC_API_KEY".to_string(),
            "op://vault/item/field".to_string(), // obfsck:ignore
        );
        let conflicts = resolve("op run -- cargo build", &env).expect("conflict");
        assert_eq!(conflicts.vars, vec!["ANTHROPIC_API_KEY".to_string()]);
    }

    #[test]
    fn already_unset_vars_are_excluded() {
        let mut env = HashMap::new();
        env.insert(
            "ANTHROPIC_API_KEY".to_string(),
            "op://vault/item/field".to_string(), // obfsck:ignore
        );
        let conflicts = resolve("env -u ANTHROPIC_API_KEY op run -- cargo build", &env);
        assert_eq!(conflicts, None);
    }

    #[test]
    fn corrected_command_prefixes_env_unset() {
        let conflicts = Conflicts {
            vars: vec!["FOO".to_string(), "BAR".to_string()],
        };
        assert_eq!(
            conflicts.corrected_command("op run -- cargo build"),
            "env -u FOO -u BAR op run -- cargo build"
        );
    }

    #[test]
    fn env_file_path_parses_quoted_and_bare() {
        assert_eq!(
            env_file_path("op run --env-file=/a/b.env -- x"),
            Some("/a/b.env".to_string())
        );
        assert_eq!(
            env_file_path(r#"op run --env-file "/a/b with space.env" -- x"#),
            Some("/a/b with space.env".to_string())
        );
        assert_eq!(
            env_file_path("op run --env-file /a/b.env -- x"),
            Some("/a/b.env".to_string())
        );
        assert_eq!(env_file_path("op run -- x"), None);
    }

    /// Regression for the port's original flag-boundary bug: `--env-file`
    /// was matched by `str::find` as a bare substring, so an unrelated flag
    /// that merely *starts with* `--env-file` (e.g. `--env-file-override`)
    /// was misparsed as `--env-file` with `-override` read back as its path
    /// argument. The fix requires the byte right after the `--env-file`
    /// literal to be `=`, ASCII whitespace, or end-of-string before treating
    /// it as a match — a flag with a `-` immediately following must not
    /// match at all.
    #[test]
    fn env_file_flag_is_not_matched_by_a_longer_flag_sharing_its_prefix() {
        assert_eq!(
            env_file_path("op run --env-file-override /some/path -- x"),
            None
        );
    }

    /// End-to-end counterpart of the above: a command carrying only the
    /// look-alike `--env-file-override` flag (pointing at a file that *does*
    /// define a conflicting var) must not surface that var as a conflict,
    /// because the flag is not `--env-file` and should never be read as an
    /// env-file source.
    #[test]
    fn resolve_does_not_treat_env_file_override_flag_as_an_env_file_source() {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "unrelated-value".to_string());

        let conflicts = resolve("op run --env-file-override /some/path -- cargo build", &env);
        assert_eq!(conflicts, None);
    }
}
