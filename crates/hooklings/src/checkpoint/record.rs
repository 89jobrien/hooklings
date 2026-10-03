//! Per-repository record of paths an agent harness has touched.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Paths edited since the last checkpoint, persisted outside the worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub edits: u64,
    #[serde(default)]
    pub started_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// Location of the sidecar file backing a [`Record`].
///
/// It lives under the git directory rather than the worktree so that recording an
/// edit never itself dirties the working tree.
#[derive(Debug, Clone)]
pub struct Sidecar {
    path: PathBuf,
}

/// Failures that can occur while reading or writing the sidecar.
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("IO error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("corrupt checkpoint record at {path}: {source}")]
    Decode {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

impl Sidecar {
    /// Places the sidecar at `<git-dir>/hooklings/recorded.json`.
    pub fn for_repo(git_dir: &Path) -> Self {
        Self {
            path: git_dir.join("hooklings").join("recorded.json"),
        }
    }

    /// The absolute path of the sidecar file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the record, treating an absent file as an empty record.
    pub fn load(&self) -> Result<Record, RecordError> {
        let raw = match std::fs::read(&self.path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Record::default()),
            Err(source) => {
                return Err(RecordError::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        };

        serde_json::from_slice(&raw).map_err(|source| RecordError::Decode {
            path: self.path.clone(),
            source,
        })
    }

    /// Writes the record, creating the parent directory if needed.
    pub fn store(&self, record: &Record) -> Result<(), RecordError> {
        let io = |source| RecordError::Io {
            path: self.path.clone(),
            source,
        };

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let encoded = serde_json::to_vec_pretty(record).map_err(|source| RecordError::Decode {
            path: self.path.clone(),
            source,
        })?;
        std::fs::write(&self.path, encoded).map_err(io)
    }

    /// Removes the sidecar, tolerating an already-absent file.
    pub fn clear(&self) -> Result<(), RecordError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(RecordError::Io {
                path: self.path.clone(),
                source,
            }),
        }
    }
}

/// Merges `path` into `record`, returning `true` when it was not already present.
pub fn merge(record: &mut Record, path: &str, now: &str) -> bool {
    let is_new = !record.paths.iter().any(|existing| existing == path);
    if is_new {
        record.paths.push(path.to_owned());
    }
    if record.started_at.is_empty() {
        record.started_at = now.to_owned();
    }
    record.updated_at = now.to_owned();
    record.edits = record.edits.saturating_add(1);
    is_new
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_sidecar() -> (tempfile::TempDir, Sidecar) {
        let dir = tempfile::tempdir().expect("temp dir");
        let sidecar = Sidecar::for_repo(dir.path());
        (dir, sidecar)
    }

    #[test]
    fn load_absent_file_is_empty_record() {
        let (_dir, sidecar) = temp_sidecar();
        assert_eq!(sidecar.load().expect("load"), Record::default());
    }

    #[test]
    fn store_then_load_round_trips() {
        let (_dir, sidecar) = temp_sidecar();
        let record = Record {
            paths: vec!["src/lib.rs".to_owned()],
            edits: 3,
            started_at: "t0".to_owned(),
            updated_at: "t1".to_owned(),
        };
        sidecar.store(&record).expect("store");
        assert_eq!(sidecar.load().expect("load"), record);
    }

    #[test]
    fn clear_is_idempotent() {
        let (_dir, sidecar) = temp_sidecar();
        sidecar.store(&Record::default()).expect("store");
        sidecar.clear().expect("first clear");
        sidecar.clear().expect("second clear");
    }

    #[test]
    fn merge_deduplicates_paths_but_counts_every_edit() {
        let mut record = Record::default();
        assert!(merge(&mut record, "src/lib.rs", "t0"));
        assert!(!merge(&mut record, "src/lib.rs", "t1"));
        assert_eq!(record.paths.len(), 1);
        assert_eq!(record.edits, 2);
        assert_eq!(record.started_at, "t0");
        assert_eq!(record.updated_at, "t1");
    }
}
