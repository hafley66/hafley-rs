use std::fs;
use std::sync::Arc;
use std::time::Instant;

use soopy::{
    CommitEngine, ContentId, FileModeObservation, InMemoryStageStore, MutationPlan, PlannedFile,
    PlannedFileKind, RootPath, SourcePath, SourceRoot, SourceRootId, StageStore,
};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let files = args
        .windows(2)
        .find(|pair| pair[0] == "--files")
        .and_then(|pair| pair[1].parse::<usize>().ok())
        .unwrap_or(1_000);
    let root = std::env::temp_dir().join(format!("soopy_commit_scale_{}", std::process::id()));
    let state =
        std::env::temp_dir().join(format!("soopy_commit_scale_state_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&state);
    fs::create_dir_all(&root).unwrap();
    let mut planned = Vec::with_capacity(files);
    for index in 0..files {
        let name = format!("file-{index}.txt");
        let before = format!("before-{index}\n").into_bytes();
        let after = format!("after-{index}\n").into_bytes();
        fs::write(root.join(&name), &before).unwrap();
        planned.push(PlannedFile {
            kind: PlannedFileKind::Replace,
            source: None,
            path_before: Some(SourcePath::Directory {
                path: RootPath(Arc::from(name.as_str())),
            }),
            path_after: Some(SourcePath::Directory {
                path: RootPath(Arc::from(name.as_str())),
            }),
            content_before: Some(ContentId::Blake3(*blake3::hash(&before).as_bytes())),
            content_after: Some(ContentId::Blake3(*blake3::hash(&after).as_bytes())),
            mode_before: Some(FileModeObservation {
                readonly: false,
                unix_mode: None,
            }),
            bytes_before: Some(before),
            bytes_after: Some(after),
            edits: vec![],
        });
    }
    let root_id = match SourceRoot::open_directory(&root).unwrap() {
        SourceRoot::Directory(directory) => SourceRootId::Directory {
            directory: directory.identity,
        },
        SourceRoot::GitWorktree(_) => unreachable!(),
    };
    let mut store = InMemoryStageStore::new();
    let stage_id = store
        .save(MutationPlan {
            root: root_id,
            files: planned,
        })
        .unwrap()
        .id;
    let stage = store.load(stage_id).unwrap().unwrap();
    let engine = CommitEngine::open(&root, &state).unwrap();
    let started = Instant::now();
    let receipt = engine.commit(&stage).unwrap();
    println!(
        "{}",
        serde_json::json!({
            "files": receipt.applied_files,
            "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
            "journal_bytes": receipt.journal_bytes,
            "checkpoint_bytes": receipt.checkpoint_bytes,
            "stage_id": receipt.stage_id.to_string(),
            "journal": engine.journal_path_for(receipt.stage_id),
        })
    );
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(state);
}
