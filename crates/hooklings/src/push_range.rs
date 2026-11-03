//! Resolves the commit range(s) being pushed, for downstream tools (secret
//! scanners, diff linters) to inspect.
//!
//! Ported from the shell snippet in `templates/git-hooks/resolve-push-range.sh`
//! (originally minibox's `.githooks/pre-push`) so any repo using hooklings gets
//! it for free via `hooklings resolve-push-range`, with no shell script to copy.
//!
//! Git invokes a pre-push hook with one line per ref being pushed on stdin:
//! `<local_ref> <local_oid> <remote_ref> <remote_oid>`. Range resolution rules:
//!
//! - `local_oid` is the null OID: a deletion, nothing to inspect.
//! - `remote_oid` is the null OID: a new branch; the base is the merge-base
//!   against `main` (falling back to the repository's root commit if `main`
//!   does not resolve).
//! - otherwise: an existing branch update; the base is `remote_oid`.

use std::io::BufRead;

use gix::ObjectId;

/// One resolved push range, printed as `base..tip`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushRange {
    pub base: ObjectId,
    pub tip: ObjectId,
}

impl std::fmt::Display for PushRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.base, self.tip)
    }
}

/// A single `<local_ref> <local_oid> <remote_ref> <remote_oid>` line from git's
/// pre-push stdin.
struct RefUpdate {
    local_oid: ObjectId,
    remote_oid: ObjectId,
}

fn parse_line(line: &str) -> anyhow::Result<Option<RefUpdate>> {
    let mut fields = line.split_whitespace();
    let (Some(_local_ref), Some(local_oid), Some(_remote_ref), Some(remote_oid)) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return Ok(None);
    };

    Ok(Some(RefUpdate {
        local_oid: ObjectId::from_hex(local_oid.as_bytes())
            .map_err(|e| anyhow::anyhow!("invalid local oid {local_oid:?}: {e}"))?,
        remote_oid: ObjectId::from_hex(remote_oid.as_bytes())
            .map_err(|e| anyhow::anyhow!("invalid remote oid {remote_oid:?}: {e}"))?,
    }))
}

/// Returns the repository's root commit reachable from `tip`, i.e. the oldest
/// ancestor with no parents — the fallback base when `main` does not exist.
/// Mirrors `git rev-list --max-parents=0 HEAD`.
fn root_commit(repo: &gix::Repository, tip: ObjectId) -> anyhow::Result<ObjectId> {
    for info in repo.rev_walk([tip]).all()? {
        let info = info?;
        if info.parent_ids.is_empty() {
            return Ok(info.id);
        }
    }
    anyhow::bail!("no root commit reachable from {tip}")
}

/// Resolves the merge-base of `tip` against `main_branch`, falling back to
/// `tip`'s root commit if `main_branch` does not resolve to a commit.
fn base_for_new_branch(
    repo: &gix::Repository,
    tip: ObjectId,
    main_branch: &str,
) -> anyhow::Result<ObjectId> {
    match repo.rev_parse_single(main_branch) {
        Ok(main_id) => Ok(repo.merge_base(tip, main_id.detach())?.detach()),
        Err(_) => root_commit(repo, tip),
    }
}

/// Reads git's pre-push stdin ref lines from `input` and returns one resolved
/// [`PushRange`] per non-deletion ref, against `repo`. `main_branch` is the
/// branch name used to compute the merge-base for newly pushed branches
/// (typically `"main"`).
pub fn resolve<R: BufRead>(
    repo: &gix::Repository,
    input: R,
    main_branch: &str,
) -> anyhow::Result<Vec<PushRange>> {
    let hash = repo.object_hash();
    let null = ObjectId::null(hash);

    let mut ranges = Vec::new();
    for line in input.lines() {
        let line = line?;
        let Some(update) = parse_line(&line)? else {
            continue;
        };

        if update.local_oid == null {
            // Branch deletion — nothing to inspect.
            continue;
        }

        let base = if update.remote_oid == null {
            base_for_new_branch(repo, update.local_oid, main_branch)?
        } else {
            update.remote_oid
        };

        ranges.push(PushRange {
            base,
            tip: update.local_oid,
        });
    }

    Ok(ranges)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn deletion_line_is_skipped() {
        let null = ObjectId::null(gix::hash::Kind::Sha1);
        let line =
            format!("refs/heads/x {null} refs/heads/x deadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
        let parsed = parse_line(&line).expect("parses").expect("some");
        assert_eq!(parsed.local_oid, null);
    }

    #[test]
    fn malformed_line_is_ignored() {
        let parsed = parse_line("not enough fields").expect("parses");
        assert!(parsed.is_none());
    }

    /// Initializes a temp repo on `branch`, commits an empty tree, and returns
    /// `(dir, repo, commit_id)` — the dir must be kept alive for the repo's lifetime.
    fn init_on(branch: &str) -> (tempfile::TempDir, gix::Repository, ObjectId) {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut repo = gix::init(dir.path()).expect("init repo");

        std::fs::write(
            repo.git_dir().join("config"),
            "\n[user]\n\tname = Push Range Test\n\temail = test@example.invalid\n",
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
        let parents: [ObjectId; 0] = [];
        let commit_id = repo
            .commit("HEAD", "initial", tree_id, parents)
            .expect("initial commit")
            .detach();
        drop(tree);

        (dir, repo, commit_id)
    }

    #[test]
    fn existing_branch_base_is_remote_oid() {
        let (_dir, repo, tip) = init_on("feature");
        let remote_oid = ObjectId::from_hex(b"deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
            .expect("valid remote oid");

        let base = if remote_oid == ObjectId::null(repo.object_hash()) {
            base_for_new_branch(&repo, tip, "main").expect("base")
        } else {
            remote_oid
        };

        assert_eq!(base, remote_oid);
    }

    #[test]
    fn new_branch_with_main_present_uses_merge_base() {
        let (_dir, repo, root) = init_on("main");

        // A second commit on `main`, then a divergent commit on `feature` from the
        // same root — the merge-base of `feature`'s tip against `main` is `root`.
        let main_tree = repo.empty_tree().id().detach();
        let main_tip = repo
            .commit("refs/heads/main", "on main", main_tree, [root])
            .expect("commit on main")
            .detach();

        let feature_tree = repo.empty_tree().id().detach();
        let feature_tip = repo
            .commit("refs/heads/feature", "on feature", feature_tree, [root])
            .expect("commit on feature")
            .detach();

        let base = base_for_new_branch(&repo, feature_tip, "main").expect("base resolves");

        assert_eq!(base, root);
        assert_ne!(base, main_tip);
    }

    #[test]
    fn new_branch_with_no_main_falls_back_to_root_commit() {
        let (_dir, repo, root) = init_on("feature");

        let base = base_for_new_branch(&repo, root, "main").expect("base resolves");

        assert_eq!(base, root);
    }
}
