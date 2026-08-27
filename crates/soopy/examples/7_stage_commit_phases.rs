//! Phase timings for the sprefa `extract move` dry-run shape: a directory
//! mirror, one Move plus 26 Replace actions, staged durably and committed.
//!
//! Run with `RUST_LOG=soopy=debug` so every phase event prints.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use soopy::{
    ActionProducer, ActionSource, ActionSpan, CommitEngine, ContentId, DirectoryId,
    DurableStageStore, FileRef, InMemoryStageStore, RootPath, SourceAction, SourcePath, SourceRoot,
    SourceRootId, StageRequest, StageStore, StagedSourceTransaction, TextEdit,
};

const PRODUCER: &str = "soopy.example.stage_commit_phases";

fn main() {
    hafley_observe::init(
        hafley_observe::Config::from_env(
            "soopy-phases",
            env!("CARGO_PKG_VERSION"),
            "soopy=debug",
            false,
        )
        .expect("log format"),
    )
    .expect("observability");

    let args: Vec<_> = std::env::args().collect();
    let corpus = numeric_arg(&args, "--files").unwrap_or(282);
    let replaces = numeric_arg(&args, "--replaces").unwrap_or(26);
    let dry_run = args.iter().any(|value| value == "--dry-run");

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let base = std::env::temp_dir().join(format!("soopy_phases_{}_{unique}", std::process::id()));
    let root = base.join("mirror");
    let state = base.join("state");
    fs::create_dir_all(&root).expect("create mirror");

    for index in 0..corpus {
        fs::write(root.join(format!("file-{index}.txt")), body(index)).expect("seed file");
    }
    // The mirror is a real git checkout, matching sprefa's temp mirror; soopy
    // opens it as a directory root, so no git mechanics run on this path.
    let _ = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status();

    let mut source_root = SourceRoot::open_directory(&root).expect("open mirror");
    let identity = source_root.directory().identity.clone();
    let root_id = SourceRootId::Directory {
        directory: identity.clone(),
    };

    let mut actions = Vec::with_capacity(replaces + 1);
    actions.push(SourceAction::Move {
        source: file_source(&identity, "file-0.txt"),
        expected: expected(&root, "file-0.txt"),
        destination: SourcePath::Directory {
            path: RootPath(Arc::from("moved/file-0.txt")),
        },
    });
    for index in 1..=replaces {
        let relative = format!("file-{index}.txt");
        let source = file_source(&identity, &relative);
        actions.push(SourceAction::Replace {
            source: source.clone(),
            expected: expected(&root, &relative),
            edits: vec![TextEdit {
                range: ActionSpan {
                    source,
                    start: 0,
                    end: 4,
                },
                replacement: b"HEAD".to_vec(),
                producer: ActionProducer::unordered(PRODUCER),
            }],
        });
    }

    let request = StageRequest::new(root_id, actions);
    let (stage, stage_ms, load_ms, staged_blobs) = if dry_run {
        let (stage, stage_ms, load_ms) =
            stage_and_load(&mut source_root, &request, &mut InMemoryStageStore::new());
        (stage, stage_ms, load_ms, None)
    } else {
        let mut store = DurableStageStore::open(state.join("stages")).expect("open stage store");
        let blobs = store.blobs_dir();
        let (stage, stage_ms, load_ms) = stage_and_load(&mut source_root, &request, &mut store);
        (stage, stage_ms, load_ms, Some(blobs))
    };

    let engine = if dry_run {
        CommitEngine::open_dry_run(&root, state.join("commits"))
    } else {
        CommitEngine::open(&root, state.join("commits"))
    }
    .expect("open commit engine");
    let engine = match staged_blobs {
        Some(blobs) => engine.with_staged_blobs(blobs),
        None => engine,
    };
    let commit_started = Instant::now();
    let receipt = engine.commit(&stage).expect("commit");
    let commit_ms = elapsed_ms(commit_started);

    println!(
        "{}",
        serde_json::json!({
            "mode": if dry_run { "dry_run" } else { "durable" },
            "corpus_files": corpus,
            "staged_files": stage.files.len(),
            "applied_files": receipt.applied_files,
            "stage_ms": stage_ms,
            "load_ms": load_ms,
            "commit_ms": commit_ms,
            "total_ms": stage_ms + load_ms + commit_ms,
            "sync": {
                "data": soopy::device_sync_counts().data,
                "fences": soopy::device_sync_counts().fences,
                "flushes": soopy::device_sync_counts().flushes,
            },
        })
    );

    let _ = fs::remove_dir_all(&base);
}

fn stage_and_load<S: StageStore>(
    source_root: &mut SourceRoot,
    request: &StageRequest,
    store: &mut S,
) -> (StagedSourceTransaction, f64, f64) {
    let staged_started = Instant::now();
    let sealed = soopy::stage_mutations(source_root, request, store).expect("stage");
    let stage_ms = elapsed_ms(staged_started);
    let load_started = Instant::now();
    let stage = soopy::show_stage(store, sealed.id)
        .expect("load stage")
        .expect("stage present");
    (stage, stage_ms, elapsed_ms(load_started))
}

fn body(index: usize) -> Vec<u8> {
    format!("line {index}\n").repeat(64).into_bytes()
}

fn numeric_arg(args: &[String], flag: &str) -> Option<usize> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .and_then(|pair| pair[1].parse().ok())
}

fn file_source(identity: &DirectoryId, relative: &str) -> ActionSource {
    ActionSource::Directory {
        file: FileRef {
            directory: identity.clone(),
            path: RootPath(Arc::from(relative)),
        },
    }
}

fn expected(root: &Path, relative: &str) -> ContentId {
    ContentId::blake3(&fs::read(root.join(relative)).expect("read seed"))
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}
