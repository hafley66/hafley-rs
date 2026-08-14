use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use soopy::{
    ContentId, GitFilesQuery, ObjectId, Pattern, ReadRequest, RepoPath, Revision, RevisionId,
    SourceQuery, SourceRef, SourceTree,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn unique(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "soopy_{}_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ))
}

fn git(root: &Path, args: &[&str]) -> String {
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
    String::from_utf8(output.stdout).unwrap()
}

fn git_stdin(root: &Path, args: &[&str], input: &[u8]) -> String {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "soopy")
        .env("GIT_AUTHOR_EMAIL", "soopy@example.invalid")
        .env("GIT_COMMITTER_NAME", "soopy")
        .env("GIT_COMMITTER_EMAIL", "soopy@example.invalid")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn set_mtime(path: &Path, time: std::time::SystemTime) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(time))
        .unwrap();
}

/// A repository with one commit tracking `src/lib.rs` and `README.md`, and a
/// dirty `src/lib.rs` in the working tree.
fn repository() -> std::path::PathBuf {
    let root = unique("repo");
    std::fs::create_dir_all(root.join("src")).unwrap();
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 1;\n").unwrap();
    std::fs::write(root.join("README.md"), "first\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "first"]);
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 2;\n").unwrap();
    root
}

#[test]
fn repo_path_cannot_escape_its_root() {
    let root = repository();
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let revision = tree.resolve_revision(Revision::Worktree).unwrap();
    let request = ReadRequest {
        source: SourceRef {
            repository: tree.repository().identity.clone(),
            revision,
            path: RepoPath(Arc::from("../../etc/passwd")),
        },
        expected: None,
    };
    assert!(tree.read_many(&[request]).is_err());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn commit_read_verifies_commit_path_against_expected_blob() {
    let root = repository();
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let head = tree
        .resolve_revision(Revision::Named(Arc::from("HEAD")))
        .unwrap();
    let entries = tree.enumerate(&head, &[Pattern("**/*".into())]).unwrap();
    assert_eq!(entries.len(), 2);
    let wrong = ReadRequest {
        source: SourceRef {
            repository: entries[0].source.repository.clone(),
            revision: entries[0].source.revision.clone(),
            path: entries[1].source.path.clone(),
        },
        expected: Some(entries[0].content.clone()),
    };
    assert!(tree.read_many(&[wrong]).is_err());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn worktree_git_files_round_trips_through_read_many() {
    let root = repository();
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let entries = tree
        .git_files(&GitFilesQuery {
            revision: Revision::Worktree,
            pathspecs: vec!["**/*.rs".into()],
        })
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].content, ContentId::GitBlob(_)));
    let request = ReadRequest {
        source: entries[0].source.clone(),
        expected: Some(entries[0].content.clone()),
    };
    let answers = tree.read_many(&[request]).unwrap();
    assert_eq!(answers[0].content, entries[0].content);
    assert_eq!(&*answers[0].bytes, b"pub const VALUE: u8 = 2;\n");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn worktree_cache_evicts_deleted_paths() {
    let root = repository();
    let path = root.join("evict.txt");
    std::fs::write(&path, "aaaa").unwrap();
    let fixed = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    set_mtime(&path, fixed);
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let query = SourceQuery {
        revision: Revision::Worktree,
        patterns: vec![Pattern("**/*.txt".into())],
    };

    let before = tree.snapshot(&query).unwrap();
    let before_content = before.files[0].content.clone();

    std::fs::remove_file(&path).unwrap();
    let mid = tree.snapshot(&query).unwrap();
    assert!(mid.files.is_empty());

    std::fs::write(&path, "bbbb").unwrap();
    set_mtime(&path, fixed);
    let after = tree.snapshot(&query).unwrap();
    assert_eq!(after.files.len(), 1);
    assert_ne!(after.files[0].content, before_content);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn tracked_symlinks_are_absent_from_both_worktree_and_commit() {
    let root = repository();
    std::os::unix::fs::symlink("src/lib.rs", root.join("link.rs")).unwrap();
    git(&root, &["add", "link.rs"]);
    git(&root, &["commit", "-qm", "link"]);
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let work = tree.resolve_revision(Revision::Worktree).unwrap();
    let head = tree
        .resolve_revision(Revision::Named(Arc::from("HEAD")))
        .unwrap();
    for revision in [work, head] {
        let entries = tree
            .enumerate(&revision, &[Pattern("**/*.rs".into())])
            .unwrap();
        assert!(entries
            .iter()
            .all(|entry| entry.source.path.0.as_ref() != "link.rs"));
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn linked_worktrees_share_repository_identity() {
    let root = repository();
    let linked = unique("linked");
    git(
        &root,
        &["worktree", "add", linked.to_str().unwrap(), "-b", "feature"],
    );
    let main = soopy::open(&root).unwrap();
    let linked_repo = soopy::open(&linked).unwrap();
    assert_eq!(main.identity, linked_repo.identity);
    assert_ne!(main.root, linked_repo.root);
    git(
        &root,
        &["worktree", "remove", "--force", linked.to_str().unwrap()],
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn failed_git_status_is_not_a_clean_worktree() {
    let root = repository();
    let repo = soopy::open(&root).unwrap();
    let tree = SourceTree::open(repo);
    std::fs::write(root.join(".git/index"), b"garbage").unwrap();
    assert!(tree.resolve_revision(Revision::Worktree).is_err());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn non_utf8_commit_path_is_rejected() {
    let root = repository();
    let blob = git_stdin(&root, &["hash-object", "-w", "--stdin"], b"x");
    let mut entry = format!("100644 blob {}\t", blob).into_bytes();
    entry.extend_from_slice(&[0xff, 0xfe]);
    entry.push(b'\n');
    let tree = git_stdin(&root, &["mktree"], &entry);
    let commit = git_stdin(&root, &["commit-tree", &tree, "-m", "nonutf8"], b"");
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let revision = RevisionId::Commit(ObjectId(Arc::from(commit)));
    assert!(tree
        .enumerate(&revision, &[Pattern("**/*".into())])
        .is_err());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn newline_bearing_path_is_rejected_by_git_files() {
    let root = repository();
    std::fs::write(root.join("src/line\nbreak.rs"), "pub const X: u8 = 1;\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "newline"]);
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let result = tree.git_files(&GitFilesQuery {
        revision: Revision::Worktree,
        pathspecs: vec!["**/*.rs".into()],
    });
    assert!(result.is_err());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn watcher_reports_an_index_only_change() {
    let root = repository();
    let repo = soopy::open(&root).unwrap();
    let tree = SourceTree::open(repo);
    let mut watcher = tree
        .watch(SourceQuery {
            revision: Revision::Worktree,
            patterns: vec![Pattern("**/*.rs".into())],
        })
        .unwrap();
    let blob = git_stdin(&root, &["hash-object", "-w", "--stdin"], b"staged");
    git(
        &root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{blob},ghost.txt"),
        ],
    );
    let deltas = watcher
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .expect("index write produced a watcher event");
    assert!(deltas
        .iter()
        .any(|delta| matches!(delta, soopy::SourceDelta::RescanRequired)));
    std::fs::remove_dir_all(&root).unwrap();
}
