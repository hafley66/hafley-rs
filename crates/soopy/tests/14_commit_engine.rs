use std::fs;
use std::process::Command;
use std::sync::Arc;

use soopy::{
    CommitEngine, CommitFailpoint, CommitRefusal, ContentId, FileModeObservation,
    InMemoryStageStore, MutationPlan, ObjectId, PlannedFile, PlannedFileKind, RepoPath, RootPath,
    SourcePath, SourceRoot, SourceRootId, StageStore,
};

fn temp_dir(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("soopy_commit_{label}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn path(value: &str) -> SourcePath {
    SourcePath::Directory {
        path: RootPath(Arc::from(value)),
    }
}

fn mode(path: &std::path::Path) -> FileModeObservation {
    let metadata = fs::symlink_metadata(path).unwrap();
    #[cfg(unix)]
    let unix_mode = {
        use std::os::unix::fs::MetadataExt;
        Some(metadata.mode())
    };
    #[cfg(not(unix))]
    let unix_mode = None;
    FileModeObservation {
        readonly: metadata.permissions().readonly(),
        unix_mode,
    }
}

fn stage(root: &std::path::Path, files: Vec<PlannedFile>) -> soopy::StagedSourceTransaction {
    let source_root = SourceRoot::open_directory(root).unwrap();
    let root_id = match source_root {
        SourceRoot::Directory(directory) => SourceRootId::Directory {
            directory: directory.identity,
        },
        SourceRoot::GitWorktree(_) => unreachable!(),
    };
    let mut store = InMemoryStageStore::new();
    let id = store
        .save(MutationPlan {
            root: root_id,
            files,
        })
        .unwrap()
        .id;
    store.load(id).unwrap().unwrap()
}

fn replace_file(root: &std::path::Path, name: &str, before: &[u8], after: &[u8]) -> PlannedFile {
    let source = root.join(name);
    fs::write(&source, before).unwrap();
    let observed = mode(&source);
    PlannedFile {
        kind: PlannedFileKind::Replace,
        source: None,
        path_before: Some(path(name)),
        path_after: Some(path(name)),
        content_before: Some(ContentId::Blake3(*blake3::hash(before).as_bytes())),
        content_after: Some(ContentId::Blake3(*blake3::hash(after).as_bytes())),
        mode_before: Some(observed),
        bytes_before: Some(before.to_vec()),
        bytes_after: Some(after.to_vec()),
        edits: vec![],
    }
}

#[test]
fn commit_applies_create_replace_move_and_delete_and_returns_watch_paths() {
    let root = temp_dir("all");
    fs::write(root.join("replace.txt"), b"old").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("replace.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::write(root.join("move.txt"), b"move").unwrap();
    fs::write(root.join("delete.txt"), b"delete").unwrap();
    let move_mode = mode(&root.join("move.txt"));
    let delete_mode = mode(&root.join("delete.txt"));
    let transaction = stage(
        &root,
        vec![
            replace_file(&root, "replace.txt", b"old", b"new"),
            PlannedFile {
                kind: PlannedFileKind::Create,
                source: None,
                path_before: None,
                path_after: Some(path("created.txt")),
                content_before: None,
                content_after: Some(ContentId::Blake3(*blake3::hash(b"created").as_bytes())),
                mode_before: None,
                bytes_before: None,
                bytes_after: Some(b"created".to_vec()),
                edits: vec![],
            },
            PlannedFile {
                kind: PlannedFileKind::Move,
                source: None,
                path_before: Some(path("move.txt")),
                path_after: Some(path("moved.txt")),
                content_before: Some(ContentId::Blake3(*blake3::hash(b"move").as_bytes())),
                content_after: Some(ContentId::Blake3(*blake3::hash(b"move").as_bytes())),
                mode_before: Some(move_mode),
                bytes_before: Some(b"move".to_vec()),
                bytes_after: Some(b"move".to_vec()),
                edits: vec![],
            },
            PlannedFile {
                kind: PlannedFileKind::Delete,
                source: None,
                path_before: Some(path("delete.txt")),
                path_after: None,
                content_before: Some(ContentId::Blake3(*blake3::hash(b"delete").as_bytes())),
                content_after: None,
                mode_before: Some(delete_mode),
                bytes_before: Some(b"delete".to_vec()),
                bytes_after: None,
                edits: vec![],
            },
        ],
    );
    let state = temp_dir("all_state");
    let engine = CommitEngine::open(&root, &state).unwrap();
    let receipt = engine.commit(&transaction).unwrap();
    assert_eq!(receipt.stage_id, transaction.id);
    assert_eq!(receipt.applied_files, 4);
    assert_eq!(receipt.watch.stage_id, transaction.id);
    assert_eq!(fs::read(root.join("replace.txt")).unwrap(), b"new");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join("replace.txt"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }
    assert_eq!(fs::read(root.join("created.txt")).unwrap(), b"created");
    assert_eq!(fs::read(root.join("moved.txt")).unwrap(), b"move");
    assert!(!root.join("move.txt").exists());
    assert!(!root.join("delete.txt").exists());
    assert!(!engine.journal_path_for(transaction.id).exists());
    assert_eq!(engine.commit(&transaction).unwrap(), receipt);
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(state);
}

#[test]
fn preflight_checks_all_files_before_mutation() {
    let root = temp_dir("preflight");
    let first = replace_file(&root, "first.txt", b"old", b"new");
    let missing = PlannedFile {
        kind: PlannedFileKind::Delete,
        source: None,
        path_before: Some(path("missing.txt")),
        path_after: None,
        content_before: Some(ContentId::Blake3(*blake3::hash(b"missing").as_bytes())),
        content_after: None,
        mode_before: Some(FileModeObservation {
            readonly: false,
            unix_mode: None,
        }),
        bytes_before: Some(b"missing".to_vec()),
        bytes_after: None,
        edits: vec![],
    };
    let transaction = stage(&root, vec![first, missing]);
    let state = temp_dir("preflight_state");
    let error = CommitEngine::open(&root, &state)
        .unwrap()
        .commit(&transaction)
        .unwrap_err();
    assert!(matches!(error, CommitRefusal::Preflight { .. }));
    assert_eq!(fs::read(root.join("first.txt")).unwrap(), b"old");
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(state);
}

#[test]
fn mode_drift_is_refused_before_mutation() {
    let root = temp_dir("mode_drift");
    let transaction = stage(&root, vec![replace_file(&root, "mode.txt", b"old", b"new")]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("mode.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let state = temp_dir("mode_drift_state");
    let error = CommitEngine::open(&root, &state)
        .unwrap()
        .commit(&transaction)
        .unwrap_err();
    #[cfg(unix)]
    assert!(matches!(error, CommitRefusal::Preflight { .. }));
    #[cfg(not(unix))]
    assert!(matches!(error, CommitRefusal::Preflight { .. }));
    assert_eq!(fs::read(root.join("mode.txt")).unwrap(), b"old");
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(state);
}

#[test]
fn failpoint_after_operation_replays_idempotently() {
    let root = temp_dir("recover");
    let file = replace_file(&root, "recover.txt", b"old", b"new");
    let transaction = stage(&root, vec![file]);
    let state = temp_dir("recover_state");
    let engine = CommitEngine::open(&root, &state).unwrap();
    let error = engine
        .commit_with_failpoint(&transaction, Some(CommitFailpoint::AfterOperation(0)))
        .unwrap_err();
    assert!(matches!(error, CommitRefusal::Failpoint { .. }));
    assert!(engine.journal_path_for(transaction.id).exists());
    let receipt = engine.recover(transaction.id).unwrap();
    assert_eq!(receipt.stage_id, transaction.id);
    assert_eq!(fs::read(root.join("recover.txt")).unwrap(), b"new");
    assert!(!engine.journal_path_for(transaction.id).exists());
    assert!(!state
        .join("checkpoints")
        .join(format!("{}.progress", transaction.id))
        .exists());
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(state);
}

#[test]
fn journal_references_payload_blob_and_recovery_uses_target_state() {
    let root = temp_dir("journal_payload");
    let first = replace_file(&root, "first.txt", b"old-first", b"new-first");
    let second = replace_file(&root, "second.txt", b"old-second", b"new-second");
    let transaction = stage(&root, vec![first, second]);
    let state = temp_dir("journal_payload_state");
    let engine = CommitEngine::open(&root, &state).unwrap();
    assert!(matches!(
        engine.commit_with_failpoint(&transaction, Some(CommitFailpoint::AfterJournal)),
        Err(CommitRefusal::Failpoint { .. })
    ));
    let journal = fs::read(engine.journal_path_for(transaction.id)).unwrap();
    assert!(!journal
        .windows(b"new-first".len())
        .any(|bytes| bytes == b"new-first"));
    let blob = transaction.files[0].staged_bytes.unwrap();
    assert_eq!(
        fs::read(state.join("blobs").join(blob.to_string())).unwrap(),
        b"new-first"
    );
    let receipt = engine.recover(transaction.id).unwrap();
    assert_eq!(receipt.journal_bytes, journal.len() as u64);
    assert_eq!(receipt.checkpoint_bytes, 0);
    assert!(!engine.journal_path_for(transaction.id).exists());
    assert!(!state
        .join("checkpoints")
        .join(format!("{}.progress", transaction.id))
        .exists());
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(state);
}

#[test]
fn target_state_survives_operation_boundary_and_recovery_restart() {
    let root = temp_dir("journal_checkpoint");
    let transaction = stage(
        &root,
        vec![
            replace_file(&root, "first.txt", b"old-first", b"new-first"),
            replace_file(&root, "second.txt", b"old-second", b"new-second"),
        ],
    );
    let state = temp_dir("journal_checkpoint_state");
    let engine = CommitEngine::open(&root, &state).unwrap();
    assert!(matches!(
        engine.commit_with_failpoint(&transaction, Some(CommitFailpoint::AfterOperation(1))),
        Err(CommitRefusal::Failpoint { .. })
    ));
    assert_eq!(fs::read(root.join("first.txt")).unwrap(), b"new-first");
    assert_eq!(fs::read(root.join("second.txt")).unwrap(), b"new-second");
    let restarted = CommitEngine::open(&root, &state).unwrap();
    let receipt = restarted.recover(transaction.id).unwrap();
    assert_eq!(receipt.checkpoint_bytes, 0);
    assert_eq!(fs::read(root.join("first.txt")).unwrap(), b"new-first");
    assert_eq!(fs::read(root.join("second.txt")).unwrap(), b"new-second");
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(state);
}

#[test]
fn parent_symlink_and_git_paths_are_rejected() {
    let root = temp_dir("safety");
    let outside = temp_dir("safety_outside");
    fs::write(outside.join("file.txt"), b"outside").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    #[cfg(unix)]
    {
        let file = PlannedFile {
            kind: PlannedFileKind::Create,
            source: None,
            path_before: None,
            path_after: Some(path("link/file.txt")),
            content_before: None,
            content_after: Some(ContentId::Blake3(*blake3::hash(b"bad").as_bytes())),
            mode_before: None,
            bytes_before: None,
            bytes_after: Some(b"bad".to_vec()),
            edits: vec![],
        };
        let transaction = stage(&root, vec![file]);
        let state = temp_dir("safety_state");
        let error = CommitEngine::open(&root, &state)
            .unwrap()
            .commit(&transaction)
            .unwrap_err();
        assert!(matches!(error, CommitRefusal::UnsafePath { .. }));
        assert_eq!(fs::read(outside.join("file.txt")).unwrap(), b"outside");
        let _ = fs::remove_dir_all(state);
    }

    let git_path = PlannedFile {
        kind: PlannedFileKind::Create,
        source: None,
        path_before: None,
        path_after: Some(path(".git/should-not-write")),
        content_before: None,
        content_after: Some(ContentId::Blake3(*blake3::hash(b"bad").as_bytes())),
        mode_before: None,
        bytes_before: None,
        bytes_after: Some(b"bad".to_vec()),
        edits: vec![],
    };
    let transaction = stage(&root, vec![git_path]);
    let state = temp_dir("git_state");
    let error = CommitEngine::open(&root, &state)
        .unwrap()
        .commit(&transaction)
        .unwrap_err();
    assert!(matches!(error, CommitRefusal::UnsafePath { .. }));
    let _ = fs::remove_dir_all(state);
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(outside);
}

fn one_operation_stage(
    root: &std::path::Path,
    kind: PlannedFileKind,
) -> soopy::StagedSourceTransaction {
    match kind {
        PlannedFileKind::Create => stage(
            root,
            vec![PlannedFile {
                kind,
                source: None,
                path_before: None,
                path_after: Some(path("created.txt")),
                content_before: None,
                content_after: Some(ContentId::Blake3(*blake3::hash(b"created").as_bytes())),
                mode_before: None,
                bytes_before: None,
                bytes_after: Some(b"created".to_vec()),
                edits: vec![],
            }],
        ),
        PlannedFileKind::Replace => stage(
            root,
            vec![replace_file(root, "replace.txt", b"old", b"new")],
        ),
        PlannedFileKind::Move => {
            fs::write(root.join("move.txt"), b"move").unwrap();
            stage(
                root,
                vec![PlannedFile {
                    kind,
                    source: None,
                    path_before: Some(path("move.txt")),
                    path_after: Some(path("moved.txt")),
                    content_before: Some(ContentId::Blake3(*blake3::hash(b"move").as_bytes())),
                    content_after: Some(ContentId::Blake3(*blake3::hash(b"move").as_bytes())),
                    mode_before: Some(mode(&root.join("move.txt"))),
                    bytes_before: Some(b"move".to_vec()),
                    bytes_after: Some(b"move".to_vec()),
                    edits: vec![],
                }],
            )
        }
        PlannedFileKind::Delete => {
            fs::write(root.join("delete.txt"), b"delete").unwrap();
            stage(
                root,
                vec![PlannedFile {
                    kind,
                    source: None,
                    path_before: Some(path("delete.txt")),
                    path_after: None,
                    content_before: Some(ContentId::Blake3(*blake3::hash(b"delete").as_bytes())),
                    content_after: None,
                    mode_before: Some(mode(&root.join("delete.txt"))),
                    bytes_before: Some(b"delete".to_vec()),
                    bytes_after: None,
                    edits: vec![],
                }],
            )
        }
    }
}

#[test]
fn failpoint_matrix_recovers_each_operation_boundary() {
    for kind in [
        PlannedFileKind::Create,
        PlannedFileKind::Replace,
        PlannedFileKind::Move,
        PlannedFileKind::Delete,
    ] {
        for point in [
            CommitFailpoint::AfterJournal,
            CommitFailpoint::BeforeOperation(0),
            CommitFailpoint::AfterOperation(0),
        ] {
            let label = format!("matrix_{kind:?}_{point:?}");
            let root = temp_dir(&label);
            let state = temp_dir(&format!("{label}_state"));
            let transaction = one_operation_stage(&root, kind);
            let engine = CommitEngine::open(&root, &state).unwrap();
            assert!(matches!(
                engine.commit_with_failpoint(&transaction, Some(point)),
                Err(CommitRefusal::Failpoint { .. })
            ));
            let receipt = engine.recover(transaction.id).unwrap();
            assert_eq!(receipt.stage_id, transaction.id);
            match kind {
                PlannedFileKind::Create => {
                    assert_eq!(fs::read(root.join("created.txt")).unwrap(), b"created")
                }
                PlannedFileKind::Replace => {
                    assert_eq!(fs::read(root.join("replace.txt")).unwrap(), b"new")
                }
                PlannedFileKind::Move => {
                    assert_eq!(fs::read(root.join("moved.txt")).unwrap(), b"move");
                    assert!(!root.join("move.txt").exists());
                }
                PlannedFileKind::Delete => assert!(!root.join("delete.txt").exists()),
            }
            let _ = fs::remove_dir_all(root);
            let _ = fs::remove_dir_all(state);
        }
    }
}

fn run_git(root: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn git_worktree_commit_preserves_head_index_and_refs() {
    let root = temp_dir("git");
    run_git(&root, &["init", "-q"]);
    run_git(&root, &["config", "user.email", "test@example.com"]);
    run_git(&root, &["config", "user.name", "Soopy Test"]);
    fs::write(root.join("tracked.txt"), b"old").unwrap();
    run_git(&root, &["add", "tracked.txt"]);
    run_git(&root, &["commit", "-qm", "initial"]);
    let head = run_git(&root, &["rev-parse", "HEAD"]);
    let tree = run_git(&root, &["write-tree"]);
    let refs = run_git(&root, &["show-ref"]);
    let source_root = SourceRoot::discover_git(&root).unwrap();
    let root_id = match source_root {
        SourceRoot::GitWorktree(git) => SourceRootId::GitWorktree {
            repository: git.repository.identity,
            worktree: git.repository.worktree,
        },
        SourceRoot::Directory(_) => unreachable!(),
    };
    let oid = run_git(&root, &["hash-object", "tracked.txt"]);
    fs::write(root.join("new-content"), b"new").unwrap();
    let new_oid = run_git(&root, &["hash-object", "new-content"]);
    fs::remove_file(root.join("new-content")).unwrap();
    let mut store = InMemoryStageStore::new();
    let id = store
        .save(MutationPlan {
            root: root_id,
            files: vec![PlannedFile {
                kind: PlannedFileKind::Replace,
                source: None,
                path_before: Some(SourcePath::Git {
                    path: RepoPath(Arc::from("tracked.txt")),
                }),
                path_after: Some(SourcePath::Git {
                    path: RepoPath(Arc::from("tracked.txt")),
                }),
                content_before: Some(ContentId::GitBlob(ObjectId(Arc::from(oid.as_str())))),
                content_after: Some(ContentId::GitBlob(ObjectId(Arc::from(new_oid.as_str())))),
                mode_before: Some(mode(&root.join("tracked.txt"))),
                bytes_before: Some(b"old".to_vec()),
                bytes_after: Some(b"new".to_vec()),
                edits: vec![],
            }],
        })
        .unwrap()
        .id;
    let transaction = store.load(id).unwrap().unwrap();
    let state = temp_dir("git_state");
    let engine = CommitEngine::open(&root, &state).unwrap();
    engine.commit(&transaction).unwrap();
    assert_eq!(run_git(&root, &["rev-parse", "HEAD"]), head);
    assert_eq!(run_git(&root, &["write-tree"]), tree);
    assert_eq!(run_git(&root, &["show-ref"]), refs);
    assert_eq!(fs::read(root.join("tracked.txt")).unwrap(), b"new");
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(state);
}

#[test]
fn journal_and_receipt_tampering_are_refused() {
    let root = temp_dir("tamper");
    let transaction = stage(
        &root,
        vec![{
            let file = root.join("tamper.txt");
            fs::write(&file, b"old").unwrap();
            replace_file(&root, "tamper.txt", b"old", b"new")
        }],
    );
    let state = temp_dir("tamper_state");
    let engine = CommitEngine::open(&root, &state).unwrap();
    assert!(matches!(
        engine.commit_with_failpoint(&transaction, Some(CommitFailpoint::AfterJournal)),
        Err(CommitRefusal::Failpoint { .. })
    ));
    fs::write(engine.journal_path_for(transaction.id), b"{}").unwrap();
    assert!(matches!(
        engine.recover(transaction.id),
        Err(CommitRefusal::Io { .. })
    ));
    let _ = fs::remove_file(engine.journal_path_for(transaction.id));
    engine.commit(&transaction).unwrap();
    let receipt_path = state
        .join("receipts")
        .join(format!("{}.json", transaction.id));
    let valid_bytes = fs::read(&receipt_path).unwrap();
    fs::write(&receipt_path, b"{}").unwrap();
    assert!(matches!(
        engine.commit(&transaction),
        Err(CommitRefusal::Io { .. })
    ));
    let valid = serde_json::from_slice::<soopy::CommitReceipt>(&valid_bytes).unwrap();
    fs::write(
        &receipt_path,
        serde_json::to_vec(&soopy::CommitReceipt {
            operations: vec![],
            applied_files: 0,
            ..valid
        })
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        engine.commit(&transaction),
        Err(CommitRefusal::ReceiptDiverged { .. })
    ));
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(state);
}

fn seed_mirror(root: &std::path::Path) {
    for index in 0..8 {
        fs::write(root.join(format!("file-{index}.txt")), format!("body {index}\n")).unwrap();
    }
}

fn dry_run_actions(root: &std::path::Path) -> soopy::StageRequest {
    let source_root = SourceRoot::open_directory(root).unwrap();
    let identity = source_root.directory().identity.clone();
    let root_id = SourceRootId::Directory {
        directory: identity.clone(),
    };
    let source = |name: &str| soopy::ActionSource::Directory {
        file: soopy::FileRef {
            directory: identity.clone(),
            path: RootPath(Arc::from(name)),
        },
    };
    let expected = |name: &str| ContentId::blake3(&fs::read(root.join(name)).unwrap());
    let mut actions = vec![soopy::SourceAction::Move {
        source: source("file-0.txt"),
        expected: expected("file-0.txt"),
        destination: path("moved/file-0.txt"),
    }];
    for index in 1..8 {
        let name = format!("file-{index}.txt");
        let handle = source(&name);
        actions.push(soopy::SourceAction::Replace {
            source: handle.clone(),
            expected: expected(&name),
            edits: vec![soopy::TextEdit {
                range: soopy::ActionSpan {
                    source: handle,
                    start: 0,
                    end: 4,
                },
                replacement: b"HEAD".to_vec(),
                producer: soopy::ActionProducer::unordered("soopy.test.dry_run"),
            }],
        });
    }
    soopy::StageRequest::new(root_id, actions)
}

fn tree_bytes(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut rows = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let entry = entry.unwrap();
            let entry_path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry_path);
                continue;
            }
            let relative = entry_path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            rows.push((relative, fs::read(&entry_path).unwrap()));
        }
    }
    rows.sort();
    rows
}

/// The dry_run engine drops device flushes only. Same request, same
/// previews, same applied operations, same resulting bytes.
#[test]
fn dry_run_commit_matches_the_durable_commit_on_the_same_request() {
    let durable_root = temp_dir("dry_run_durable_root");
    let durable_state = temp_dir("dry_run_durable_state");
    let dry_run_root = temp_dir("dry_run_root");
    let dry_run_state = temp_dir("dry_run_state");
    seed_mirror(&durable_root);
    seed_mirror(&dry_run_root);

    let mut durable_source = SourceRoot::open_directory(&durable_root).unwrap();
    let durable_request = dry_run_actions(&durable_root);
    let mut durable_store = soopy::DurableStageStore::open(durable_state.join("stages")).unwrap();
    let durable_sealed =
        soopy::stage_mutations(&mut durable_source, &durable_request, &mut durable_store).unwrap();
    let durable_stage = soopy::show_stage(&durable_store, durable_sealed.id)
        .unwrap()
        .unwrap();
    let durable_engine =
        CommitEngine::open(&durable_root, durable_state.join("commits")).unwrap();
    assert_eq!(durable_engine.durability(), soopy::Durability::Durable);
    let durable_receipt = durable_engine.commit(&durable_stage).unwrap();

    let mut dry_run_source = SourceRoot::open_directory(&dry_run_root).unwrap();
    let dry_run_request = dry_run_actions(&dry_run_root);
    let mut dry_run_store = InMemoryStageStore::new();
    let dry_run_sealed = soopy::stage_mutations(
        &mut dry_run_source,
        &dry_run_request,
        &mut dry_run_store,
    )
    .unwrap();
    let dry_run_stage = soopy::show_stage(&dry_run_store, dry_run_sealed.id)
        .unwrap()
        .unwrap();
    let dry_run_engine =
        CommitEngine::open_dry_run(&dry_run_root, dry_run_state.join("commits")).unwrap();
    assert_eq!(
        dry_run_engine.durability(),
        soopy::Durability::DryRun
    );
    let dry_run_receipt = dry_run_engine.commit(&dry_run_stage).unwrap();

    assert_eq!(durable_stage.previews, dry_run_stage.previews);
    assert_eq!(durable_receipt.applied_files, dry_run_receipt.applied_files);
    assert_eq!(durable_receipt.operations, dry_run_receipt.operations);
    assert_eq!(tree_bytes(&durable_root), tree_bytes(&dry_run_root));
    assert!(dry_run_root.join("moved/file-0.txt").is_file());
    assert!(!dry_run_root.join("file-0.txt").exists());
    assert_eq!(
        fs::read(dry_run_root.join("file-1.txt")).unwrap(),
        b"HEAD 1\n".to_vec()
    );

    // The dry_run engine still writes its receipt, so a replayed commit is
    // still an early-out rather than a second application.
    let replay = dry_run_engine.commit(&dry_run_stage).unwrap();
    assert_eq!(replay.operations, dry_run_receipt.operations);

    for directory in [
        durable_root,
        durable_state,
        dry_run_root,
        dry_run_state,
    ] {
        let _ = fs::remove_dir_all(directory);
    }
}
