//! One test per binary: `device_sync_counts` is process-wide, so a second
//! test running in parallel would add its own syncs to the tally.

use std::fs;
use std::sync::Arc;

use soopy::{
    device_sync_counts, CommitEngine, ContentId, DurableStageStore, FileModeObservation,
    MutationPlan, PlannedFile, PlannedFileKind, RootPath, SourcePath, SourceRoot, SourceRootId,
    StageStore,
};

fn temp_dir(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("soopy_flushes_{label}_{}", std::process::id()));
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

fn seeded(root: &std::path::Path, name: &str, bytes: &[u8]) -> FileModeObservation {
    fs::write(root.join(name), bytes).unwrap();
    mode(&root.join(name))
}

fn blake3_id(bytes: &[u8]) -> ContentId {
    ContentId::Blake3(*blake3::hash(bytes).as_bytes())
}

#[test]
fn a_durable_stage_and_commit_pay_two_device_flushes() {
    let root = temp_dir("root");
    let state = temp_dir("state");
    let replace_mode = seeded(&root, "replace.txt", b"old");
    let move_mode = seeded(&root, "move.txt", b"carried");
    let delete_mode = seeded(&root, "delete.txt", b"gone");

    let root_id = match SourceRoot::open_directory(&root).unwrap() {
        SourceRoot::Directory(directory) => SourceRootId::Directory {
            directory: directory.identity,
        },
        SourceRoot::GitWorktree(_) => unreachable!(),
    };
    let plan = MutationPlan {
        root: root_id,
        files: vec![
            PlannedFile {
                kind: PlannedFileKind::Create,
                source: None,
                path_before: None,
                path_after: Some(path("created.txt")),
                content_before: None,
                content_after: Some(blake3_id(b"created")),
                mode_before: None,
                bytes_before: None,
                bytes_after: Some(b"created".to_vec()),
                edits: vec![],
            },
            PlannedFile {
                kind: PlannedFileKind::Replace,
                source: None,
                path_before: Some(path("replace.txt")),
                path_after: Some(path("replace.txt")),
                content_before: Some(blake3_id(b"old")),
                content_after: Some(blake3_id(b"new")),
                mode_before: Some(replace_mode),
                bytes_before: Some(b"old".to_vec()),
                bytes_after: Some(b"new".to_vec()),
                edits: vec![],
            },
            PlannedFile {
                kind: PlannedFileKind::Move,
                source: None,
                path_before: Some(path("move.txt")),
                path_after: Some(path("moved.txt")),
                content_before: Some(blake3_id(b"carried")),
                content_after: Some(blake3_id(b"carried")),
                mode_before: Some(move_mode),
                bytes_before: Some(b"carried".to_vec()),
                bytes_after: Some(b"carried".to_vec()),
                edits: vec![],
            },
            PlannedFile {
                kind: PlannedFileKind::Delete,
                source: None,
                path_before: Some(path("delete.txt")),
                path_after: None,
                content_before: Some(blake3_id(b"gone")),
                content_after: None,
                mode_before: Some(delete_mode),
                bytes_before: Some(b"gone".to_vec()),
                bytes_after: None,
                edits: vec![],
            },
        ],
    };

    let mut store = DurableStageStore::open(state.join("stages")).unwrap();
    let before = device_sync_counts();
    let sealed = store.save(plan).unwrap();
    let stage = store.load(sealed.id).unwrap().unwrap();
    let staged = device_sync_counts() - before;

    let engine = CommitEngine::open(&root, state.join("commits"))
        .unwrap()
        .with_staged_blobs(store.blobs_dir());
    let receipt = engine.commit(&stage).unwrap();
    let committed = device_sync_counts() - before - staged;

    assert_eq!(receipt.applied_files, 4);
    assert_eq!(fs::read(root.join("created.txt")).unwrap(), b"created");
    assert_eq!(fs::read(root.join("replace.txt")).unwrap(), b"new");
    assert_eq!(fs::read(root.join("moved.txt")).unwrap(), b"carried");
    assert!(!root.join("delete.txt").exists());

    // Three blob bodies and the manifest at fsync, one fence so the manifest
    // cannot land before the blobs it names, one flush to settle them.
    assert_eq!(staged.data, 4);
    assert_eq!(staged.fences, 1);
    assert_eq!(staged.flushes, 1);

    // Payloads are links, so the only bodies synced are the journal, the two
    // written targets and the receipt, plus one sync of the one parent
    // directory. Fences: payloads, journal, targets. Flush: the receipt.
    assert_eq!(committed.data, 5);
    assert_eq!(committed.fences, 3);
    assert_eq!(committed.flushes, 1);

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(state);
}
