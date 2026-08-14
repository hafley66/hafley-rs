use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use soopy::{ContentId, GitFilesQuery, Pattern, ReadRequest, Revision, SourceQuery, SourceTree};

static NEXT_REPOSITORY: AtomicU64 = AtomicU64::new(0);

fn repository() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "source_tree_{}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT_REPOSITORY.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir_all(root.join("src")).unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "source-tree")
            .env("GIT_AUTHOR_EMAIL", "source-tree@example.invalid")
            .env("GIT_COMMITTER_NAME", "source-tree")
            .env("GIT_COMMITTER_EMAIL", "source-tree@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-q"]);
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 1;\n").unwrap();
    std::fs::write(root.join("README.md"), "first\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "first"]);
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 2;\n").unwrap();
    root
}

#[test]
fn worktree_and_head_are_distinct_sources() {
    let root = repository();
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let patterns = [Pattern("**/*.rs".into())];
    let work = tree.resolve_revision(Revision::Worktree).unwrap();
    let head = tree
        .resolve_revision(Revision::Named(Arc::from("HEAD")))
        .unwrap();
    let work_entries = tree.enumerate(&work, &patterns).unwrap();
    let head_entries = tree.enumerate(&head, &patterns).unwrap();
    assert_eq!(work_entries.len(), 1);
    assert_eq!(head_entries.len(), 1);
    assert!(matches!(work_entries[0].content, ContentId::Blake3(_)));
    assert!(matches!(head_entries[0].content, ContentId::GitBlob(_)));
    let requests = [ReadRequest {
        source: head_entries[0].source.clone(),
        expected: Some(head_entries[0].content.clone()),
    }];
    let bytes = tree.read_many(&requests).unwrap();
    assert_eq!(&*bytes[0].bytes, b"pub const VALUE: u8 = 1;\n");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn read_each_streams_worktree_and_commit_bytes() {
    let root = repository();
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let patterns = [Pattern("**/*.rs".into())];
    let worktree = tree.resolve_revision(Revision::Worktree).unwrap();
    let commit = tree
        .resolve_revision(Revision::Named(Arc::from("HEAD")))
        .unwrap();
    let worktree_entry = tree.enumerate(&worktree, &patterns).unwrap().pop().unwrap();
    let commit_entry = tree.enumerate(&commit, &patterns).unwrap().pop().unwrap();
    let requests = [
        ReadRequest {
            source: worktree_entry.source,
            expected: Some(worktree_entry.content),
        },
        ReadRequest {
            source: commit_entry.source,
            expected: Some(commit_entry.content),
        },
    ];
    let mut buffer = Vec::new();
    let mut bytes = Vec::new();
    tree.read_each(&requests, &mut buffer, |answer| {
        bytes.push((answer.content.clone(), answer.bytes.to_vec()));
        Ok(())
    })
    .unwrap();
    assert!(matches!(bytes[0].0, ContentId::Blake3(_)));
    assert_eq!(bytes[0].1, b"pub const VALUE: u8 = 2;\n");
    assert!(matches!(bytes[1].0, ContentId::GitBlob(_)));
    assert_eq!(bytes[1].1, b"pub const VALUE: u8 = 1;\n");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn read_each_reuses_the_caller_buffer() {
    let root = repository();
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let revision = tree.resolve_revision(Revision::Worktree).unwrap();
    let entry = tree
        .enumerate(&revision, &[Pattern("**/*.rs".into())])
        .unwrap()
        .pop()
        .unwrap();
    let request = ReadRequest {
        source: entry.source,
        expected: Some(entry.content),
    };
    let mut buffer = Vec::with_capacity(64);
    let pointer = buffer.as_ptr();
    for _ in 0..2 {
        tree.read_each(std::slice::from_ref(&request), &mut buffer, |answer| {
            assert_eq!(answer.bytes.as_ptr(), pointer);
            assert_eq!(answer.bytes, b"pub const VALUE: u8 = 2;\n");
            Ok(())
        })
        .unwrap();
        assert_eq!(buffer.as_ptr(), pointer);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn read_each_propagates_callback_failure_and_keeps_git_batch_aligned() {
    let root = repository();
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let revision = tree
        .resolve_revision(Revision::Named(Arc::from("HEAD")))
        .unwrap();
    let entry = tree
        .enumerate(&revision, &[Pattern("**/*.rs".into())])
        .unwrap()
        .pop()
        .unwrap();
    let request = ReadRequest {
        source: entry.source,
        expected: Some(entry.content),
    };
    let mut buffer = Vec::new();
    let error = tree
        .read_each(std::slice::from_ref(&request), &mut buffer, |_| {
            anyhow::bail!("stop streaming")
        })
        .unwrap_err();
    assert!(error.to_string().contains("stop streaming"));
    tree.read_each(std::slice::from_ref(&request), &mut buffer, |answer| {
        assert_eq!(answer.bytes, b"pub const VALUE: u8 = 1;\n");
        Ok(())
    })
    .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn snapshot_derives_directories_and_prunes_nested_repositories() {
    let root = repository();
    std::fs::create_dir_all(root.join("vendor/inner/src")).unwrap();
    let output = Command::new("git")
        .arg("-C")
        .arg(root.join("vendor/inner"))
        .args(["init", "-q"])
        .output()
        .unwrap();
    assert!(output.status.success());
    std::fs::write(
        root.join("vendor/inner/src/nope.rs"),
        "pub const NOPE: u8 = 0;\n",
    )
    .unwrap();
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let snapshot = tree
        .snapshot(&SourceQuery {
            revision: Revision::Worktree,
            patterns: vec![Pattern("**/*.rs".into())],
        })
        .unwrap();
    let paths: Vec<_> = snapshot
        .files
        .iter()
        .map(|entry| entry.source.path.0.as_ref())
        .collect();
    assert_eq!(paths, ["src/lib.rs"]);
    let directories: Vec<_> = snapshot
        .directories
        .iter()
        .map(|entry| entry.path.0.as_ref())
        .collect();
    assert_eq!(directories, ["src"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn racy_same_second_same_size_worktree_rewrite_rehashes() {
    let root = repository();
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let query = SourceQuery {
        revision: Revision::Worktree,
        patterns: vec![Pattern("**/*.rs".into())],
    };
    let before = tree.snapshot(&query).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 3;\n").unwrap();
    let after = tree.snapshot(&query).unwrap();
    assert_ne!(before.files[0].content, after.files[0].content);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn one_session_reads_blobs_from_two_commit_revisions() {
    let root = repository();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "source-tree")
            .env("GIT_AUTHOR_EMAIL", "source-tree@example.invalid")
            .env("GIT_COMMITTER_NAME", "source-tree")
            .env("GIT_COMMITTER_EMAIL", "source-tree@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["add", "src/lib.rs"]);
    git(&["commit", "-qm", "second"]);
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let patterns = [Pattern("**/*.rs".into())];
    let first = tree
        .resolve_revision(Revision::Named(Arc::from("HEAD~1")))
        .unwrap();
    let second = tree
        .resolve_revision(Revision::Named(Arc::from("HEAD")))
        .unwrap();
    let first_entry = tree.enumerate(&first, &patterns).unwrap().pop().unwrap();
    let second_entry = tree.enumerate(&second, &patterns).unwrap().pop().unwrap();
    let answers = tree
        .read_many(&[
            ReadRequest {
                source: first_entry.source,
                expected: Some(first_entry.content),
            },
            ReadRequest {
                source: second_entry.source,
                expected: Some(second_entry.content),
            },
        ])
        .unwrap();
    assert_eq!(&*answers[0].bytes, b"pub const VALUE: u8 = 1;\n");
    assert_eq!(&*answers[1].bytes, b"pub const VALUE: u8 = 2;\n");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn watcher_reports_a_worktree_content_change() {
    let root = repository();
    let repo = soopy::open(&root).unwrap();
    let tree = SourceTree::open(repo);
    let mut watcher = tree
        .watch(SourceQuery {
            revision: Revision::Worktree,
            patterns: vec![Pattern("**/*.rs".into())],
        })
        .unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 4;\n").unwrap();
    let deltas = watcher
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .expect("source write produced watcher event");
    assert!(deltas.iter().any(|delta| matches!(delta, soopy::SourceDelta::Changed { after, .. } if after.source.path.0.as_ref() == "src/lib.rs")));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_files_keeps_git_pathspec_and_tracked_only_semantics() {
    let root = repository();
    std::fs::write(
        root.join("src/untracked.rs"),
        "pub const UNTRACKED: u8 = 9;\n",
    )
    .unwrap();
    let repo = soopy::open(&root).unwrap();
    let mut tree = SourceTree::open(repo);
    let worktree = tree
        .git_files(&GitFilesQuery {
            revision: Revision::Worktree,
            pathspecs: vec!["**/*.rs".to_string()],
        })
        .unwrap();
    assert_eq!(worktree.len(), 1);
    assert_eq!(worktree[0].source.path.0.as_ref(), "src/lib.rs");
    assert!(matches!(worktree[0].content, ContentId::GitBlob(_)));
    let head = tree
        .git_files(&GitFilesQuery {
            revision: Revision::Named(Arc::from("HEAD")),
            pathspecs: vec!["**/*.rs".to_string()],
        })
        .unwrap();
    assert_eq!(head.len(), 1);
    assert_eq!(head[0].source.path.0.as_ref(), "src/lib.rs");
    assert!(matches!(head[0].content, ContentId::GitBlob(_)));
    std::fs::remove_dir_all(root).unwrap();
}
