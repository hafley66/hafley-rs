use std::sync::Arc;

use soopy::{
    plan_mutations, ActionProducer, ActionSource, ActionSpan, ContentId, FileRef, ProducedEdit,
    ProducedEditBatch, RootPath, SourceAction, SourceRoot, SourceRootId, StageRefusal,
};

fn temporary_root(label: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "soopy_producer_planner_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn content(bytes: &[u8]) -> ContentId {
    ContentId::Blake3(*blake3::hash(bytes).as_bytes())
}

#[test]
fn producer_batch_reaches_planner_with_provenance_and_conflicts() {
    let directory = temporary_root("combined");
    std::fs::write(directory.join("source.txt"), b"abc").unwrap();
    let mut root = SourceRoot::open_directory(&directory).unwrap();
    let root_id = SourceRootId::Directory {
        directory: root.directory().identity.clone(),
    };
    let source = ActionSource::Directory {
        file: FileRef {
            directory: root.directory().identity.clone(),
            path: RootPath(Arc::from("source.txt")),
        },
    };
    let equivalent = ProducedEditBatch::new(vec![
        ProducedEdit::new(
            ActionSpan {
                source: source.clone(),
                start: 0,
                end: 1,
            },
            b"x".to_vec(),
            ActionProducer::unordered("ast-grep").with_rule("rename"),
        ),
        ProducedEdit::new(
            ActionSpan {
                source: source.clone(),
                start: 0,
                end: 1,
            },
            b"x".to_vec(),
            ActionProducer::unordered("dl6").with_rule("rename"),
        ),
    ]);
    let edits = equivalent.into_text_edits().unwrap();
    let request = soopy::StageRequest::new(
        root_id.clone(),
        vec![SourceAction::Replace {
            source: source.clone(),
            expected: content(b"abc"),
            edits,
        }],
    );
    let plan = plan_mutations(&mut root, &request).unwrap();
    assert_eq!(
        plan.files[0].bytes_after.as_deref(),
        Some(b"xbc".as_slice())
    );
    assert_eq!(plan.files[0].edits[0].producers.len(), 2);
    assert_eq!(
        plan.files[0].edits[0].producers[0].rule.as_deref(),
        Some("rename")
    );
    assert_eq!(
        plan.files[0].edits[0].producers[1].rule.as_deref(),
        Some("rename")
    );

    let conflicting = ProducedEditBatch::new(vec![
        ProducedEdit::new(
            ActionSpan {
                source: source.clone(),
                start: 0,
                end: 1,
            },
            b"x".to_vec(),
            ActionProducer::unordered("ast-grep"),
        ),
        ProducedEdit::new(
            ActionSpan {
                source: source.clone(),
                start: 0,
                end: 1,
            },
            b"y".to_vec(),
            ActionProducer::unordered("biome"),
        ),
    ]);
    let conflict_request = soopy::StageRequest::new(
        root_id,
        vec![SourceAction::Replace {
            source,
            expected: content(b"abc"),
            edits: conflicting.into_text_edits().unwrap(),
        }],
    );
    assert!(matches!(
        plan_mutations(&mut root, &conflict_request),
        Err(StageRefusal::Conflict { .. })
    ));
    std::fs::remove_dir_all(directory).unwrap();
}
