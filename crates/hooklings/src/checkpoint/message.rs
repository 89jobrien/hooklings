//! Conventional Commit inference for coalesced checkpoint commits.

/// What happened to a single tracked path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
}

impl ChangeKind {
    /// Single-character marker used in the commit body.
    pub fn marker(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Modified => 'M',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
        }
    }
}

/// A committed path together with its line delta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
    pub added: u64,
    pub removed: u64,
    pub binary: bool,
}

/// Conventional Commit type, restricted to what offline inference can defend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitType {
    Feat,
    Fix,
    Docs,
    Test,
    Refactor,
    Style,
    Build,
    Chore,
}

impl CommitType {
    /// Conventional Commit keyword.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Feat => "feat",
            Self::Fix => "fix",
            Self::Docs => "docs",
            Self::Test => "test",
            Self::Refactor => "refactor",
            Self::Style => "style",
            Self::Build => "build",
            Self::Chore => "chore",
        }
    }
}

/// Verb describing the dominant change kind.
fn verb(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => "add",
        ChangeKind::Deleted => "remove",
        ChangeKind::Renamed => "move",
        ChangeKind::Modified => "update",
    }
}

/// Returns `true` for paths whose only role in the repo is prose.
pub fn is_doc_path(path: &str) -> bool {
    matches!(
        extension(path).as_deref(),
        Some("md" | "markdown" | "mdx" | "rst" | "adoc" | "txt")
    ) || path.starts_with("docs/")
}

/// Returns `true` for paths that exist to assert behaviour.
pub fn is_test_path(path: &str) -> bool {
    let file = file_name(path);
    let ext = extension(path).unwrap_or_default();
    let stem = file.strip_suffix(ext.as_str()).unwrap_or(file.as_str());

    path.starts_with("tests/")
        || path.starts_with("test/")
        || path.starts_with("spec/")
        || path.contains("/tests/")
        || path.contains("/test/")
        || path.ends_with("_test.rs")
        || path.ends_with(".test.ts")
        || path.ends_with(".test.js")
        || path.ends_with(".spec.ts")
        || path.ends_with(".spec.js")
        || stem.starts_with("test_")
        || stem.ends_with("_test")
}

/// Returns `true` for paths that only record dependency or formatting state.
pub fn is_style_path(path: &str) -> bool {
    let file = file_name(path);
    matches!(
        file.as_str(),
        "Cargo.lock"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "uv.lock"
            | ".rustfmt.toml"
            | ".editorconfig"
    )
}

/// Returns `true` for dependency manifests.
pub fn is_build_path(path: &str) -> bool {
    matches!(
        file_name(path).as_str(),
        "Cargo.toml"
            | "package.json"
            | "pyproject.toml"
            | "go.mod"
            | "go.sum"
            | "build.gradle"
            | "pom.xml"
    )
}

/// Infers the commit type from the whole changeset.
///
/// A single-file changeset classifies by that file's role; a mixed changeset
/// classifies by its strongest member. `fix` is never inferred, because nothing
/// about a diff distinguishes a repair from any other edit — callers pass it
/// explicitly instead.
pub fn infer_type(changes: &[FileChange]) -> CommitType {
    if changes.is_empty() {
        return CommitType::Chore;
    }

    if changes.iter().any(|change| change.binary) {
        return CommitType::Chore;
    }

    if changes
        .iter()
        .all(|change| change.kind == ChangeKind::Added)
    {
        return CommitType::Feat;
    }
    if changes
        .iter()
        .all(|change| matches!(change.kind, ChangeKind::Deleted | ChangeKind::Renamed))
    {
        return CommitType::Refactor;
    }

    let any = |predicate: fn(&str) -> bool| changes.iter().any(|change| predicate(&change.path));
    let all = |predicate: fn(&str) -> bool| changes.iter().all(|change| predicate(&change.path));

    if all(is_test_path) {
        CommitType::Test
    } else if all(is_doc_path) {
        CommitType::Docs
    } else if all(is_style_path) {
        CommitType::Style
    } else if all(is_build_path) {
        CommitType::Build
    } else if any(is_test_path) {
        CommitType::Test
    } else {
        CommitType::Chore
    }
}

/// Derives a Conventional Commit scope from the deepest shared path prefix.
///
/// Returns `None` when paths span multiple top-level directories, since any scope
/// covering both would be too vague to be useful.
pub fn infer_scope(changes: &[FileChange]) -> Option<String> {
    let mut dirs: Option<Vec<&str>> = None;

    for change in changes {
        let parts: Vec<&str> = change.path.split('/').collect();
        let dir = &parts[..parts.len().saturating_sub(1)];

        dirs = Some(match dirs {
            None => dir.to_vec(),
            Some(current) => {
                let mut shared = Vec::with_capacity(current.len());
                for (left, right) in current.iter().zip(dir.iter()) {
                    if left == right {
                        shared.push(*left);
                    } else {
                        break;
                    }
                }
                shared
            }
        });
    }

    match dirs {
        Some(parts) if !parts.is_empty() => Some(parts.join("/")),
        _ => None,
    }
}

/// Builds the commit subject line.
pub fn subject(changes: &[FileChange], commit_type: CommitType) -> String {
    let count = changes.len();
    let noun = if count == 1 { "file" } else { "files" };

    let mut verb_source = ChangeKind::Modified;
    for kind in [
        ChangeKind::Deleted,
        ChangeKind::Renamed,
        ChangeKind::Added,
        ChangeKind::Modified,
    ] {
        if changes.iter().any(|change| change.kind == kind) {
            verb_source = kind;
            break;
        }
    }

    match infer_scope(changes) {
        Some(scope) => format!(
            "{}({scope}): {} {count} {noun}",
            commit_type.as_str(),
            verb(verb_source)
        ),
        None => format!(
            "{}: {} {count} {noun}",
            commit_type.as_str(),
            verb(verb_source)
        ),
    }
}

/// Builds the commit body: a per-file stat block and provenance line.
pub fn body(changes: &[FileChange], edits: u64, since: &str) -> String {
    let mut out = String::from("Files:\n");

    let width = changes
        .iter()
        .map(|change| change.path.len())
        .max()
        .unwrap_or(0)
        .max(4);

    for change in changes {
        let delta = match change.binary {
            true => "binary".to_owned(),
            false => format!("+{} -{}", change.added, change.removed),
        };
        out.push_str(&format!(
            "  {} {:<width$}  {delta}\n",
            change.kind.marker(),
            change.path,
            width = width
        ));
    }

    out.push_str(&format!(
        "\nCoalesced checkpoint: {edits} edit(s) since {since}."
    ));
    out
}

fn extension(path: &str) -> Option<String> {
    path.rsplit_once('.').map(|(_, ext)| ext.to_owned())
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, kind: ChangeKind) -> FileChange {
        FileChange {
            path: path.to_owned(),
            kind,
            added: 0,
            removed: 0,
            binary: false,
        }
    }

    #[test]
    fn new_files_infer_feat() {
        let changes = vec![
            change("src/checkpoint/mod.rs", ChangeKind::Added),
            change("src/checkpoint/record.rs", ChangeKind::Added),
        ];
        assert_eq!(infer_type(&changes), CommitType::Feat);
    }

    #[test]
    fn deletions_and_renames_infer_refactor() {
        let changes = vec![
            change("src/old.rs", ChangeKind::Deleted),
            change("src/new.rs", ChangeKind::Renamed),
        ];
        assert_eq!(infer_type(&changes), CommitType::Refactor);
    }

    #[test]
    fn homogeneous_docs_infer_docs() {
        let changes = vec![
            change("README.md", ChangeKind::Modified),
            change("docs/guide.mdx", ChangeKind::Added),
        ];
        assert_eq!(infer_type(&changes), CommitType::Docs);
    }

    #[test]
    fn homogeneous_tests_infer_test() {
        let changes = vec![
            change("tests/config.rs", ChangeKind::Modified),
            change("src/config_test.rs", ChangeKind::Modified),
        ];
        assert_eq!(infer_type(&changes), CommitType::Test);
    }

    #[test]
    fn test_file_in_mixed_changeset_outranks_docs() {
        let changes = vec![
            change("README.md", ChangeKind::Modified),
            change("tests/config.rs", ChangeKind::Modified),
        ];
        assert_eq!(infer_type(&changes), CommitType::Test);
    }

    #[test]
    fn ordinary_edits_default_to_chore_not_fix() {
        let changes = vec![change("src/main.rs", ChangeKind::Modified)];
        assert_eq!(infer_type(&changes), CommitType::Chore);
    }

    #[test]
    fn lockfile_only_changeset_infers_style() {
        let changes = vec![change("Cargo.lock", ChangeKind::Modified)];
        assert_eq!(infer_type(&changes), CommitType::Style);
    }

    #[test]
    fn manifest_only_changeset_infers_build() {
        let changes = vec![change("crates/hooklings/Cargo.toml", ChangeKind::Modified)];
        assert_eq!(infer_type(&changes), CommitType::Build);
    }

    #[test]
    fn binary_changeset_falls_back_to_chore() {
        let mut changes = vec![change("assets/logo.png", ChangeKind::Added)];
        changes[0].binary = true;
        assert_eq!(infer_type(&changes), CommitType::Chore);
    }

    #[test]
    fn scope_uses_shared_parent_directory() {
        let changes = vec![
            change(
                "crates/hooklings/src/checkpoint/mod.rs",
                ChangeKind::Modified,
            ),
            change("crates/hooklings/src/main.rs", ChangeKind::Modified),
        ];
        assert_eq!(
            infer_scope(&changes).as_deref(),
            Some("crates/hooklings/src")
        );
    }

    #[test]
    fn scope_is_none_across_top_level_roots() {
        let changes = vec![
            change("src/main.rs", ChangeKind::Modified),
            change("docs/readme.md", ChangeKind::Modified),
        ];
        assert_eq!(infer_scope(&changes), None);
    }

    #[test]
    fn scope_is_none_for_root_level_file() {
        let changes = vec![change("README.md", ChangeKind::Modified)];
        assert_eq!(infer_scope(&changes), None);
    }

    #[test]
    fn subject_is_singular_for_one_file() {
        let changes = vec![change("src/main.rs", ChangeKind::Modified)];
        assert_eq!(
            subject(&changes, CommitType::Chore),
            "chore(src): update 1 file"
        );
    }

    #[test]
    fn subject_is_plural_with_scope() {
        let changes = vec![
            change("src/checkpoint/mod.rs", ChangeKind::Added),
            change("src/checkpoint/record.rs", ChangeKind::Added),
        ];
        assert_eq!(
            subject(&changes, CommitType::Feat),
            "feat(src/checkpoint): add 2 files"
        );
    }

    #[test]
    fn subject_prefers_removal_verb_when_anything_is_deleted() {
        let changes = vec![
            change("src/gone.rs", ChangeKind::Deleted),
            change("src/main.rs", ChangeKind::Modified),
        ];
        assert_eq!(
            subject(&changes, CommitType::Chore),
            "chore(src): remove 2 files"
        );
    }

    #[test]
    fn body_contains_marker_stats_and_provenance() {
        let mut first = change("src/main.rs", ChangeKind::Modified);
        first.added = 8;
        first.removed = 2;
        let mut second = change("src/new.rs", ChangeKind::Added);
        second.added = 40;

        let rendered = body(&[first, second], 5, "2026-10-02T00:00:00Z");

        assert!(rendered.contains("  M src/main.rs"), "{rendered}");
        assert!(rendered.contains("+8 -2"), "{rendered}");
        assert!(rendered.contains("  A src/new.rs"), "{rendered}");
        assert!(rendered.contains("+40 -0"), "{rendered}");
        assert!(
            rendered.contains("Coalesced checkpoint: 5 edit(s)"),
            "{rendered}"
        );
    }

    #[test]
    fn body_marks_binary_files_instead_of_fake_line_counts() {
        let mut binary = change("assets/logo.png", ChangeKind::Added);
        binary.binary = true;
        let rendered = body(&[binary], 1, "2026-10-02T00:00:00Z");
        assert!(rendered.contains("binary"), "{rendered}");
        assert!(!rendered.contains("+0 -0"), "{rendered}");
    }
}
