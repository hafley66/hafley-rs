use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use soopy::{
    ContentId, ObjectId, Pattern, ReadRequest, RefId, RepoPath, RepositoryId, Revision, RevisionId,
    SourceRef, SourceTree, WorktreeId,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn unique(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "soopy_id_{}_{}_{}_{}",
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
    String::from_utf8(output.stdout).unwrap()
}

/// A repository with one commit tracking `src/lib.rs`, plus a linked worktree
/// checked out at `feature`. Returns `(main_root, linked_root)`.
fn linked_pair() -> (std::path::PathBuf, std::path::PathBuf) {
    let main = unique("main");
    std::fs::create_dir_all(main.join("src")).unwrap();
    git(&main, &["init", "-q"]);
    std::fs::write(main.join("src/lib.rs"), "pub const VALUE: u8 = 1;\n").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-qm", "first"]);
    let linked = unique("linked");
    git(
        &main,
        &["worktree", "add", linked.to_str().unwrap(), "-b", "feature"],
    );
    (main, linked)
}

fn cleanup(roots: &[std::path::PathBuf]) {
    for root in roots {
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn linked_worktrees_share_repository_but_have_distinct_worktree_ids() {
    let (main, linked) = linked_pair();
    let main_repo = soopy::open(&main).unwrap();
    let linked_repo = soopy::open(&linked).unwrap();
    assert_eq!(main_repo.identity, linked_repo.identity);
    assert_ne!(main_repo.worktree, linked_repo.worktree);
    git(
        &main,
        &["worktree", "remove", "--force", linked.to_str().unwrap()],
    );
    cleanup(&[main]);
}

#[test]
fn reopening_one_worktree_reproduces_both_identifiers() {
    let (main, linked) = linked_pair();
    for root in [&main, &linked] {
        let first = soopy::open(root).unwrap();
        let second = soopy::open(root).unwrap();
        assert_eq!(first.identity, second.identity);
        assert_eq!(first.worktree, second.worktree);
    }
    git(
        &main,
        &["worktree", "remove", "--force", linked.to_str().unwrap()],
    );
    cleanup(&[main]);
}

#[test]
fn worktree_source_ref_from_one_checkout_cannot_be_read_through_another() {
    let (main, linked) = linked_pair();
    let mut main_tree = SourceTree::open(soopy::open(&main).unwrap());
    let work = main_tree.resolve_revision(Revision::Worktree).unwrap();
    let entries = main_tree
        .enumerate(&work, &[Pattern("**/*.rs".into())])
        .unwrap();
    assert_eq!(entries.len(), 1);

    let mut linked_tree = SourceTree::open(soopy::open(&linked).unwrap());
    let request = ReadRequest {
        source: entries[0].source.clone(),
        expected: Some(entries[0].content.clone()),
    };
    assert!(linked_tree
        .read_many(std::slice::from_ref(&request))
        .is_err());
    let mut buffer = Vec::new();
    assert!(linked_tree
        .read_each(std::slice::from_ref(&request), &mut buffer, |_| Ok(()))
        .is_err());
    git(
        &main,
        &["worktree", "remove", "--force", linked.to_str().unwrap()],
    );
    cleanup(&[main]);
}

#[test]
fn commit_source_ref_is_readable_from_either_linked_worktree() {
    let (main, linked) = linked_pair();
    let mut main_tree = SourceTree::open(soopy::open(&main).unwrap());
    let head = main_tree
        .resolve_revision(Revision::Named(Arc::from("HEAD")))
        .unwrap();
    assert!(matches!(head, RevisionId::Commit(_)));
    let entries = main_tree
        .enumerate(&head, &[Pattern("**/*.rs".into())])
        .unwrap();
    assert_eq!(entries.len(), 1);

    let mut linked_tree = SourceTree::open(soopy::open(&linked).unwrap());
    let request = ReadRequest {
        source: entries[0].source.clone(),
        expected: Some(entries[0].content.clone()),
    };
    let answers = linked_tree.read_many(&[request]).unwrap();
    assert_eq!(&*answers[0].bytes, b"pub const VALUE: u8 = 1;\n");
    git(
        &main,
        &["worktree", "remove", "--force", linked.to_str().unwrap()],
    );
    cleanup(&[main]);
}

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug + PartialEq>(
    value: &T,
) -> T {
    let json = serde_json::to_string(value).unwrap();
    let back: T = serde_json::from_str(&json).unwrap();
    assert_eq!(&back, value);
    back
}

#[test]
fn every_public_coordinate_round_trips_without_display_strings() {
    let repository = RepositoryId(Arc::from("repo"));
    let worktree = WorktreeId(Arc::from("wt"));
    let object = ObjectId(Arc::from("oid"));
    let path = RepoPath(Arc::from("src/lib.rs"));

    round_trip(&repository);
    round_trip(&worktree);
    round_trip(&object);
    round_trip(&path);

    round_trip(&RefId::new(
        repository.clone(),
        Arc::from("refs/heads/main"),
    ));

    round_trip(&RevisionId::Worktree {
        worktree: worktree.clone(),
        head: Some(object.clone()),
        dirty: true,
    });
    round_trip(&RevisionId::Commit(object.clone()));

    round_trip(&ContentId::GitBlob(object.clone()));
    round_trip(&ContentId::Blake3(*blake3::hash(b"x").as_bytes()));

    round_trip(&SourceRef {
        repository: repository.clone(),
        revision: RevisionId::Commit(object.clone()),
        path: path.clone(),
    });

    // The serialized forms never carry a `Display` spelling: BLAKE3 content
    // serializes as a byte array, and Git blobs as their hex OID.
    let blob = serde_json::to_string(&ContentId::GitBlob(object)).unwrap();
    assert!(blob.contains("oid"));
    let blake = serde_json::to_string(&ContentId::Blake3(*blake3::hash(b"x").as_bytes())).unwrap();
    assert!(!blake.contains("blake3:"));
}

// FAIL-PRE-FIX: `ContentId::blake3` did not exist and `ReadRequest` derived no
// serde, so this pair failed to compile (`no function or associated item named
// `blake3``, and `ReadRequest: Serialize` unsatisfied).

#[test]
fn the_blake3_constructor_matches_the_variant_it_builds() {
    assert_eq!(
        ContentId::blake3(b"hello"),
        ContentId::Blake3(*blake3::hash(b"hello").as_bytes())
    );
    assert_eq!(ContentId::blake3(b"hello"), ContentId::blake3(b"hello"));
    assert_ne!(ContentId::blake3(b"hello"), ContentId::blake3(b"other"));
    assert!(ContentId::blake3(b"hello")
        .to_string()
        .starts_with("blake3:"));
}

#[test]
fn a_read_request_round_trips_through_json() {
    let repository = RepositoryId(Arc::from("repo"));
    let object = ObjectId(Arc::from("oid"));
    let request = ReadRequest {
        source: SourceRef {
            repository,
            revision: RevisionId::Commit(object.clone()),
            path: RepoPath(Arc::from("src/lib.rs")),
        },
        expected: Some(ContentId::GitBlob(object)),
    };
    round_trip(&request);
}
