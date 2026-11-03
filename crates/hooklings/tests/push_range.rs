//! Integration tests for `push_range::resolve`, the public entry point that reads git's
//! pre-push stdin ref lines. `push_range.rs`'s own `#[cfg(test)]` module covers the
//! private helpers (`parse_line`, `base_for_new_branch`) directly; these exercise the
//! public function end to end.

use hooklings::push_range::resolve;
use std::io::Cursor;

/// Initializes a temp repo on `branch`, commits an empty tree, and returns
/// `(dir, repo, commit_id)` — the dir must be kept alive for the repo's lifetime.
fn init_on(branch: &str) -> (tempfile::TempDir, gix::Repository, gix::ObjectId) {
    let dir = tempfile::tempdir().unwrap();
    let repo = gix::init(dir.path()).unwrap();

    std::fs::write(
        repo.git_dir().join("config"),
        "\n[user]\n\tname = Push Range Test\n\temail = test@example.invalid\n",
    )
    .unwrap();

    std::fs::write(
        repo.git_dir().join("HEAD"),
        format!("ref: refs/heads/{branch}\n"),
    )
    .unwrap();

    let tree = repo.empty_tree();
    let tree_id = tree.id().detach();
    let parents: [gix::ObjectId; 0] = [];
    let commit_id = repo
        .commit("HEAD", "initial", tree_id, parents)
        .unwrap()
        .detach();
    drop(tree);

    (dir, repo, commit_id)
}

#[test]
fn deletion_line_yields_no_ranges() {
    let (_dir, repo, tip) = init_on("main");
    let null = gix::ObjectId::null(repo.object_hash());
    let line = format!("refs/heads/gone {null} refs/heads/gone {tip}\n");

    let ranges = resolve(&repo, Cursor::new(line), "main").unwrap();

    assert!(ranges.is_empty());
}

#[test]
fn existing_branch_update_uses_remote_oid_as_base() {
    let (_dir, repo, tip) = init_on("feature");
    let remote_oid = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
    let line = format!("refs/heads/feature {tip} refs/heads/feature {remote_oid}\n");

    let ranges = resolve(&repo, Cursor::new(line), "main").unwrap();

    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].tip, tip);
    assert_eq!(ranges[0].base.to_string(), remote_oid);
}

#[test]
fn new_branch_with_no_main_falls_back_to_root_commit() {
    let (_dir, repo, root) = init_on("feature");
    let null = gix::ObjectId::null(repo.object_hash());
    let line = format!("refs/heads/feature {root} refs/heads/feature {null}\n");

    let ranges = resolve(&repo, Cursor::new(line), "main").unwrap();

    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].tip, root);
    assert_eq!(ranges[0].base, root);
}

#[test]
fn multiple_ref_lines_each_resolve_independently() {
    let (_dir, repo, tip) = init_on("main");
    let null = gix::ObjectId::null(repo.object_hash());
    let remote_oid = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
    let input = format!(
        "refs/heads/a {tip} refs/heads/a {remote_oid}\n\
         refs/heads/b {null} refs/heads/b {tip}\n\
         malformed line\n"
    );

    let ranges = resolve(&repo, Cursor::new(input), "main").unwrap();

    // One real update (a), one deletion (b, skipped), one malformed line (ignored).
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].tip, tip);
}

#[test]
fn display_format_is_base_dot_dot_tip() {
    let (_dir, repo, tip) = init_on("main");
    let remote_oid = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
    let line = format!("refs/heads/main {tip} refs/heads/main {remote_oid}\n");

    let ranges = resolve(&repo, Cursor::new(line), "main").unwrap();

    assert_eq!(ranges[0].to_string(), format!("{remote_oid}..{tip}"));
}
