use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use soopy::{
    Head, Pattern, RefDelta, RefQuery, RepositoryDelta, Revision, SourceDelta, SourceQuery,
    SourceTree, WatchCoalescing, WatchQuery, WorktreeDelta,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn unique(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "soopy_watch_{}_{}_{}_{}",
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
        .env("GIT_AUTHOR_NAME", "soopy-watch")
        .env("GIT_AUTHOR_EMAIL", "soopy-watch@example.invalid")
        .env("GIT_COMMITTER_NAME", "soopy-watch")
        .env("GIT_COMMITTER_EMAIL", "soopy-watch@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn repository() -> PathBuf {
    let root = unique("repository");
    std::fs::create_dir_all(root.join("src")).unwrap();
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 1;\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "first"]);
    root
}

fn query(tree: &SourceTree, index: bool, linked_worktrees: bool) -> WatchQuery {
    WatchQuery {
        source: Some(SourceQuery {
            revision: Revision::Worktree,
            patterns: vec![Pattern("**/*.rs".into())],
        }),
        refs: Some(RefQuery {
            repository: tree.repository().identity.clone(),
            namespace: Arc::from(""),
            name: None,
            pattern: None,
        }),
        index,
        linked_worktrees,
        coalescing: WatchCoalescing {
            quiet_ms: 80,
            max_ms: 500,
        },
    }
}

fn source_query() -> WatchQuery {
    WatchQuery {
        source: Some(SourceQuery {
            revision: Revision::Worktree,
            patterns: vec![Pattern("**/*.rs".into())],
        }),
        refs: None,
        index: false,
        linked_worktrees: false,
        coalescing: WatchCoalescing {
            quiet_ms: 80,
            max_ms: 500,
        },
    }
}

fn receive(watcher: &mut soopy::RepositoryWatcher) -> Vec<RepositoryDelta> {
    watcher
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .expect("filesystem event")
}

#[test]
fn repository_watcher_rejects_invalid_source_and_coalescing() {
    let root = repository();
    let tree = SourceTree::open(soopy::open(&root).unwrap());
    let mut invalid_source = query(&tree, false, false);
    invalid_source.source.as_mut().unwrap().revision = Revision::Named(Arc::from("HEAD"));
    assert!(tree.watch_repository(invalid_source).is_err());

    let mut invalid_coalescing = query(&tree, false, false);
    invalid_coalescing.coalescing.quiet_ms = 0;
    assert!(tree.watch_repository(invalid_coalescing).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn repository_watcher_emits_source_and_index_deltas() {
    let root = repository();
    let tree = SourceTree::open(soopy::open(&root).unwrap());
    let mut watcher = tree.watch_repository(query(&tree, true, false)).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 2;\n").unwrap();
    let source = receive(&mut watcher);
    assert!(source.iter().any(|delta| {
        matches!(delta, RepositoryDelta::Source(SourceDelta::Changed { after, .. }) if after.source.path.0.as_ref() == "src/lib.rs")
    }));
    assert!(!source
        .iter()
        .any(|delta| matches!(delta, RepositoryDelta::Index(_))));
    assert!(!source.iter().any(|delta| {
        matches!(
            delta,
            RepositoryDelta::Source(SourceDelta::RevisionChanged { .. })
        )
    }));

    git(&root, &["add", "src/lib.rs"]);
    let index = receive(&mut watcher);
    assert!(index
        .iter()
        .any(|delta| matches!(delta, RepositoryDelta::Index(_))));
    assert!(!index.iter().any(|delta| {
        matches!(
            delta,
            RepositoryDelta::Source(SourceDelta::RevisionChanged { .. })
        )
    }));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn repository_watcher_emits_ref_targets_and_head_changes() {
    let root = repository();
    git(&root, &["branch", "topic"]);
    let tree = SourceTree::open(soopy::open(&root).unwrap());
    let mut watcher = tree.watch_repository(query(&tree, false, false)).unwrap();

    git(&root, &["checkout", "-q", "topic"]);
    let head = receive(&mut watcher);
    assert!(head.iter().any(|delta| {
        matches!(delta, RepositoryDelta::Ref(RefDelta::HeadChanged { before: soopy::HeadObservation { state: Head::Symbolic { target: before }, target: Some(_) }, after: soopy::HeadObservation { state: Head::Symbolic { target: after }, target: Some(_) } }) if (before.as_ref() == "refs/heads/master" || before.as_ref() == "refs/heads/main") && after.as_ref() == "refs/heads/topic")
    }));

    let before = git(&root, &["rev-parse", "refs/heads/topic"]);
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 2;\n").unwrap();
    git(&root, &["add", "src/lib.rs"]);
    git(&root, &["commit", "-qm", "advance topic"]);
    let advanced = receive(&mut watcher);
    assert!(advanced.iter().any(|delta| {
        matches!(delta, RepositoryDelta::Ref(RefDelta::Changed { before: old, after }) if old.name.as_ref() == "refs/heads/topic" && old.direct.0.as_ref() == before.trim() && old.direct != after.direct)
    }));

    git(&root, &["tag", "-a", "v1", "-m", "tag one"]);
    git(&root, &["pack-refs", "--all", "--prune"]);
    let tag = receive(&mut watcher);
    assert!(tag.iter().any(|delta| {
        matches!(delta, RepositoryDelta::Ref(RefDelta::Added(observation)) if observation.name.as_ref() == "refs/tags/v1" && observation.peeled.is_some())
    }));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn repository_watcher_emits_detached_head_transition() {
    let root = repository();
    let tree = SourceTree::open(soopy::open(&root).unwrap());
    let mut watcher = tree.watch_repository(query(&tree, false, false)).unwrap();
    git(&root, &["checkout", "-q", "--detach", "HEAD"]);
    let deltas = receive(&mut watcher);
    assert!(deltas.iter().any(|delta| {
        matches!(delta, RepositoryDelta::Ref(RefDelta::HeadChanged { before: soopy::HeadObservation { state: Head::Symbolic { .. }, target: Some(_) }, after: soopy::HeadObservation { state: Head::Detached(_), target: Some(_) } }))
    }));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn repository_watcher_orders_source_create_change_and_remove_paths() {
    let root = repository();
    let tree = SourceTree::open(soopy::open(&root).unwrap());
    let mut watcher = tree.watch_repository(source_query()).unwrap();

    std::fs::write(root.join("src/z.rs"), "pub const Z: u8 = 1;\n").unwrap();
    std::fs::write(root.join("src/a.rs"), "pub const A: u8 = 1;\n").unwrap();
    let created = receive(&mut watcher);
    let paths: Vec<_> = created
        .iter()
        .filter_map(|delta| match delta {
            RepositoryDelta::Source(SourceDelta::Added(entry)) => Some(entry.source.path.0.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(paths, ["src/a.rs", "src/z.rs"]);

    std::fs::write(root.join("src/a.rs"), "pub const A: u8 = 2;\n").unwrap();
    std::fs::remove_file(root.join("src/z.rs")).unwrap();
    let changed = receive(&mut watcher);
    let paths: Vec<_> = changed
        .iter()
        .filter_map(|delta| match delta {
            RepositoryDelta::Source(SourceDelta::Changed { after, .. }) => {
                Some(after.source.path.0.as_ref())
            }
            RepositoryDelta::Source(SourceDelta::Removed(source)) => Some(source.path.0.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(paths, ["src/a.rs", "src/z.rs"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn repository_watcher_observes_linked_worktree_metadata() {
    let root = repository();
    let linked = unique("linked");
    let tree = SourceTree::open(soopy::open(&root).unwrap());
    let mut watcher = tree
        .watch_repository(WatchQuery {
            source: None,
            refs: None,
            index: false,
            linked_worktrees: true,
            coalescing: WatchCoalescing {
                quiet_ms: 80,
                max_ms: 500,
            },
        })
        .unwrap();
    git(
        &root,
        &["worktree", "add", "-q", "-b", "feature", linked.to_str().unwrap()],
    );
    let deltas = receive(&mut watcher);
    let linked = linked.canonicalize().unwrap();
    assert!(deltas.iter().any(|delta| {
        matches!(delta, RepositoryDelta::Worktree(WorktreeDelta::Added(observation)) if observation.root == linked)
    }));
    std::fs::write(linked.join("src/lib.rs"), "pub const VALUE: u8 = 2;\n").unwrap();
    git(&linked, &["add", "src/lib.rs"]);
    git(&linked, &["commit", "-qm", "advance linked"]);
    let deltas = receive(&mut watcher);
    assert!(deltas.iter().any(|delta| {
        matches!(delta, RepositoryDelta::Worktree(WorktreeDelta::Changed { before, after }) if before.root == linked && before.commit != after.commit)
    }));
    git(&root, &["worktree", "remove", "--force", linked.to_str().unwrap()]);
    let deltas = receive(&mut watcher);
    assert!(deltas.iter().any(|delta| {
        matches!(delta, RepositoryDelta::Worktree(WorktreeDelta::Removed(observation)) if observation.root == linked)
    }));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn watch_types_serde_round_trip() {
    let root = repository();
    let tree = SourceTree::open(soopy::open(&root).unwrap());
    let query = query(&tree, true, true);
    let encoded = serde_json::to_string(&query).unwrap();
    let decoded: WatchQuery = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, query);
    let mut watcher = tree.watch_repository(query).unwrap();
    let snapshot = serde_json::to_string(watcher.snapshot()).unwrap();
    let snapshot: soopy::RepositorySnapshot = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(&snapshot, watcher.snapshot());
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 2;\n").unwrap();
    let deltas = receive(&mut watcher);
    let encoded = serde_json::to_string(&deltas).unwrap();
    let decoded: Vec<RepositoryDelta> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, deltas);
    std::fs::remove_dir_all(root).unwrap();
}
