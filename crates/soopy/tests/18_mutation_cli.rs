use std::sync::Arc;

use soopy::{
    ActionProducer, ActionSource, ActionSpan, CommitEngine, CommitFailpoint, ContentId,
    DurableStageStore, FileRef, RootPath, SourceAction, SourceRoot, SourceRootId, StageRequest,
    StageStore, TextEdit,
};
use tempfile::tempdir;

fn run(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_soopy"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn stage_commit_and_recover_are_available_through_the_soopy_cli() {
    let target = tempdir().unwrap();
    let state = tempdir().unwrap();
    let store = tempdir().unwrap();
    std::fs::write(target.path().join("source.txt"), b"before\n").unwrap();

    let source_root = SourceRoot::open_directory(target.path()).unwrap();
    let directory = source_root.directory().identity.clone();
    let source = ActionSource::Directory {
        file: FileRef {
            directory: directory.clone(),
            path: RootPath(Arc::from("source.txt")),
        },
    };
    let request = StageRequest::new(
        SourceRootId::Directory { directory },
        vec![SourceAction::Replace {
            source: source.clone(),
            expected: ContentId::Blake3(*blake3::hash(b"before\n").as_bytes()),
            edits: vec![TextEdit {
                range: ActionSpan {
                    source,
                    start: 0,
                    end: 6,
                },
                replacement: b"after".to_vec(),
                producer: ActionProducer::unordered("fixture"),
            }],
        }],
    );
    let request_file = state.path().join("request.json");
    std::fs::write(&request_file, serde_json::to_vec(&request).unwrap()).unwrap();

    let staged = run(&[
        "--repo",
        target.path().to_str().unwrap(),
        "stage",
        "--store",
        store.path().to_str().unwrap(),
        "--request",
        request_file.to_str().unwrap(),
    ]);
    assert!(
        staged.status.success(),
        "{}",
        String::from_utf8_lossy(&staged.stderr)
    );
    assert_eq!(
        std::fs::read(target.path().join("source.txt")).unwrap(),
        b"before\n"
    );
    let stage: serde_json::Value = serde_json::from_slice(&staged.stdout).unwrap();
    let id = stage["stage_id"].as_str().unwrap();
    assert_eq!(stage["preview"]["previews"].as_array().unwrap().len(), 1);

    let committed = run(&[
        "--repo",
        target.path().to_str().unwrap(),
        "commit",
        id,
        "--store",
        store.path().to_str().unwrap(),
        "--state",
        state.path().join("commit-state").to_str().unwrap(),
    ]);
    assert!(
        committed.status.success(),
        "{}",
        String::from_utf8_lossy(&committed.stderr)
    );
    assert_eq!(
        std::fs::read(target.path().join("source.txt")).unwrap(),
        b"after\n"
    );

    std::fs::write(target.path().join("interrupted.txt"), b"before\n").unwrap();
    let source_root = SourceRoot::open_directory(target.path()).unwrap();
    let directory = source_root.directory().identity.clone();
    let source = ActionSource::Directory {
        file: FileRef {
            directory: directory.clone(),
            path: RootPath(Arc::from("interrupted.txt")),
        },
    };
    let recovery_request = StageRequest::new(
        SourceRootId::Directory { directory },
        vec![SourceAction::Replace {
            source: source.clone(),
            expected: ContentId::Blake3(*blake3::hash(b"before\n").as_bytes()),
            edits: vec![TextEdit {
                range: ActionSpan {
                    source,
                    start: 0,
                    end: 6,
                },
                replacement: b"after".to_vec(),
                producer: ActionProducer::unordered("fixture-recovery"),
            }],
        }],
    );
    std::fs::write(
        &request_file,
        serde_json::to_vec(&recovery_request).unwrap(),
    )
    .unwrap();
    let staged_recovery = run(&[
        "--repo",
        target.path().to_str().unwrap(),
        "stage",
        "--store",
        store.path().to_str().unwrap(),
        "--request",
        request_file.to_str().unwrap(),
    ]);
    assert!(
        staged_recovery.status.success(),
        "{}",
        String::from_utf8_lossy(&staged_recovery.stderr)
    );
    let stage: serde_json::Value = serde_json::from_slice(&staged_recovery.stdout).unwrap();
    let recovery_id = stage["stage_id"].as_str().unwrap();
    let durable_store = DurableStageStore::open(store.path()).unwrap();
    let transaction = durable_store
        .load(recovery_id.parse().unwrap())
        .unwrap()
        .unwrap();
    let engine = CommitEngine::open(target.path(), state.path().join("commit-state")).unwrap();
    assert!(matches!(
        engine.commit_with_failpoint(&transaction, Some(CommitFailpoint::AfterJournal)),
        Err(soopy::CommitRefusal::Failpoint {
            point: CommitFailpoint::AfterJournal
        })
    ));
    assert_eq!(
        std::fs::read(target.path().join("interrupted.txt")).unwrap(),
        b"before\n"
    );

    let recovered = run(&[
        "--repo",
        target.path().to_str().unwrap(),
        "recover",
        recovery_id,
        "--state",
        state.path().join("commit-state").to_str().unwrap(),
    ]);
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&recovered.stdout).unwrap();
    assert_eq!(receipt["stage_id"].as_array().unwrap().len(), 32);
    assert_eq!(receipt["applied_files"], 1);
}
