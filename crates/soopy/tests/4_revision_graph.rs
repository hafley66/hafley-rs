use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use soopy::{
    Acquisition, AcquisitionOperation, AcquisitionPolicy, AcquisitionReceipt, AcquisitionRequest,
    ObjectId, RefQuery, Refs, Revision, RevisionGraph, RevisionGraphQuery, RevisionResolution,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn unique(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "soopy_graph_{}_{}_{}_{}",
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

fn oid(root: &std::path::Path, name: &str) -> ObjectId {
    ObjectId(Arc::from(git(root, &["rev-parse", name])))
}

fn repo_identity(root: &std::path::Path) -> soopy::RepositoryId {
    soopy::open(root).unwrap().identity
}

/// A repository with a linear chain of `count` commits on `main`, one file per
/// commit so every commit has a distinct tree. Returns the root.
fn linear_repo(count: usize) -> std::path::PathBuf {
    let root = unique("linear");
    std::fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    for i in 0..count {
        std::fs::write(root.join(format!("f{i}")), format!("{i}\n")).unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-qm", &format!("c{i}")]);
    }
    root
}

fn query(root: &std::path::Path, q: &RevisionGraphQuery) -> soopy::RevisionGraphResult {
    RevisionGraph::open(soopy::open(root).unwrap())
        .query(q)
        .unwrap()
}

#[test]
fn linear_graph_exposes_parents_and_deterministic_traversal() {
    let root = linear_repo(3);
    let c0 = oid(&root, "HEAD~2");
    let c1 = oid(&root, "HEAD~1");
    let c2 = oid(&root, "HEAD");

    let result = query(
        &root,
        &RevisionGraphQuery {
            repository: repo_identity(&root),
            resolve: vec![],
            parents: vec![c0.clone(), c1.clone(), c2.clone()],
            ancestry: vec![],
            merge_bases: vec![],
            ahead_behind: vec![],
            walks: vec![Revision::Named(Arc::from("HEAD"))],
        },
    );

    assert_eq!(result.parents[0].parents, vec![]);
    assert_eq!(result.parents[1].parents, vec![c0.clone()]);
    assert_eq!(result.parents[2].parents, vec![c1.clone()]);

    let walk = &result.walks[0];
    assert_eq!(walk.start, c2.clone());
    assert_eq!(walk.commits, vec![c2.clone(), c1.clone(), c0.clone()]);

    // A second query returns byte-identical results: traversal is deterministic.
    let again = query(
        &root,
        &RevisionGraphQuery {
            repository: repo_identity(&root),
            resolve: vec![],
            parents: vec![],
            ancestry: vec![],
            merge_bases: vec![],
            ahead_behind: vec![],
            walks: vec![Revision::Named(Arc::from("HEAD"))],
        },
    );
    assert_eq!(result.walks, again.walks);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn branched_graph_merges_preserve_parent_order() {
    let root = linear_repo(1);
    git(&root, &["checkout", "-qb", "feature"]);
    std::fs::write(root.join("f.txt"), "feature\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "feature"]);
    let feature = oid(&root, "HEAD");
    git(&root, &["checkout", "-q", "main"]);
    std::fs::write(root.join("m.txt"), "main\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "main"]);
    let main = oid(&root, "HEAD");
    git(&root, &["merge", "-q", "--no-ff", "-m", "merge", "feature"]);
    let merge = oid(&root, "HEAD");

    let result = query(
        &root,
        &RevisionGraphQuery {
            repository: repo_identity(&root),
            resolve: vec![],
            parents: vec![merge.clone()],
            ancestry: vec![],
            merge_bases: vec![],
            ahead_behind: vec![],
            walks: vec![],
        },
    );
    // First parent is the branch merged into, second is the merged-in branch.
    assert_eq!(
        result.parents[0].parents,
        vec![main.clone(), feature.clone()]
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn ancestry_merge_base_and_ahead_behind_agree_on_diverged_branches() {
    let root = linear_repo(1);
    let base = oid(&root, "HEAD");
    git(&root, &["checkout", "-qb", "left"]);
    std::fs::write(root.join("l"), "l\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "left"]);
    let left = oid(&root, "HEAD");
    git(&root, &["checkout", "-q", "main"]);
    std::fs::write(root.join("r"), "r\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "right"]);
    let right = oid(&root, "HEAD");

    let result = query(
        &root,
        &RevisionGraphQuery {
            repository: repo_identity(&root),
            resolve: vec![],
            parents: vec![],
            ancestry: vec![(base.clone(), left.clone()), (left.clone(), right.clone())],
            merge_bases: vec![(left.clone(), right.clone())],
            ahead_behind: vec![(left.clone(), right.clone())],
            walks: vec![],
        },
    );

    assert!(result.ancestry[0].is_ancestor);
    assert!(!result.ancestry[1].is_ancestor);

    assert_eq!(result.merge_bases[0].bases, vec![base.clone()]);

    // Each side has one commit the other lacks.
    assert_eq!(result.ahead_behind[0].ahead, 1);
    assert_eq!(result.ahead_behind[0].behind, 1);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn lightweight_and_annotated_tags_peel_before_walking() {
    let root = linear_repo(2);
    let head = oid(&root, "HEAD");
    git(&root, &["tag", "light"]);
    git(&root, &["tag", "-a", "annotated", "-m", "release"]);

    // The annotated tag's direct object is the tag object, distinct from HEAD.
    let snapshot = Refs::open(soopy::open(&root).unwrap())
        .snapshot(&RefQuery {
            repository: repo_identity(&root),
            namespace: Arc::from("refs/tags"),
            name: None,
            pattern: None,
        })
        .unwrap();
    let annotated = snapshot
        .refs
        .iter()
        .find(|row| row.name.as_ref() == "refs/tags/annotated")
        .unwrap();
    let peeled = annotated.peeled.as_ref().expect("annotated tag peels");

    let result = query(
        &root,
        &RevisionGraphQuery {
            repository: repo_identity(&root),
            resolve: vec![],
            parents: vec![],
            ancestry: vec![],
            merge_bases: vec![],
            ahead_behind: vec![],
            walks: vec![
                Revision::Named(Arc::from("light")),
                Revision::Named(Arc::from("annotated")),
            ],
        },
    );

    // Both tags walk the same commits, and the annotated tag's walk start is
    // its peeled target, not the tag object.
    assert_eq!(result.walks[0].start, head.clone());
    assert_eq!(result.walks[1].start, peeled.clone());
    assert_eq!(result.walks[0].commits, result.walks[1].commits);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn missing_shallow_and_corrupt_objects_are_distinct() {
    // Missing: a 40-hex OID that names no object.
    let root = linear_repo(1);
    let missing = ObjectId(Arc::from("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"));
    let result = query(
        &root,
        &RevisionGraphQuery {
            repository: repo_identity(&root),
            resolve: vec![Revision::Commit(missing.clone())],
            parents: vec![],
            ancestry: vec![],
            merge_bases: vec![],
            ahead_behind: vec![],
            walks: vec![],
        },
    );
    assert_eq!(result.resolutions[0], RevisionResolution::Absent);

    // Corrupt: an object that exists on disk but cannot be unpacked.
    let head = oid(&root, "HEAD");
    let corrupt = head.clone();
    let loose = root
        .join(".git/objects")
        .join(&head.0[..2])
        .join(&head.0[2..]);
    let mut permissions = std::fs::metadata(&loose).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&loose, permissions).unwrap();
    std::fs::write(&loose, b"garbage\n").unwrap();
    let result = query(
        &root,
        &RevisionGraphQuery {
            repository: repo_identity(&root),
            resolve: vec![Revision::Commit(corrupt)],
            parents: vec![],
            ancestry: vec![],
            merge_bases: vec![],
            ahead_behind: vec![],
            walks: vec![],
        },
    );
    assert_eq!(result.resolutions[0], RevisionResolution::CorruptObject);
    std::fs::remove_dir_all(&root).unwrap();

    // Shallow: a depth-1 clone of a multi-commit repo resolves HEAD at the
    // shallow boundary.
    let origin = linear_repo(4);
    let shallow = unique("shallow");
    git(
        &origin,
        &[
            "clone",
            "-q",
            "--depth=1",
            &format!("file://{}", origin.display()),
            shallow.to_str().unwrap(),
        ],
    );
    let result = query(
        &shallow,
        &RevisionGraphQuery {
            repository: repo_identity(&shallow),
            resolve: vec![Revision::Named(Arc::from("HEAD"))],
            parents: vec![],
            ancestry: vec![],
            merge_bases: vec![],
            ahead_behind: vec![],
            walks: vec![],
        },
    );
    let expected = oid(&shallow, "HEAD");
    assert_eq!(
        result.resolutions[0],
        RevisionResolution::ShallowBoundary(expected)
    );
    std::fs::remove_dir_all(&origin).unwrap();
    std::fs::remove_dir_all(&shallow).unwrap();
}

#[test]
fn read_only_queries_leave_repository_state_unchanged() {
    let root = linear_repo(3);
    git(&root, &["tag", "v1"]);
    let head = oid(&root, "HEAD");
    let before = state_fingerprint(&root);

    let _ = query(
        &root,
        &RevisionGraphQuery {
            repository: repo_identity(&root),
            resolve: vec![Revision::Named(Arc::from("HEAD"))],
            parents: vec![head.clone()],
            ancestry: vec![(oid(&root, "HEAD~2"), head.clone())],
            merge_bases: vec![(head.clone(), oid(&root, "HEAD~1"))],
            ahead_behind: vec![(head.clone(), oid(&root, "HEAD~1"))],
            walks: vec![Revision::Named(Arc::from("HEAD"))],
        },
    );

    assert_eq!(before, state_fingerprint(&root));
    std::fs::remove_dir_all(&root).unwrap();
}

/// A stable multi-line fingerprint of the observable repository state: refs,
/// object count, shallow file, index bytes, and worktree status.
fn state_fingerprint(root: &std::path::Path) -> String {
    let refs = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["for-each-ref", "--format=%(refname) %(objectname)"])
        .output()
        .unwrap();
    let object_count = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "cat-file",
            "--batch-all-objects",
            "--batch-check=%(objectname)",
        ])
        .output()
        .unwrap()
        .stdout
        .len();
    let shallow_path = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--git-path", "shallow"])
        .output()
        .unwrap();
    let shallow_path = String::from_utf8(shallow_path.stdout).unwrap();
    let shallow = std::fs::read_to_string(shallow_path.trim()).unwrap_or_default();
    let index_path = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--git-path", "index"])
        .output()
        .unwrap();
    let index =
        std::fs::read(String::from_utf8(index_path.stdout).unwrap().trim()).unwrap_or_default();
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    format!(
        "refs={}\nobjects={}\nshallow={}\nindex={:?}\nstatus={}",
        String::from_utf8_lossy(&refs.stdout),
        object_count,
        shallow,
        blake3::hash(&index),
        String::from_utf8_lossy(&status.stdout),
    )
}

#[test]
fn acquisition_policy_rejects_network_mutation_by_default() {
    let root = linear_repo(1);
    let acquisition = Acquisition::open(soopy::open(&root).unwrap());
    let request = AcquisitionRequest {
        repository: repo_identity(&root),
        operations: vec![
            AcquisitionOperation::FetchRef {
                remote: Arc::from("origin"),
                name: Arc::from("topic"),
            },
            AcquisitionOperation::FetchTag {
                remote: Arc::from("origin"),
                name: Arc::from("v9"),
            },
            AcquisitionOperation::Deepen {
                remote: Arc::from("origin"),
                depth: 1,
            },
            AcquisitionOperation::Unshallow {
                remote: Arc::from("origin"),
            },
        ],
    };
    let receipts = acquisition
        .execute(&AcquisitionPolicy::default(), &request)
        .unwrap();
    assert!(receipts
        .iter()
        .all(|outcome| matches!(outcome.receipt, AcquisitionReceipt::RejectedByPolicy)));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn offline_local_remote_fetch_and_deepen_are_permitted() {
    let origin = linear_repo(5);
    // Tag an intermediate commit so the tag is absent from a depth-1 clone.
    git(&origin, &["tag", "-a", "v1", "-m", "release", "HEAD~2"]);
    let tag_target = oid(&origin, "v1^{commit}");
    let tag_object = oid(&origin, "v1");

    // A depth-1 clone starts shallow at HEAD and lacks the tag.
    let shallow = unique("shallow");
    git(
        &origin,
        &[
            "clone",
            "-q",
            "--depth=1",
            &format!("file://{}", origin.display()),
            shallow.to_str().unwrap(),
        ],
    );
    let identity = repo_identity(&shallow);
    let acquisition = Acquisition::open(soopy::open(&shallow).unwrap());
    let policy = AcquisitionPolicy {
        allow_fetch: true,
        allow_tag_fetch: true,
        allow_unshallow: true,
    };

    // The tag is not present, so it is fetched from the local file remote.
    let request = AcquisitionRequest {
        repository: identity.clone(),
        operations: vec![AcquisitionOperation::FetchTag {
            remote: Arc::from("origin"),
            name: Arc::from("v1"),
        }],
    };
    let receipts = acquisition.execute(&policy, &request).unwrap();
    assert!(matches!(
        &receipts[0].receipt,
        AcquisitionReceipt::FetchedTag { name, direct, peeled: Some(target) }
            if name.as_ref() == "v1" && *direct == tag_object && *target == tag_target
    ));

    // Unshallow against the local file remote, no external network involved.
    let request = AcquisitionRequest {
        repository: identity.clone(),
        operations: vec![AcquisitionOperation::Unshallow {
            remote: Arc::from("origin"),
        }],
    };
    let receipts = acquisition.execute(&policy, &request).unwrap();
    assert_eq!(receipts[0].receipt, AcquisitionReceipt::Unshallowed);

    // The full history is now reachable and HEAD resolves as present.
    let result = query(
        &shallow,
        &RevisionGraphQuery {
            repository: identity.clone(),
            resolve: vec![Revision::Named(Arc::from("HEAD"))],
            parents: vec![],
            ancestry: vec![],
            merge_bases: vec![],
            ahead_behind: vec![],
            walks: vec![Revision::Named(Arc::from("HEAD"))],
        },
    );
    assert_eq!(
        result.resolutions[0],
        RevisionResolution::Present(oid(&shallow, "HEAD"))
    );
    assert_eq!(result.walks[0].commits.len(), 5);

    // A tag already fetched is already present on a second request.
    let request = AcquisitionRequest {
        repository: identity,
        operations: vec![AcquisitionOperation::FetchTag {
            remote: Arc::from("origin"),
            name: Arc::from("v1"),
        }],
    };
    let receipts = acquisition.execute(&policy, &request).unwrap();
    assert!(matches!(
        receipts[0].receipt,
        AcquisitionReceipt::AlreadyPresent { .. }
    ));

    std::fs::remove_dir_all(&origin).unwrap();
    std::fs::remove_dir_all(&shallow).unwrap();
}

#[test]
fn unrelated_histories_have_no_merge_base() {
    let root = linear_repo(1);
    let left = oid(&root, "HEAD");
    git(&root, &["checkout", "-q", "--orphan", "other"]);
    git(&root, &["rm", "-q", "-rf", "."]);
    std::fs::write(root.join("other"), "other\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "other"]);
    let right = oid(&root, "HEAD");
    let result = query(
        &root,
        &RevisionGraphQuery {
            repository: repo_identity(&root),
            resolve: vec![],
            parents: vec![],
            ancestry: vec![],
            merge_bases: vec![(left, right)],
            ahead_behind: vec![],
            walks: vec![],
        },
    );
    assert!(result.merge_bases[0].bases.is_empty());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn fetch_ref_and_deepen_execute_against_a_local_remote() {
    let origin = linear_repo(5);
    let shallow = unique("fetch_ref_deepen");
    git(
        &origin,
        &[
            "clone",
            "-q",
            "--depth=1",
            &format!("file://{}", origin.display()),
            shallow.to_str().unwrap(),
        ],
    );
    git(&origin, &["checkout", "-qb", "topic"]);
    std::fs::write(origin.join("topic"), "topic\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-qm", "topic"]);
    let topic = oid(&origin, "HEAD");

    let repository = soopy::open(&shallow).unwrap();
    let acquisition = Acquisition::open(repository.clone());
    let policy = AcquisitionPolicy {
        allow_fetch: true,
        allow_tag_fetch: false,
        allow_unshallow: true,
    };
    let outcomes = acquisition
        .execute(
            &policy,
            &AcquisitionRequest {
                repository: repository.identity.clone(),
                operations: vec![
                    AcquisitionOperation::FetchRef {
                        remote: Arc::from("origin"),
                        name: Arc::from("topic"),
                    },
                    AcquisitionOperation::Deepen {
                        remote: Arc::from("origin"),
                        depth: 2,
                    },
                ],
            },
        )
        .unwrap();
    assert!(
        matches!(&outcomes[0].receipt, AcquisitionReceipt::FetchedRef { target, .. } if *target == topic)
    );
    assert_eq!(
        outcomes[1].receipt,
        AcquisitionReceipt::Deepened { depth: 2 }
    );
    let walked = query(
        &shallow,
        &RevisionGraphQuery {
            repository: repository.identity,
            resolve: vec![],
            parents: vec![],
            ancestry: vec![],
            merge_bases: vec![],
            ahead_behind: vec![],
            walks: vec![Revision::Named(Arc::from("HEAD"))],
        },
    );
    assert_eq!(walked.walks[0].commits.len(), 3);
    std::fs::remove_dir_all(&origin).unwrap();
    std::fs::remove_dir_all(&shallow).unwrap();
}

#[test]
fn acquisition_validates_the_complete_request_before_mutation() {
    let root = linear_repo(1);
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            &format!("file://{}", root.display()),
        ],
    );
    let repository = soopy::open(&root).unwrap();
    let acquisition = Acquisition::open(repository.clone());
    let request = AcquisitionRequest {
        repository: repository.identity,
        operations: vec![
            AcquisitionOperation::FetchRef {
                remote: Arc::from("origin"),
                name: Arc::from("main"),
            },
            AcquisitionOperation::FetchTag {
                remote: Arc::from("--upload-pack=bad"),
                name: Arc::from("v1"),
            },
        ],
    };
    let before = state_fingerprint(&root);
    assert!(acquisition
        .execute(
            &AcquisitionPolicy {
                allow_fetch: true,
                allow_tag_fetch: true,
                allow_unshallow: false
            },
            &request
        )
        .is_err());
    assert_eq!(before, state_fingerprint(&root));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn acquisition_requests_round_trip_and_complete_repositories_are_noops() {
    let root = linear_repo(1);
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            &format!("file://{}", root.display()),
        ],
    );
    let repository = soopy::open(&root).unwrap();
    let request = AcquisitionRequest {
        repository: repository.identity.clone(),
        operations: vec![AcquisitionOperation::Unshallow {
            remote: Arc::from("origin"),
        }],
    };
    let encoded = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::from_str::<AcquisitionRequest>(&encoded).unwrap(),
        request
    );
    let outcomes = Acquisition::open(repository)
        .execute(
            &AcquisitionPolicy {
                allow_fetch: false,
                allow_tag_fetch: false,
                allow_unshallow: true,
            },
            &request,
        )
        .unwrap();
    assert_eq!(outcomes[0].receipt, AcquisitionReceipt::AlreadyComplete);
    std::fs::remove_dir_all(&root).unwrap();
}
