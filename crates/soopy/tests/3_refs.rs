use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use soopy::{diff_refs, Head, ObjectId, ObjectKind, RefDelta, RefQuery, RefSnapshot, Refs};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn unique(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "soopy_refs_{}_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ))
}

fn git(root: &std::path::Path, args: &[&str]) -> String {
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
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn snapshot(root: &std::path::Path, namespace: &str) -> RefSnapshot {
    let repo = soopy::open(root).unwrap();
    Refs::open(repo)
        .snapshot(&RefQuery {
            repository: repo_identity(root),
            namespace: Arc::from(namespace),
            name: None,
            pattern: None,
        })
        .unwrap()
}

fn repo_identity(root: &std::path::Path) -> soopy::RepositoryId {
    soopy::open(root).unwrap().identity
}

/// A repository with one commit tracking a file, on branch `main`.
fn repository() -> std::path::PathBuf {
    let root = unique("repo");
    std::fs::create_dir_all(root.join("src")).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 1;\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "first"]);
    root
}

#[test]
fn loose_and_packed_branches_produce_one_observation_per_full_name() {
    let root = repository();
    git(&root, &["branch", "feature"]);
    // Pack the existing refs, then create one more loose branch so the
    // snapshot spans both storage layouts.
    git(&root, &["pack-refs", "--all"]);
    git(&root, &["branch", "loose/branch"]);

    let snap = snapshot(&root, "refs/heads");
    let names: Vec<&str> = snap.refs.iter().map(|row| row.name.as_ref()).collect();
    assert_eq!(
        names,
        [
            "refs/heads/feature",
            "refs/heads/loose/branch",
            "refs/heads/main"
        ]
    );
    // Every observation carries the same commit target.
    let commit = git(&root, &["rev-parse", "HEAD"]);
    for row in &snap.refs {
        assert_eq!(row.direct.0.as_ref(), commit);
        assert_eq!(row.kind, ObjectKind::Commit);
        assert!(row.peeled.is_none());
        assert!(row.tag.is_none());
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn lightweight_tag_retains_direct_and_peeled_semantics() {
    let root = repository();
    git(&root, &["tag", "light"]);

    let snap = snapshot(&root, "refs/tags");
    assert_eq!(snap.refs.len(), 1);
    let tag = &snap.refs[0];
    assert_eq!(tag.name.as_ref(), "refs/tags/light");
    // A lightweight tag points directly at the commit: no peeled target and
    // no tag object.
    let commit = git(&root, &["rev-parse", "HEAD"]);
    assert_eq!(tag.direct.0.as_ref(), commit);
    assert_eq!(tag.kind, ObjectKind::Commit);
    assert!(tag.peeled.is_none());
    assert!(tag.tag.is_none());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn annotated_tag_retains_annotation_peeled_target_tagger_and_message() {
    let root = repository();
    git(
        &root,
        &["tag", "-a", "release", "-m", "the annotated message"],
    );

    let snap = snapshot(&root, "refs/tags");
    assert_eq!(snap.refs.len(), 1);
    let tag = &snap.refs[0];
    assert_eq!(tag.name.as_ref(), "refs/tags/release");
    assert_eq!(tag.kind, ObjectKind::Tag);
    // The direct object is the annotation object, distinct from the commit.
    let commit = git(&root, &["rev-parse", "HEAD"]);
    let annotation = git(&root, &["rev-parse", "refs/tags/release"]);
    assert_eq!(tag.direct.0.as_ref(), annotation);
    assert_ne!(annotation, commit);
    // The peeled target is the commit.
    let peeled = tag
        .peeled
        .as_ref()
        .expect("annotated tag peels to a target");
    assert_eq!(peeled.0.as_ref(), commit);

    let meta = tag.tag.as_ref().expect("annotated tag carries metadata");
    assert_eq!(meta.target_kind, ObjectKind::Commit);
    assert_eq!(meta.tagger.name.as_ref(), "soopy");
    assert_eq!(meta.tagger.email.as_ref(), "soopy@example.invalid");
    assert!(meta.tagger.when > 0);
    assert_eq!(meta.message.as_ref(), "the annotated message");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn annotated_tag_message_preserves_internal_newlines() {
    let root = repository();
    std::fs::write(
        root.join("msg.txt"),
        "first line\nsecond line\nthird line\n",
    )
    .unwrap();
    git(&root, &["tag", "-a", "multiline", "-F", "msg.txt"]);

    let snap = snapshot(&root, "refs/tags");
    let meta = snap.refs[0]
        .tag
        .as_ref()
        .expect("annotated tag carries metadata");
    assert_eq!(meta.message.as_ref(), "first line\nsecond line\nthird line");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn symbolic_and_detached_head_are_distinct() {
    let root = repository();
    let symbolic = snapshot(&root, "refs/heads");
    assert_eq!(
        symbolic.head,
        Head::Symbolic {
            target: Arc::from("refs/heads/main")
        }
    );

    git(&root, &["checkout", "-q", "--detach", "HEAD"]);
    let detached = snapshot(&root, "refs/heads");
    let commit = git(&root, &["rev-parse", "HEAD"]);
    assert_eq!(detached.head, Head::Detached(ObjectId(Arc::from(commit))));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn linked_worktrees_share_refs_but_keep_per_worktree_head() {
    let root = repository();
    let linked = unique("linked");
    git(
        &root,
        &["worktree", "add", linked.to_str().unwrap(), "-b", "feature"],
    );

    let main = snapshot(&root, "refs/heads");
    let linked_snap = snapshot(&linked, "refs/heads");

    // Named refs are identical across the two checkouts.
    let main_names: Vec<&str> = main.refs.iter().map(|row| row.name.as_ref()).collect();
    let linked_names: Vec<&str> = linked_snap
        .refs
        .iter()
        .map(|row| row.name.as_ref())
        .collect();
    assert_eq!(main_names, linked_names);
    assert_eq!(main_names, ["refs/heads/feature", "refs/heads/main"]);

    // HEAD differs per checkout.
    assert_eq!(
        main.head,
        Head::Symbolic {
            target: Arc::from("refs/heads/main")
        }
    );
    assert_eq!(
        linked_snap.head,
        Head::Symbolic {
            target: Arc::from("refs/heads/feature")
        }
    );

    git(
        &root,
        &["worktree", "remove", "--force", linked.to_str().unwrap()],
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn snapshot_ordering_and_serialization_are_deterministic() {
    let root = repository();
    git(&root, &["tag", "-a", "zebra", "-m", "z"]);
    git(&root, &["branch", "alpha"]);

    let first = snapshot(&root, "");
    let second = snapshot(&root, "");

    // Two snapshots agree and sort by full ref name.
    assert_eq!(first, second);
    let names: Vec<&str> = first.refs.iter().map(|row| row.name.as_ref()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);

    // Serialization round-trips and is byte-stable across equal snapshots.
    let json = serde_json::to_string(&first).unwrap();
    let again = serde_json::to_string(&second).unwrap();
    assert_eq!(json, again);
    let parsed: RefSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, first);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn ref_delta_classifies_add_remove_and_target_change() {
    let root = repository();
    let first_commit = git(&root, &["rev-parse", "HEAD"]);
    git(&root, &["tag", "v1"]);
    let before = snapshot(&root, "");

    // Add a branch, remove the tag, and advance the tracked branch.
    git(&root, &["branch", "topic"]);
    git(&root, &["tag", "-d", "v1"]);
    std::fs::write(root.join("src/lib.rs"), "pub const VALUE: u8 = 2;\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "second"]);
    let after = snapshot(&root, "");

    let deltas = diff_refs(&before, &after);
    let mut added = 0;
    let mut removed = 0;
    let mut changed = 0;
    for delta in &deltas {
        match delta {
            RefDelta::Added(row) => {
                assert_eq!(row.name.as_ref(), "refs/heads/topic");
                added += 1;
            }
            RefDelta::Removed(row) => {
                assert_eq!(row.name.as_ref(), "refs/tags/v1");
                removed += 1;
            }
            RefDelta::Changed { before, after } => {
                assert_eq!(before.name.as_ref(), "refs/heads/main");
                assert_eq!(after.name.as_ref(), "refs/heads/main");
                assert_eq!(before.direct.0.as_ref(), first_commit);
                assert_ne!(before.direct.0.as_ref(), after.direct.0.as_ref());
                changed += 1;
            }
        }
    }
    assert_eq!((added, removed, changed), (1, 1, 1));
    std::fs::remove_dir_all(&root).unwrap();
}
