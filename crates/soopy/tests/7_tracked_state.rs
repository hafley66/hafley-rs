use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use soopy::{
    EntryTransition, GitEntryKind, GitFileQuery, SourceRoot, TrackedFileState, UntrackedFilePolicy,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "soopy_tracked_state_{}_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "soopy")
        .env("GIT_AUTHOR_EMAIL", "soopy@example.invalid")
        .env("GIT_COMMITTER_NAME", "soopy")
        .env("GIT_COMMITTER_EMAIL", "soopy@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repository(name: &str) -> PathBuf {
    let root = root(name);
    git(&root, &["init", "-q"]);
    root
}

fn commit(root: &Path, path: &str, contents: &str) {
    std::fs::write(root.join(path), contents).unwrap();
    git(root, &["add", path]);
    git(root, &["commit", "-qm", "fixture"]);
}

fn observations(root: &Path, query: GitFileQuery) -> Vec<soopy::TrackedFileObservation> {
    let mut source = SourceRoot::discover_git(root).unwrap();
    source.git_mut().unwrap().tracked_state(&query).unwrap()
}

fn observation(root: &Path, path: &str) -> soopy::TrackedFileObservation {
    observations(
        root,
        GitFileQuery {
            pathspecs: vec![path.into()],
            ..GitFileQuery::default()
        },
    )
    .into_iter()
    .next()
    .unwrap()
}

#[test]
fn adjacent_transitions_distinguish_clean_unstaged_staged_and_both() {
    let root = repository("primary");
    commit(&root, "state.txt", "head\n");

    let clean = observation(&root, "state.txt");
    assert_eq!(clean.state, TrackedFileState::Clean);
    assert_eq!(clean.staged_change, Some(false));
    assert_eq!(clean.unstaged_change, Some(false));

    std::fs::write(root.join("state.txt"), "worktree\n").unwrap();
    let unstaged = observation(&root, "state.txt");
    assert_eq!(unstaged.state, TrackedFileState::Unstaged);
    assert_eq!(unstaged.staged_change, Some(false));
    assert_eq!(unstaged.unstaged_change, Some(true));
    assert_eq!(unstaged.index_to_worktree, Some(EntryTransition::Modified));

    git(&root, &["add", "state.txt"]);
    let staged = observation(&root, "state.txt");
    assert_eq!(staged.state, TrackedFileState::Staged);
    assert_eq!(staged.staged_change, Some(true));
    assert_eq!(staged.unstaged_change, Some(false));

    // HEAD and worktree match again, but the two adjacent transitions still
    // differ because the index holds the staged replacement.
    std::fs::write(root.join("state.txt"), "head\n").unwrap();
    let both = observation(&root, "state.txt");
    assert_eq!(both.state, TrackedFileState::StagedAndUnstaged);
    assert_eq!(both.staged_change, Some(true));
    assert_eq!(both.unstaged_change, Some(true));
    assert_eq!(both.head, both.worktree_entry);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn transitions_preserve_add_delete_mode_type_and_untracked_cases() {
    let root = repository("transitions");
    commit(&root, "delete.txt", "delete\n");
    commit(&root, "mode.txt", "mode\n");
    commit(&root, "type.txt", "type\n");

    std::fs::write(root.join("added.txt"), "added\n").unwrap();
    git(&root, &["add", "added.txt"]);
    let added = observation(&root, "added.txt");
    assert_eq!(added.state, TrackedFileState::Staged);
    assert_eq!(added.head_to_index, Some(EntryTransition::Added));

    git(&root, &["rm", "-q", "delete.txt"]);
    let deleted = observation(&root, "delete.txt");
    assert_eq!(deleted.state, TrackedFileState::Staged);
    assert_eq!(deleted.head_to_index, Some(EntryTransition::Deleted));

    std::fs::remove_file(root.join("mode.txt")).unwrap();
    std::fs::write(root.join("mode.txt"), "mode\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(root.join("mode.txt"))
            .unwrap()
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(root.join("mode.txt"), permissions).unwrap();
        let mode = observation(&root, "mode.txt");
        assert_eq!(mode.state, TrackedFileState::Unstaged);
        assert_eq!(mode.index_to_worktree, Some(EntryTransition::ModeChanged));
    }

    #[cfg(unix)]
    {
        std::fs::remove_file(root.join("type.txt")).unwrap();
        std::os::unix::fs::symlink("target", root.join("type.txt")).unwrap();
        let type_change = observation(&root, "type.txt");
        assert_eq!(type_change.state, TrackedFileState::Unstaged);
        assert_eq!(
            type_change.index_to_worktree,
            Some(EntryTransition::TypeModeAndContentChanged)
        );
        assert_eq!(
            type_change.worktree_entry.unwrap().kind,
            GitEntryKind::Symlink
        );
    }

    std::fs::write(root.join("untracked.txt"), "untracked\n").unwrap();
    let untracked = observations(
        &root,
        GitFileQuery {
            pathspecs: vec!["untracked.txt".into()],
            untracked: UntrackedFilePolicy::Include,
        },
    );
    assert_eq!(untracked.len(), 1);
    assert_eq!(untracked[0].state, TrackedFileState::Untracked);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_target_bytes_are_a_comparable_git_owned_identity() {
    let root = repository("symlink_clean");
    std::os::unix::fs::symlink("target bytes", root.join("link")).unwrap();
    git(&root, &["add", "link"]);
    git(&root, &["commit", "-qm", "symlink"]);
    let mut source = SourceRoot::discover_git(&root).unwrap();
    let result = source
        .git_mut()
        .unwrap()
        .tracked_state_with_metrics(&GitFileQuery::default())
        .unwrap();
    assert_eq!(result.observations.len(), 1);
    let link = &result.observations[0];
    assert_eq!(link.state, TrackedFileState::Clean);
    assert_eq!(link.head, link.worktree_entry);
    assert_eq!(
        link.worktree_entry.as_ref().unwrap().kind,
        GitEntryKind::Symlink
    );
    assert_eq!(link.worktree_entry.as_ref().unwrap().mode.0, 0o120000);
    assert_eq!(result.metrics.byte_worker_launches, 1);
    assert_eq!(result.metrics.bytes_hashed, b"target bytes".len() as u64);
    let warm = source
        .git_mut()
        .unwrap()
        .tracked_state_with_metrics(&GitFileQuery::default())
        .unwrap();
    assert_eq!(warm.metrics.byte_worker_launches, 0);
    assert_eq!(warm.metrics.bytes_hashed, 0);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn persistent_git_hash_worker_applies_clean_filters_and_crlf_normalization() {
    let root = repository("filters");
    git(&root, &["config", "filter.upper.clean", "tr a-z A-Z"]);
    git(&root, &["config", "core.autocrlf", "true"]);
    std::fs::write(
        root.join(".gitattributes"),
        "filtered.txt filter=upper\ncrlf.txt text eol=crlf\n",
    )
    .unwrap();
    std::fs::write(root.join("filtered.txt"), "lowercase\n").unwrap();
    std::fs::write(root.join("crlf.txt"), "line one\nline two\n").unwrap();
    git(
        &root,
        &["add", ".gitattributes", "filtered.txt", "crlf.txt"],
    );
    git(&root, &["commit", "-qm", "filtered fixture"]);
    std::fs::remove_file(root.join("crlf.txt")).unwrap();
    git(&root, &["checkout", "--", "crlf.txt"]);

    let rows = observations(&root, GitFileQuery::default());
    assert!(rows.iter().all(|row| row.state == TrackedFileState::Clean));
    let filtered = rows
        .iter()
        .find(|row| row.path.0.as_ref() == "filtered.txt")
        .unwrap();
    let crlf = rows
        .iter()
        .find(|row| row.path.0.as_ref() == "crlf.txt")
        .unwrap();
    assert_eq!(filtered.head, filtered.worktree_entry);
    assert_eq!(crlf.head, crlf.worktree_entry);
    assert!(std::fs::read(root.join("crlf.txt"))
        .unwrap()
        .windows(2)
        .any(|pair| pair == b"\r\n"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unborn_intent_to_add_unmerged_and_skip_worktree_are_typed() {
    let unborn = repository("unborn");
    std::fs::write(unborn.join("first.txt"), "first\n").unwrap();
    git(&unborn, &["add", "first.txt"]);
    let first = observation(&unborn, "first.txt");
    assert_eq!(first.state, TrackedFileState::Staged);
    assert!(matches!(first.head_state, soopy::TrackedHeadState::Unborn));
    std::fs::remove_dir_all(unborn).unwrap();

    let intent = repository("intent");
    commit(&intent, "base.txt", "base\n");
    std::fs::write(intent.join("intent.txt"), "intent\n").unwrap();
    git(&intent, &["add", "-N", "intent.txt"]);
    let intent_row = observation(&intent, "intent.txt");
    assert_eq!(intent_row.state, TrackedFileState::IntentToAdd);
    assert!(intent_row.index.is_some());
    assert!(intent_row.worktree_entry.is_some());
    std::fs::remove_dir_all(intent).unwrap();

    let conflict = repository("unmerged");
    commit(&conflict, "conflict.txt", "base\n");
    let base_branch = String::from_utf8(
        Command::new("git")
            .arg("-C")
            .arg(&conflict)
            .args(["symbolic-ref", "--short", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    git(&conflict, &["checkout", "-qb", "other"]);
    std::fs::write(conflict.join("conflict.txt"), "other\n").unwrap();
    git(&conflict, &["commit", "-am", "other"]);
    git(&conflict, &["checkout", "-q", base_branch.trim()]);
    std::fs::write(conflict.join("conflict.txt"), "master\n").unwrap();
    git(&conflict, &["commit", "-am", "master"]);
    let output = Command::new("git")
        .arg("-C")
        .arg(&conflict)
        .args(["merge", "other"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let unmerged = observation(&conflict, "conflict.txt");
    assert_eq!(unmerged.state, TrackedFileState::Unmerged);
    assert_eq!(unmerged.index_stages.len(), 3);
    assert!(unmerged.worktree_entry.is_some());
    std::fs::remove_dir_all(conflict).unwrap();

    let sparse = repository("skip");
    commit(&sparse, "skip.txt", "skip\n");
    git(&sparse, &["update-index", "--skip-worktree", "skip.txt"]);
    assert_eq!(
        observation(&sparse, "skip.txt").state,
        TrackedFileState::Sparse
    );
    std::fs::remove_dir_all(sparse).unwrap();
}

#[test]
fn gitlinks_keep_kind_and_return_a_typed_result_when_worktree_is_unavailable() {
    let nested = repository("gitlink_nested");
    commit(&nested, "nested.txt", "nested\n");
    let outer = repository("gitlink_outer");
    let output = Command::new("git")
        .arg("-C")
        .arg(&outer)
        .args([
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            nested.to_str().unwrap(),
            "sub",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    git(&outer, &["commit", "-qam", "submodule"]);
    let row = observation(&outer, "sub");
    assert_eq!(row.head.as_ref().unwrap().kind, GitEntryKind::Gitlink);
    assert!(matches!(
        row.state,
        TrackedFileState::Clean | TrackedFileState::Unsupported(_)
    ));
    std::fs::remove_dir_all(outer).unwrap();
    std::fs::remove_dir_all(nested).unwrap();
}

#[test]
fn metrics_report_actual_hashes_and_path_scoped_cache_invalidation() {
    let root = repository("metrics");
    commit(&root, "one.txt", "one\n");
    commit(&root, "two.txt", "two\n");
    let mut source = SourceRoot::discover_git(&root).unwrap();
    let git_root = source.git_mut().unwrap();

    let cold = git_root
        .tracked_state_with_metrics(&GitFileQuery::default())
        .unwrap();
    assert_eq!(cold.metrics.worktree_cache_hits, 0);
    assert_eq!(cold.metrics.worktree_cache_misses, 2);
    assert_eq!(cold.metrics.bytes_hashed, 8);
    assert_eq!(cold.metrics.hash_worker_launches, 1);

    let warm = git_root
        .tracked_state_with_metrics(&GitFileQuery::default())
        .unwrap();
    assert_eq!(warm.metrics.worktree_cache_hits, 2);
    assert_eq!(warm.metrics.worktree_cache_misses, 0);
    assert_eq!(warm.metrics.bytes_hashed, 0);
    assert_eq!(warm.metrics.hash_worker_launches, 0);
    assert!(warm.metrics.git_child_processes < cold.metrics.git_child_processes);

    std::fs::write(root.join("one.txt"), "next\n").unwrap();
    let worktree_changed = git_root
        .tracked_state_with_metrics(&GitFileQuery::default())
        .unwrap();
    assert_eq!(worktree_changed.metrics.worktree_cache_hits, 1);
    assert_eq!(worktree_changed.metrics.worktree_cache_misses, 1);
    assert_eq!(worktree_changed.metrics.bytes_hashed, 5);
    assert_eq!(worktree_changed.metrics.hash_worker_launches, 0);

    git(&root, &["add", "one.txt"]);
    let index_changed = git_root
        .tracked_state_with_metrics(&GitFileQuery::default())
        .unwrap();
    assert_eq!(index_changed.metrics.worktree_cache_hits, 1);
    assert_eq!(index_changed.metrics.worktree_cache_misses, 1);
    assert_eq!(index_changed.metrics.bytes_hashed, 5);
    assert_eq!(index_changed.metrics.hash_worker_launches, 0);
    std::fs::remove_dir_all(root).unwrap();
}
