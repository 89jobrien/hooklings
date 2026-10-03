//! End-to-end checkpoint behaviour against real temporary repositories.

use gix::bstr::ByteSlice;

use hooklings::checkpoint::{self, Outcome, RefuseReason, RejectionReason, Request, SkipReason};
use hooklings::config::CheckpointConfig;

struct Fixture {
    _dir: tempfile::TempDir,
    repo: gix::Repository,
}

fn init_on(branch: &str) -> Fixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut repo = gix::init(dir.path()).expect("init repo");

    std::fs::write(
        repo.git_dir().join("config"),
        "\n[user]\n\tname = Checkpoint Test\n\temail = test@example.invalid\n",
    )
    .expect("write user config");
    repo.config_snapshot_mut();

    std::fs::write(
        repo.git_dir().join("HEAD"),
        format!("ref: refs/heads/{branch}\n"),
    )
    .expect("write HEAD");

    let tree = repo.empty_tree();
    let tree_id = tree.id().detach();
    let parents: [gix::hash::ObjectId; 0] = [];
    repo.commit("HEAD", "initial", tree_id, parents)
        .expect("initial commit");
    drop(tree);

    Fixture { _dir: dir, repo }
}

fn enabled_config() -> CheckpointConfig {
    CheckpointConfig {
        enabled: true,
        ..Default::default()
    }
}

fn request<'a>(config: &'a CheckpointConfig) -> Request<'a> {
    Request {
        config,
        subject_override: None,
        type_override: None,
        scope_override: None,
        dry_run: false,
    }
}

fn write(root: &std::path::Path, path: &str, contents: &str) {
    let full = root.join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(full, contents).expect("write file");
}

fn tracked_paths(repo: &gix::Repository) -> Vec<String> {
    let tree = repo.head_tree().expect("head tree");
    let blob_mode = gix::objs::tree::EntryMode::from(gix::objs::tree::EntryKind::Blob);
    let mut paths: Vec<String> = tree
        .traverse()
        .breadthfirst
        .files()
        .expect("traverse")
        .iter()
        .filter(|entry| entry.mode == blob_mode)
        .map(|entry| entry.filepath.to_string())
        .collect();
    paths.sort();
    paths
}

fn head_subject(repo: &gix::Repository) -> String {
    let commit = repo.head_commit().expect("head commit");
    let message = commit.message().expect("decode message");
    format!(
        "{}\n\n{}",
        String::from_utf8_lossy(message.title),
        String::from_utf8_lossy(message.body.unwrap_or_default().as_bytes())
    )
}

/// Stages a path without touching anything else, to simulate a concurrent writer.
fn stage(repo: &gix::Repository, path: &str, contents: &str) {
    let mut index = repo
        .index_or_load_from_head_or_empty()
        .expect("index")
        .into_owned();
    let id = repo.write_blob(contents).expect("blob").detach();
    let bpath = gix::bstr::BString::from(path);

    index.dangerously_push_entry(
        gix::index::entry::Stat::default(),
        id,
        gix::index::entry::Flags::empty(),
        gix::index::entry::Mode::FILE,
        bpath.as_ref(),
    );
    index.sort_entries();
    index
        .write(gix::index::write::Options::default())
        .expect("write index");
}

#[test]
fn commits_recorded_added_file_as_one_coalesced_commit() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, "src/alpha.rs", "fn alpha() {}\n");
    write(&root, "src/beta.rs", "fn beta() {}\n");

    checkpoint::record_paths(
        &fixture.repo,
        &["src/alpha.rs".into(), "src/beta.rs".into()],
        "t0",
    )
    .expect("record");

    let report = checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");

    let Outcome::Committed { subject, files, .. } = &report.outcome else {
        panic!("expected a commit, got {:?}", report.outcome);
    };

    assert_eq!(*files, 2, "both edits coalesce into one commit");
    assert!(subject.starts_with("feat(src): add 2 files"), "{subject}");
    assert_eq!(
        tracked_paths(&fixture.repo),
        ["src/alpha.rs", "src/beta.rs"]
    );

    let message = head_subject(&fixture.repo);
    assert!(message.contains("feat(src): add 2 files"), "{message}");
    assert!(message.contains("A src/alpha.rs"), "{message}");
    assert!(message.contains("Coalesced checkpoint"), "{message}");
}

#[test]
fn clears_the_record_after_committing() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, "notes.md", "hello\n");
    checkpoint::record_paths(&fixture.repo, &["notes.md".into()], "t0").expect("record");
    checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");

    let record = checkpoint::load_record(&fixture.repo).expect("record");
    assert!(record.paths.is_empty(), "record is cleared after a commit");
}

#[test]
fn refuses_to_commit_on_a_protected_branch() {
    let fixture = init_on("main");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, "notes.md", "hello\n");
    checkpoint::record_paths(&fixture.repo, &["notes.md".into()], "t0").expect("record");

    let report = checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");

    assert_eq!(
        report.outcome,
        Outcome::Refused(RefuseReason::OnProtectedBranch {
            branch: "main".into()
        })
    );
    assert!(
        tracked_paths(&fixture.repo).is_empty(),
        "nothing is committed on a protected branch"
    );
}

#[test]
fn skips_when_checkpointing_is_disabled() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, "notes.md", "hello\n");
    checkpoint::record_paths(&fixture.repo, &["notes.md".into()], "t0").expect("record");

    let config = CheckpointConfig::default();
    let report = checkpoint::commit::run(&fixture.repo, &request(&config)).expect("run");

    assert_eq!(report.outcome, Outcome::Skipped(SkipReason::Disabled));
}

#[test]
fn skips_when_recorded_paths_are_unchanged() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, "notes.md", "hello\n");
    checkpoint::record_paths(&fixture.repo, &["notes.md".into()], "t0").expect("record");
    checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");

    checkpoint::record_paths(&fixture.repo, &["notes.md".into()], "t1").expect("record");
    let report = checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");

    assert_eq!(report.outcome, Outcome::Skipped(SkipReason::NothingChanged));
}

#[test]
fn skips_when_nothing_has_been_recorded() {
    let fixture = init_on("feature/checkpoint");
    let report = checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");
    assert_eq!(report.outcome, Outcome::Skipped(SkipReason::NoRecord));
}

#[test]
fn dry_run_reports_without_committing() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, "notes.md", "hello\n");
    checkpoint::record_paths(&fixture.repo, &["notes.md".into()], "t0").expect("record");

    let config = enabled_config();
    let request = Request {
        dry_run: true,
        ..request(&config)
    };
    let report = checkpoint::commit::run(&fixture.repo, &request).expect("run");

    assert!(matches!(report.outcome, Outcome::Planned { .. }));
    assert!(
        tracked_paths(&fixture.repo).is_empty(),
        "dry run commits nothing"
    );

    let record = checkpoint::load_record(&fixture.repo).expect("record");
    assert_eq!(record.paths.len(), 1, "dry run keeps the record intact");
}

#[test]
fn refuses_when_another_writer_already_staged_a_path() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, "ours.md", "ours\n");
    checkpoint::record_paths(&fixture.repo, &["ours.md".into()], "t0").expect("record");
    stage(&fixture.repo, "theirs.md", "theirs\n");

    let report = checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");

    assert_eq!(
        report.outcome,
        Outcome::Refused(RefuseReason::ForeignStaged {
            paths: vec!["theirs.md".into()]
        })
    );
    assert!(
        tracked_paths(&fixture.repo).is_empty(),
        "a foreign staged path blocks the whole checkpoint"
    );
}

#[test]
fn refuses_when_the_changeset_exceeds_the_file_cap() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    let paths: Vec<String> = (0..4).map(|index| format!("file{index}.txt")).collect();
    for path in &paths {
        write(&root, path, "content\n");
    }
    checkpoint::record_paths(&fixture.repo, &paths, "t0").expect("record");

    let config = CheckpointConfig {
        enabled: true,
        max_files: 2,
        ..Default::default()
    };
    let report = checkpoint::commit::run(&fixture.repo, &request(&config)).expect("run");

    assert_eq!(
        report.outcome,
        Outcome::Refused(RefuseReason::TooManyFiles { count: 4, limit: 2 })
    );
}

#[test]
fn rejects_paths_that_escape_the_repository() {
    let fixture = init_on("feature/checkpoint");

    let added = checkpoint::record_paths(
        &fixture.repo,
        &[
            "../escape.txt".into(),
            "/etc/passwd".into(),
            ".git/config".into(),
        ],
        "t0",
    )
    .expect("record");

    assert!(added.is_empty(), "hostile paths are never recorded");
    assert!(
        checkpoint::load_record(&fixture.repo)
            .expect("record")
            .paths
            .is_empty()
    );
}

#[test]
fn rejects_paths_that_are_gitignored() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, ".gitignore", "ignored.txt\n");
    write(&root, "ignored.txt", "secret\n");
    checkpoint::record_paths(&fixture.repo, &["ignored.txt".into()], "t0").expect("record");

    let report = checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");

    assert_eq!(report.outcome, Outcome::Skipped(SkipReason::NothingChanged));
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].path, "ignored.txt");
    assert_eq!(report.rejected[0].reason, RejectionReason::Ignored);
    assert!(
        tracked_paths(&fixture.repo).is_empty(),
        "a gitignored path never reaches the commit"
    );
}

#[test]
fn commits_modifications_and_deletions_together() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, "keep.md", "before\n");
    write(&root, "drop.md", "doomed\n");
    checkpoint::record_paths(&fixture.repo, &["keep.md".into(), "drop.md".into()], "t0")
        .expect("record");
    checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");

    write(&root, "keep.md", "after\n");
    std::fs::remove_file(root.join("drop.md")).expect("remove");

    checkpoint::record_paths(&fixture.repo, &["keep.md".into(), "drop.md".into()], "t1")
        .expect("record");
    let report = checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");

    assert!(matches!(
        report.outcome,
        Outcome::Committed { files: 2, .. }
    ));
    assert_eq!(tracked_paths(&fixture.repo), ["keep.md"]);

    let message = head_subject(&fixture.repo);
    assert!(message.contains("D drop.md"), "{message}");
}

#[test]
fn refuses_executable_files_rather_than_dropping_the_mode_bit() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, "script.sh", "#!/bin/sh\necho hi\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            root.join("script.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("chmod");
    }

    checkpoint::record_paths(&fixture.repo, &["script.sh".into()], "t0").expect("record");
    let report = checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");

    assert_eq!(report.outcome, Outcome::Skipped(SkipReason::NothingChanged));
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].reason, RejectionReason::UnsupportedMode);
    assert!(
        tracked_paths(&fixture.repo).is_empty(),
        "an executable file is never silently downgraded to 0o100644"
    );
}

#[test]
fn refuses_symlinks_rather_than_committing_them_as_regular_files() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, "real.md", "real\n");
    #[cfg(unix)]
    std::os::unix::fs::symlink("real.md", root.join("link.md")).expect("symlink");

    checkpoint::record_paths(&fixture.repo, &["link.md".into()], "t0").expect("record");
    let report = checkpoint::commit::run(&fixture.repo, &request(&enabled_config())).expect("run");

    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].reason, RejectionReason::UnsupportedMode);
    assert!(
        tracked_paths(&fixture.repo).is_empty(),
        "a symlink is never committed as a regular file"
    );
}

#[test]
fn subject_override_replaces_inference() {
    let fixture = init_on("feature/checkpoint");
    let root = fixture.repo.workdir().expect("workdir").to_path_buf();

    write(&root, "bug.rs", "fn main() {}\n");
    checkpoint::record_paths(&fixture.repo, &["bug.rs".into()], "t0").expect("record");

    let config = enabled_config();
    let request = Request {
        subject_override: Some("fix(parser): handle empty input".into()),
        type_override: Some(hooklings::checkpoint::CommitType::Fix),
        ..request(&config)
    };
    let report = checkpoint::commit::run(&fixture.repo, &request).expect("run");

    let Outcome::Committed { subject, .. } = &report.outcome else {
        panic!("expected a commit, got {:?}", report.outcome);
    };
    assert_eq!(subject, "fix(parser): handle empty input");
}
