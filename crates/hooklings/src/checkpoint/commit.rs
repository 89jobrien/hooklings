//! Coalesced checkpoint commits driven by an agent harness.

use std::collections::{BTreeSet, HashMap};

use gix::index::entry::{Flags, Mode, Stage};
use gix::objs::tree::{EntryKind, EntryMode};

use crate::config::CheckpointConfig;

use super::message::{self, ChangeKind, CommitType, FileChange};
use super::record::{Record, Sidecar};
use super::stats::{self, Delta};

/// Caller-supplied adjustments to a checkpoint run.
#[derive(Debug)]
pub struct Request<'a> {
    pub config: &'a CheckpointConfig,
    pub subject_override: Option<String>,
    pub type_override: Option<CommitType>,
    pub scope_override: Option<String>,
    pub dry_run: bool,
}

/// Why no commit was attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    Disabled,
    NoRecord,
    NothingChanged,
}

/// Why a recorded changeset was deliberately not committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefuseReason {
    OnProtectedBranch { branch: String },
    ForeignStaged { paths: Vec<String> },
    TooManyFiles { count: usize, limit: usize },
    TooManyLines { count: u64, limit: u64 },
}

/// Result of a checkpoint attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Committed {
        id: String,
        subject: String,
        files: usize,
        added: u64,
        removed: u64,
    },
    Planned {
        subject: String,
        files: usize,
        added: u64,
        removed: u64,
    },
    Skipped(SkipReason),
    Refused(RefuseReason),
}

/// Why a recorded path cannot be checkpointed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    OutsideRepository,
    Ignored,
    NotAFile,
    UnsupportedMode,
}

impl std::fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutsideRepository => f.write_str("outside repository workdir"),
            Self::Ignored => f.write_str("matches .gitignore"),
            Self::NotAFile => f.write_str("not a regular file"),
            Self::UnsupportedMode => {
                f.write_str("executable or symlink; commit this path manually")
            }
        }
    }
}

/// A recorded path that was dropped, with the reason it was dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedPath {
    pub path: String,
    pub reason: RejectionReason,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => f.write_str("checkpointing is disabled in config"),
            Self::NoRecord => f.write_str("no recorded paths since last checkpoint"),
            Self::NothingChanged => f.write_str("recorded paths are unchanged in the worktree"),
        }
    }
}

impl std::fmt::Display for RefuseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OnProtectedBranch { branch } => {
                write!(
                    f,
                    "refusing to commit directly to protected branch '{branch}'"
                )
            }
            Self::ForeignStaged { paths } => write!(
                f,
                "refusing to commit: {} path(s) staged outside this checkpoint: {}",
                paths.len(),
                paths.join(", ")
            ),
            Self::TooManyFiles { count, limit } => {
                write!(f, "refusing to commit {count} files: limit is {limit}")
            }
            Self::TooManyLines { count, limit } => write!(
                f,
                "refusing to commit {count} changed lines: limit is {limit}"
            ),
        }
    }
}

/// Rejects paths that must never enter a checkpoint.
///
/// Absolute paths, parent traversal, and git-internal paths are dropped so that a
/// confused or compromised harness cannot make a checkpoint write outside the
/// repository or into the git directory.
pub fn validate_recorded_path(path: &str) -> Option<RejectionReason> {
    let path = path.strip_prefix("./").unwrap_or(path);

    if path.is_empty() || path.starts_with('/') {
        return Some(RejectionReason::OutsideRepository);
    }
    if path
        .split('/')
        .any(|segment| segment == ".." || segment.is_empty())
    {
        return Some(RejectionReason::OutsideRepository);
    }
    if path == ".git" || path.starts_with(".git/") {
        return Some(RejectionReason::OutsideRepository);
    }
    None
}

/// Returns the short name of the checked-out branch, if there is one.
pub fn current_branch(repo: &gix::Repository) -> Option<String> {
    let head = repo.head().ok()?;
    Some(head.referent_name()?.shorten().to_string())
}

/// Returns `true` for branches that must only move by explicit merge.
pub fn is_protected_branch(branch: &str, configured: &[String]) -> bool {
    const ALWAYS: &[&str] = &["main", "master"];
    configured.iter().any(|name| name == branch) || ALWAYS.contains(&branch)
}

/// New blob payload for a changed path, with the index mode it should be stored under.
struct Payload {
    mode: Mode,
    bytes: Vec<u8>,
}

/// What a single recorded path turned out to be.
enum PathState {
    Changed {
        change: FileChange,
        payload: Option<Payload>,
    },
    Unchanged,
    Rejected(RejectionReason),
}

/// Resolves the index mode for a regular worktree file.
fn file_mode(metadata: &std::fs::Metadata) -> Mode {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 != 0 {
            return Mode::FILE_EXECUTABLE;
        }
    }
    let _ = metadata;
    Mode::FILE
}

/// Builds a path-to-blob map for the committed tree.
///
/// The traversal yields tree entries as well as blobs, so tree entries are
/// filtered out: treating a directory's object id as file content would make
/// every change under it look permanently modified.
fn committed_entries(
    repo: &gix::Repository,
) -> anyhow::Result<HashMap<String, gix::hash::ObjectId>> {
    let tree = repo.head_tree()?;
    let blob_mode = EntryMode::from(EntryKind::Blob);
    let mut entries = HashMap::new();

    for entry in tree.traverse().breadthfirst.files()? {
        if entry.mode != blob_mode {
            continue;
        }
        entries.insert(entry.filepath.to_string(), entry.oid.to_owned());
    }

    Ok(entries)
}

fn blob_bytes(repo: &gix::Repository, oid: gix::hash::ObjectId) -> Option<Vec<u8>> {
    repo.find_object(oid).ok().map(|object| object.data.clone())
}

/// Reads a payload and its mode for a regular worktree file.
fn read_payload(path: &std::path::Path, metadata: &std::fs::Metadata) -> Option<Payload> {
    let bytes = std::fs::read(path).ok()?;
    Some(Payload {
        mode: file_mode(metadata),
        bytes,
    })
}

/// Turns recorded paths into a real changeset relative to `HEAD`.
fn resolve_paths<'a>(
    repo: &gix::Repository,
    workdir: &std::path::Path,
    committed: &HashMap<String, gix::hash::ObjectId>,
    excluded: &BTreeSet<String>,
    paths: &'a [String],
) -> Vec<(&'a str, PathState)> {
    let mut out = Vec::with_capacity(paths.len());

    for path in paths {
        let state = resolve_one(repo, workdir, committed, excluded, path);
        out.push((path.as_str(), state));
    }

    out
}

fn resolve_one(
    repo: &gix::Repository,
    workdir: &std::path::Path,
    committed: &HashMap<String, gix::hash::ObjectId>,
    excluded: &BTreeSet<String>,
    path: &str,
) -> PathState {
    if validate_recorded_path(path).is_some() {
        return PathState::Rejected(RejectionReason::OutsideRepository);
    }
    if excluded.contains(path) {
        return PathState::Rejected(RejectionReason::Ignored);
    }

    let relative = path.strip_prefix("./").unwrap_or(path);
    let absolute = workdir.join(relative);
    let in_tree = committed.get(relative).copied();

    let metadata = match std::fs::symlink_metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(_) if in_tree.is_some() => {
            let before = blob_bytes(repo, in_tree.expect("guarded above"));
            let (added, removed, binary) = split(stats::delta(before.as_deref(), None));
            return PathState::Changed {
                change: FileChange {
                    path: path.to_owned(),
                    kind: ChangeKind::Deleted,
                    added,
                    removed,
                    binary,
                },
                payload: None,
            };
        }
        Err(_) => return PathState::Rejected(RejectionReason::OutsideRepository),
    };

    if metadata.is_dir() {
        return PathState::Rejected(RejectionReason::NotAFile);
    }

    // gix's tree editor accepts an `EntryKind` rather than a mode, so it can only
    // express 0o100644 blobs. Staging an executable or a symlink through it would
    // silently drop the mode, so those paths are refused and left to the agent.
    if metadata.is_symlink() || file_mode(&metadata) != Mode::FILE {
        return PathState::Rejected(RejectionReason::UnsupportedMode);
    }

    let Some(payload) = read_payload(&absolute, &metadata) else {
        return PathState::Rejected(RejectionReason::NotAFile);
    };

    let before = in_tree.and_then(|oid| blob_bytes(repo, oid));
    if in_tree.is_some() && before.as_deref() == Some(payload.bytes.as_slice()) {
        return PathState::Unchanged;
    }

    let (added, removed, binary) = split(stats::delta(before.as_deref(), Some(&payload.bytes)));

    PathState::Changed {
        change: FileChange {
            path: path.to_owned(),
            kind: if in_tree.is_some() {
                ChangeKind::Modified
            } else {
                ChangeKind::Added
            },
            added,
            removed,
            binary,
        },
        payload: Some(payload),
    }
}

fn split(delta: Delta) -> (u64, u64, bool) {
    match delta {
        Delta::Lines(counts) => (counts.added, counts.removed, false),
        Delta::Binary => (0, 0, true),
    }
}

/// Lists paths that are already staged and were not recorded by this checkpoint.
fn foreign_staged(
    repo: &gix::Repository,
    committed: &HashMap<String, gix::hash::ObjectId>,
    ours: &BTreeSet<String>,
) -> anyhow::Result<Vec<String>> {
    let index = repo.index_or_load_from_head_or_empty()?.into_owned();
    let state: &gix::index::State = &index;

    let mut foreign = Vec::new();
    for entry in state.entries() {
        if entry.stage() != Stage::Unconflicted {
            continue;
        }
        let path = entry.path(state).to_string();
        if ours.contains(&path) {
            continue;
        }
        if committed.get(&path) != Some(&entry.id) {
            foreign.push(path);
        }
    }
    foreign.sort();

    Ok(foreign)
}

/// Writes the committed paths into the on-disk index so the worktree reads clean.
fn sync_index(
    repo: &gix::Repository,
    changed: &[(String, Payload)],
    deletions: &[String],
) -> anyhow::Result<()> {
    let mut index = repo.index_or_load_from_head_or_empty()?.into_owned();

    for path in deletions {
        let path = gix::bstr::BString::from(path.as_str());
        if let Some(entry) = index.entry_mut_by_path_and_stage(path.as_ref(), Stage::Unconflicted) {
            entry.flags = Flags::REMOVE;
        }
    }

    for (path, payload) in changed {
        let path = gix::bstr::BString::from(path.as_str());
        let id = repo.write_blob(&payload.bytes)?.detach();

        match index.entry_mut_by_path_and_stage(path.as_ref(), Stage::Unconflicted) {
            Some(entry) => {
                entry.id = id;
                entry.mode = payload.mode;
            }
            None => {
                index.dangerously_push_entry(
                    gix::index::entry::Stat::default(),
                    id,
                    Flags::empty(),
                    payload.mode,
                    path.as_ref(),
                );
                index.sort_entries();
            }
        }
    }

    index.remove_tree();
    index.write(gix::index::write::Options::default())?;

    Ok(())
}

/// Assembles the commit message for a resolved changeset.
fn build_message(
    changes: &[FileChange],
    record: &Record,
    request: &Request<'_>,
) -> (String, String) {
    let commit_type = request
        .type_override
        .unwrap_or_else(|| message::infer_type(changes));

    let subject = match (&request.subject_override, &request.scope_override) {
        (Some(subject), _) => subject.clone(),
        (None, Some(scope)) => {
            let base = message::subject(changes, commit_type);
            match base.split_once(": ") {
                Some((_, rest)) => format!("{}({scope}): {rest}", commit_type.as_str()),
                None => base,
            }
        }
        (None, None) => message::subject(changes, commit_type),
    };

    let started_at = if record.started_at.is_empty() {
        "unknown"
    } else {
        record.started_at.as_str()
    };

    (subject, message::body(changes, record.edits, started_at))
}

/// Report returned to the caller alongside the outcome.
#[derive(Debug)]
pub struct Report {
    pub outcome: Outcome,
    pub rejected: Vec<RejectedPath>,
}

/// Runs a checkpoint against an already-discovered repository.
pub fn run(repo: &gix::Repository, request: &Request<'_>) -> anyhow::Result<Report> {
    let config = request.config;
    let mut report = Report {
        outcome: Outcome::Skipped(SkipReason::Disabled),
        rejected: Vec::new(),
    };

    if !config.enabled {
        return Ok(report);
    }

    let sidecar = Sidecar::for_repo(repo.git_dir());
    let record = sidecar.load()?;

    if record.paths.is_empty() {
        report.outcome = Outcome::Skipped(SkipReason::NoRecord);
        return Ok(report);
    }

    let branch = current_branch(repo).unwrap_or_else(|| "HEAD".to_owned());
    if !config.allow_protected_branch && is_protected_branch(&branch, &config.protected_branches) {
        report.outcome = Outcome::Refused(RefuseReason::OnProtectedBranch { branch });
        return Ok(report);
    }

    if repo.head()?.is_unborn() {
        anyhow::bail!("repository has no commits yet; make an initial commit first");
    }

    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("cannot checkpoint a bare repository"))?
        .to_path_buf();

    let committed = committed_entries(repo)?;
    let excluded = excluded_paths(repo, &record.paths);

    let resolved = resolve_paths(repo, &workdir, &committed, &excluded, &record.paths);

    let mut changes: Vec<FileChange> = Vec::new();
    let mut payloads: Vec<(String, Payload)> = Vec::new();
    let mut deletions: Vec<String> = Vec::new();

    for (path, state) in resolved {
        match state {
            PathState::Unchanged => {}
            PathState::Rejected(reason) => {
                report.rejected.push(RejectedPath {
                    path: path.to_owned(),
                    reason,
                });
            }
            PathState::Changed { change, payload } => {
                changes.push(change);
                match payload {
                    Some(payload) => payloads.push((path.to_owned(), payload)),
                    None => deletions.push(path.to_owned()),
                }
            }
        }
    }

    if changes.is_empty() {
        report.outcome = Outcome::Skipped(SkipReason::NothingChanged);
        return Ok(report);
    }

    if changes.len() > config.max_files {
        report.outcome = Outcome::Refused(RefuseReason::TooManyFiles {
            count: changes.len(),
            limit: config.max_files,
        });
        return Ok(report);
    }

    let total_lines: u64 = changes
        .iter()
        .map(|change| change.added.saturating_add(change.removed))
        .sum();
    if total_lines > config.max_lines {
        report.outcome = Outcome::Refused(RefuseReason::TooManyLines {
            count: total_lines,
            limit: config.max_lines,
        });
        return Ok(report);
    }

    let ours: BTreeSet<String> = changes.iter().map(|change| change.path.clone()).collect();

    let foreign = foreign_staged(repo, &committed, &ours)?;
    if !foreign.is_empty() {
        report.outcome = Outcome::Refused(RefuseReason::ForeignStaged { paths: foreign });
        return Ok(report);
    }

    let (subject, body) = build_message(&changes, &record, request);
    let added: u64 = changes.iter().map(|change| change.added).sum();
    let removed: u64 = changes.iter().map(|change| change.removed).sum();

    if request.dry_run {
        report.outcome = Outcome::Planned {
            subject,
            files: changes.len(),
            added,
            removed,
        };
        return Ok(report);
    }

    let head_tree = repo.head_tree()?;
    let mut editor = head_tree.edit()?;

    for (path, payload) in &payloads {
        let id = repo.write_blob(&payload.bytes)?.detach();
        editor.upsert(path.as_str(), EntryKind::Blob, id)?;
    }
    for path in &deletions {
        editor.remove_leaf(path.as_str())?;
    }

    let tree_id = editor.write()?;
    sync_index(repo, &payloads, &deletions)?;

    let head_id = repo.head_id()?.detach();
    let commit_id = repo.commit("HEAD", format!("{subject}\n\n{body}"), tree_id, [head_id])?;

    sidecar.clear()?;

    report.outcome = Outcome::Committed {
        id: commit_id.detach().to_string(),
        subject,
        files: changes.len(),
        added,
        removed,
    };

    Ok(report)
}

/// Computes which recorded paths the repository's ignore rules exclude.
fn excluded_paths(repo: &gix::Repository, paths: &[String]) -> BTreeSet<String> {
    let mut excluded = BTreeSet::new();

    let Ok(index) = repo
        .index_or_load_from_head_or_empty()
        .map(|index| index.into_owned())
    else {
        return excluded;
    };
    let state: &gix::index::State = &index;
    let Ok(mut stack) = repo.excludes(state, None, Default::default()) else {
        return excluded;
    };

    for path in paths {
        let relative = path.strip_prefix("./").unwrap_or(path);
        let is_excluded = stack
            .at_path(std::path::Path::new(relative), None)
            .map(|platform| platform.is_excluded())
            .unwrap_or(false);

        if is_excluded {
            excluded.insert(path.clone());
        }
    }

    excluded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_absolute_paths() {
        assert_eq!(
            validate_recorded_path("/etc/passwd"),
            Some(RejectionReason::OutsideRepository)
        );
    }

    #[test]
    fn rejects_parent_traversal() {
        assert_eq!(
            validate_recorded_path("../outside.rs"),
            Some(RejectionReason::OutsideRepository)
        );
        assert_eq!(
            validate_recorded_path("src/../../outside.rs"),
            Some(RejectionReason::OutsideRepository)
        );
    }

    #[test]
    fn rejects_git_internals() {
        assert_eq!(
            validate_recorded_path(".git/config"),
            Some(RejectionReason::OutsideRepository)
        );
        assert_eq!(
            validate_recorded_path(".git"),
            Some(RejectionReason::OutsideRepository)
        );
    }

    #[test]
    fn rejects_empty_and_double_slash_paths() {
        assert!(validate_recorded_path("").is_some());
        assert!(validate_recorded_path("src//main.rs").is_some());
    }

    #[test]
    fn accepts_relative_paths_and_strips_dot_prefix() {
        assert_eq!(validate_recorded_path("src/main.rs"), None);
        assert_eq!(validate_recorded_path("./src/main.rs"), None);
        assert_eq!(validate_recorded_path("a/b/c.rs"), None);
    }

    #[test]
    fn main_and_master_are_always_protected() {
        assert!(is_protected_branch("main", &[]));
        assert!(is_protected_branch("master", &[]));
        assert!(!is_protected_branch("feature/x", &[]));
    }

    #[test]
    fn configured_branches_are_protected() {
        let configured = vec!["develop".to_owned()];
        assert!(is_protected_branch("develop", &configured));
        assert!(!is_protected_branch("develop", &[]));
    }
}
