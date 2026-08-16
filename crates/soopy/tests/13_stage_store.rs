use std::sync::Arc;

use soopy::{
    discard_stage, show_stage, CleanupPolicy, ContentId, DurableStageStore, FilePreview,
    InMemoryStageStore, MutationPlan, PlannedFile, PlannedFileKind, SourcePath, SourceRootId,
    StageId, StageStore,
};

fn root() -> SourceRootId {
    SourceRootId::Directory {
        directory: soopy::DirectoryId(Arc::from("fixture-root")),
    }
}

fn path(name: &str) -> SourcePath {
    SourcePath::Directory {
        path: soopy::RootPath(Arc::from(name)),
    }
}

fn plan() -> MutationPlan {
    MutationPlan {
        root: root(),
        files: vec![PlannedFile {
            kind: PlannedFileKind::Replace,
            source: None,
            path_before: Some(path("a.txt")),
            path_after: Some(path("a.txt")),
            content_before: Some(ContentId::Blake3(*blake3::hash(b"old\n").as_bytes())),
            content_after: Some(ContentId::Blake3(*blake3::hash(b"new\n").as_bytes())),
            mode_before: None,
            bytes_before: Some(b"old\n".to_vec()),
            bytes_after: Some(b"new\n".to_vec()),
            edits: vec![],
        }],
    }
}

#[test]
fn in_memory_store_deduplicates_and_discards_without_deleting_shared_blob() {
    let mut store = InMemoryStageStore::new();
    let first = store.save(plan()).unwrap();
    let second = store.save(plan()).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(store.stage_count(), 1);
    assert_eq!(store.blob_count(), 1);
    assert!(discard_stage(&mut store, first.id).unwrap());
    assert_eq!(store.blob_count(), 1);
    assert_eq!(store.cleanup(&CleanupPolicy::default()).unwrap(), 0);
}

#[test]
fn durable_store_reopens_without_target_reads_and_recomputes_id() {
    let root = std::env::temp_dir().join(format!(
        "soopy_stage_store_{}_{}",
        std::process::id(),
        StageId([7; 32])
    ));
    let _ = std::fs::remove_dir_all(&root);
    let mut store = DurableStageStore::open(&root).unwrap();
    let stage = store.save(plan()).unwrap();
    let manifest = stage.canonical_manifest_bytes().unwrap();
    drop(store);
    let store = DurableStageStore::open(&root).unwrap();
    let reopened = show_stage(&store, stage.id).unwrap().unwrap();
    assert_eq!(reopened.id, stage.id);
    assert_eq!(reopened.recompute_id().unwrap(), stage.id);
    assert_eq!(reopened.canonical_manifest_bytes().unwrap(), manifest);
    assert_eq!(
        reopened.files[0].bytes_after.as_deref(),
        Some(b"new\n".as_slice())
    );
    assert_eq!(reopened.previews[0].before_bytes, 4);
    assert_eq!(reopened.previews[0].after_bytes, 4);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn durable_save_repairs_a_corrupt_existing_blob() {
    let root = std::env::temp_dir().join(format!(
        "soopy_stage_store_corrupt_{}_{}",
        std::process::id(),
        StageId([8; 32])
    ));
    let _ = std::fs::remove_dir_all(&root);
    let mut store = DurableStageStore::open(&root).unwrap();
    let stage = store.save(plan()).unwrap();
    let blob = stage.files[0].staged_bytes.unwrap();
    std::fs::write(root.join("blobs").join(blob.to_string()), b"corrupt").unwrap();
    store.save(plan()).unwrap();
    let loaded = store.load(stage.id).unwrap().unwrap();
    assert_eq!(
        loaded.files[0].bytes_after.as_deref(),
        Some(b"new\n".as_slice())
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn presentation_is_separate_from_stage_id() {
    let mut store = InMemoryStageStore::new();
    let mut first = store.save(plan()).unwrap();
    let id = first.id;
    first.previews.push(FilePreview {
        kind: PlannedFileKind::Create,
        path_before: None,
        path_after: Some(path("presentation-only")),
        summary: "changed style".into(),
        unified: Some("different".into()),
        binary: false,
        before_bytes: 0,
        after_bytes: 0,
    });
    assert_eq!(first.recompute_id().unwrap(), id);
}

#[test]
fn explicit_gc_requires_both_manifest_and_blob_policy() {
    let mut store = InMemoryStageStore::new();
    let stage = store.save(plan()).unwrap();
    let policy = CleanupPolicy {
        remove_manifests: true,
        remove_unreferenced_blobs: true,
        ..CleanupPolicy::default()
    };
    assert_eq!(store.cleanup(&policy).unwrap(), 2);
    assert!(store.load(stage.id).unwrap().is_none());
}

#[test]
fn preview_fixtures_cover_operations_binary_and_trailing_newline() {
    let make =
        |kind, before: &[u8], after: &[u8], from: Option<&str>, to: Option<&str>| PlannedFile {
            kind,
            source: None,
            path_before: from.map(path),
            path_after: to.map(path),
            content_before: (!before.is_empty())
                .then(|| ContentId::Blake3(*blake3::hash(before).as_bytes())),
            content_after: (!after.is_empty())
                .then(|| ContentId::Blake3(*blake3::hash(after).as_bytes())),
            mode_before: None,
            bytes_before: (!before.is_empty()).then(|| before.to_vec()),
            bytes_after: (!after.is_empty()).then(|| after.to_vec()),
            edits: vec![],
        };
    let mut store = InMemoryStageStore::new();
    let stage = store
        .save(MutationPlan {
            root: root(),
            files: vec![
                make(
                    PlannedFileKind::Create,
                    b"",
                    b"create\n",
                    None,
                    Some("create.txt"),
                ),
                make(
                    PlannedFileKind::Replace,
                    b"old\n",
                    b"new",
                    Some("replace.txt"),
                    Some("replace.txt"),
                ),
                make(
                    PlannedFileKind::Move,
                    b"move\n",
                    b"move\n",
                    Some("from.txt"),
                    Some("to.txt"),
                ),
                make(
                    PlannedFileKind::Delete,
                    b"delete\n",
                    b"",
                    Some("delete.txt"),
                    None,
                ),
                make(
                    PlannedFileKind::Replace,
                    &[0, 159, 255],
                    &[1, 2, 3],
                    Some("binary"),
                    Some("binary"),
                ),
            ],
        })
        .unwrap();
    let loaded = store.load(stage.id).unwrap().unwrap();
    assert_eq!(
        loaded
            .previews
            .iter()
            .filter(|preview| preview.binary)
            .count(),
        1
    );
    assert_eq!(
        loaded
            .previews
            .iter()
            .map(|preview| (preview.summary.as_str(), preview.unified.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            ("delete delete.txt", Some("--- original\n+++ modified\n@@ -1 +0,0 @@\n-delete\n")),
            ("update binary (3 bytes)", None),
            ("create create.txt (7 bytes)", Some("--- original\n+++ modified\n@@ -0,0 +1 @@\n+create\n")),
            ("update replace.txt (3 bytes)", Some("--- original\n+++ modified\n@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file\n")),
            ("move from.txt -> to.txt (5 bytes)", Some("--- original\n+++ modified\n")),
        ]
    );
}
