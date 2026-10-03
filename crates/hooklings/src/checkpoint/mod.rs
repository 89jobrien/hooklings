//! Coalesced checkpoint commits for agent-driven edits.
//!
//! An agent harness records each path it touches while it works. Nothing is
//! committed at that point. At a turn or session boundary, a single checkpoint
//! commit collects everything recorded since the last one, so a multi-file change
//! lands as one coherent commit rather than one per keystroke.

pub mod commit;
pub mod message;
pub mod record;
pub mod stats;

pub use commit::{
    Outcome, RefuseReason, RejectedPath, RejectionReason, Report, Request, SkipReason,
};
pub use message::{ChangeKind, CommitType, FileChange};
pub use record::{Record, RecordError, Sidecar};

use std::path::Path;

/// Opens the repository containing `dir`, if there is one.
pub fn discover(dir: &Path) -> anyhow::Result<gix::Repository> {
    Ok(gix::discover(dir)?)
}

/// Records `paths` as touched since the last checkpoint.
///
/// Returns the paths newly added to the record, which excludes duplicates.
pub fn record_paths(
    repo: &gix::Repository,
    paths: &[String],
    now: &str,
) -> anyhow::Result<Vec<String>> {
    let sidecar = Sidecar::for_repo(repo.git_dir());
    let mut record = sidecar.load()?;
    let mut added = Vec::new();

    for path in paths {
        if commit::validate_recorded_path(path).is_some() {
            continue;
        }
        if record::merge(&mut record, path, now) {
            added.push(path.clone());
        }
    }

    if !added.is_empty() {
        sidecar.store(&record)?;
    }

    Ok(added)
}

/// Reads the current record for a repository.
pub fn load_record(repo: &gix::Repository) -> anyhow::Result<Record> {
    Ok(Sidecar::for_repo(repo.git_dir()).load()?)
}
