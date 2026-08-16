use std::sync::Arc;

use soopy::{
    plan_mutations, ActionProducer, ActionSource, ActionSpan, CommitEngine, CommitFailpoint,
    ContentId, DurableStageStore, FileRef, ProducedEdit, ProducedEditBatch, RootPath, SourceAction,
    SourceRoot, SourceRootId, StageRefusal, StageRequest, StageStore,
};

fn content(bytes: &[u8]) -> ContentId {
    ContentId::Blake3(*blake3::hash(bytes).as_bytes())
}

fn temporary_root(label: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "soopy_source_mutations_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn producer_planner_durable_stage_commit_and_replay_share_one_stage_id() {
    let target = temporary_root("aggregate_target");
    let store_root = temporary_root("aggregate_store");
    let state_root = temporary_root("aggregate_state");
    std::fs::write(target.join("source.txt"), b"before\n").unwrap();

    let mut source_root = SourceRoot::open_directory(&target).unwrap();
    let directory = source_root.directory().identity.clone();
    let root = SourceRootId::Directory {
        directory: directory.clone(),
    };
    let source = ActionSource::Directory {
        file: FileRef {
            directory,
            path: RootPath(Arc::from("source.txt")),
        },
    };
    let produced = ProducedEditBatch::new(vec![
        ProducedEdit::new(
            ActionSpan {
                source: source.clone(),
                start: 0,
                end: 6,
            },
            b"after",
            ActionProducer::unordered("ast-grep").with_rule("rename"),
        ),
        ProducedEdit::new(
            ActionSpan {
                source: source.clone(),
                start: 0,
                end: 6,
            },
            b"after",
            ActionProducer::unordered("dl6").with_rule("rename"),
        ),
    ]);
    let edits = produced.into_text_edits().unwrap();
    let request = StageRequest::new(
        root.clone(),
        vec![SourceAction::Replace {
            source: source.clone(),
            expected: content(b"before\n"),
            edits,
        }],
    );

    let plan = plan_mutations(&mut source_root, &request).unwrap();
    assert_eq!(plan.files.len(), 1);
    assert_eq!(
        plan.files[0].bytes_after.as_deref(),
        Some(b"after\n".as_slice())
    );
    assert_eq!(plan.files[0].edits[0].producers.len(), 2);

    let mut store = DurableStageStore::open(&store_root).unwrap();
    let stage = store.save(plan).unwrap();
    let stage_id = stage.id;
    let loaded = store.load(stage_id).unwrap().unwrap();
    assert_eq!(loaded.id, stage_id);
    assert_eq!(
        loaded.files[0].bytes_after.as_deref(),
        Some(b"after\n".as_slice())
    );

    let engine = CommitEngine::open(&target, &state_root).unwrap();
    assert!(matches!(
        engine.commit_with_failpoint(&loaded, Some(CommitFailpoint::AfterJournal)),
        Err(soopy::CommitRefusal::Failpoint {
            point: CommitFailpoint::AfterJournal
        })
    ));
    assert_eq!(
        std::fs::read(target.join("source.txt")).unwrap(),
        b"before\n"
    );

    let recovered = engine.recover(stage_id).unwrap();
    assert_eq!(recovered.stage_id, stage_id);
    assert_eq!(recovered.applied_files, 1);
    assert_eq!(
        std::fs::read(target.join("source.txt")).unwrap(),
        b"after\n"
    );
    assert_eq!(engine.commit(&loaded).unwrap(), recovered);

    let _ = std::fs::remove_dir_all(target);
    let _ = std::fs::remove_dir_all(store_root);
    let _ = std::fs::remove_dir_all(state_root);
}

#[test]
fn aggregate_gate_rejects_stale_input_before_durable_stage() {
    let target = temporary_root("aggregate_stale");
    let store_root = temporary_root("aggregate_stale_store");
    std::fs::write(target.join("source.txt"), b"changed").unwrap();
    let mut source_root = SourceRoot::open_directory(&target).unwrap();
    let source = ActionSource::Directory {
        file: FileRef {
            directory: source_root.directory().identity.clone(),
            path: RootPath(Arc::from("source.txt")),
        },
    };
    let request = StageRequest::new(
        SourceRootId::Directory {
            directory: source_root.directory().identity.clone(),
        },
        vec![SourceAction::Replace {
            source: source.clone(),
            expected: content(b"before"),
            edits: ProducedEditBatch::new(vec![ProducedEdit::new(
                ActionSpan {
                    source,
                    start: 0,
                    end: 6,
                },
                b"after",
                ActionProducer::unordered("fixture"),
            )])
            .into_text_edits()
            .unwrap(),
        }],
    );
    let store = DurableStageStore::open(&store_root).unwrap();
    assert!(matches!(
        plan_mutations(&mut source_root, &request),
        Err(StageRefusal::Stale { .. })
    ));
    assert_eq!(store.blob_count().unwrap(), 0);
    let _ = std::fs::remove_dir_all(target);
    let _ = std::fs::remove_dir_all(store_root);
}
