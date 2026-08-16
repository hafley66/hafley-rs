use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{ensure, Context, Result};
use clap::Parser;
use serde::Serialize;
use soopy::{
    plan_mutations, ActionProducer, ActionSource, ActionSpan, CommitEngine, CommitFailpoint,
    ContentId, DurableStageStore, FileRef, ProducedEdit, ProducedEditBatch, RootPath, SourceAction,
    SourceRoot, SourceRootId, StageRequest, StageStore,
};
use sysinfo::{Pid, ProcessesToUpdate, System};

#[derive(Debug, Parser)]
#[command(about = "Run the complete source mutation pipeline and emit one JSON receipt")]
struct Args {
    #[arg(long, default_value_t = 1_000)]
    files: usize,
    #[arg(long, default_value_t = 100)]
    edits_per_file: usize,
    #[arg(long, default_value_t = 4_096)]
    bytes_per_file: usize,
    #[arg(long)]
    store: Option<PathBuf>,
    #[arg(long)]
    receipt: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct Receipt {
    schema_version: u32,
    scenario: &'static str,
    root_kind: &'static str,
    repository_count: usize,
    root_count: usize,
    files: usize,
    input_edits: usize,
    source_bytes: usize,
    result_bytes: usize,
    phases: PhaseTimes,
    rss_samples: Vec<RssSample>,
    peak_rss_bytes: Option<u64>,
    allocation_bytes: Option<u64>,
    stage: StageMetrics,
    commit: CommitMetrics,
}

#[derive(Debug, Serialize)]
struct PhaseTimes {
    fixture_ms: f64,
    producer_ms: f64,
    planner_ms: f64,
    stage_ms: f64,
    load_ms: f64,
    commit_ms: f64,
    replay_ms: f64,
    total_ms: f64,
}

#[derive(Debug, Serialize)]
struct RssSample {
    phase: &'static str,
    bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
struct StageMetrics {
    id: String,
    unique_blobs: usize,
    unique_blob_bytes: usize,
    dedup_hits: usize,
    manifest_bytes: u64,
    previews: usize,
}

#[derive(Debug, Serialize)]
struct CommitMetrics {
    applied_files: usize,
    journal_bytes: u64,
    recovered_stage_id: String,
    idempotent_replay: bool,
}

fn rss(system: &mut System) -> Option<u64> {
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|process| process.memory())
}

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

fn temporary_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "soopy_source_mutations_{label}_{}",
        std::process::id()
    ))
}

fn content(bytes: &[u8]) -> ContentId {
    ContentId::Blake3(*blake3::hash(bytes).as_bytes())
}

fn source_for(directory: &soopy::DirectoryId, name: &str) -> ActionSource {
    ActionSource::Directory {
        file: FileRef {
            directory: directory.clone(),
            path: RootPath(Arc::from(name)),
        },
    }
}

fn write_receipt(path: &Path, receipt: &Receipt) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(receipt)?)?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(args.edits_per_file > 0, "--edits-per-file must be positive");
    ensure!(
        args.bytes_per_file >= 8 + args.edits_per_file.saturating_sub(1) * 2,
        "--bytes-per-file must leave room for non-overlapping edits"
    );
    let total_started = Instant::now();
    let target = temporary_root("target");
    let store_is_ephemeral = args.store.is_none();
    let store_root = args
        .store
        .clone()
        .unwrap_or_else(|| temporary_root("store"));
    let state_root = temporary_root("state");
    let _ = std::fs::remove_dir_all(&target);
    if store_is_ephemeral {
        let _ = std::fs::remove_dir_all(&store_root);
    }
    let _ = std::fs::remove_dir_all(&state_root);
    std::fs::create_dir_all(&target)?;
    let mut system = System::new();
    let mut rss_samples = Vec::new();

    let fixture_started = Instant::now();
    let mut before_by_file = Vec::with_capacity(args.files);
    for index in 0..args.files {
        let before = vec![b'a'; args.bytes_per_file];
        let name = format!("file-{index:05}.txt");
        std::fs::write(target.join(&name), &before)
            .with_context(|| format!("write fixture {}", target.join(&name).display()))?;
        before_by_file.push((name, before));
    }
    rss_samples.push(RssSample {
        phase: "fixture",
        bytes: rss(&mut system),
    });
    let fixture_ms = ms(fixture_started);

    let mut source_root = SourceRoot::open_directory(&target)?;
    let directory = source_root.directory().identity.clone();
    let root = SourceRootId::Directory {
        directory: directory.clone(),
    };
    let producer_started = Instant::now();
    let mut actions = Vec::with_capacity(args.files);
    for (index, (name, before)) in before_by_file.iter().enumerate() {
        let source = source_for(&directory, name);
        let mut produced = Vec::with_capacity(args.edits_per_file);
        produced.push(ProducedEdit::new(
            ActionSpan {
                source: source.clone(),
                start: 0,
                end: 8,
            },
            index.to_le_bytes(),
            ActionProducer::unordered(format!("scale-{index}-0")).with_rule("source-mutations"),
        ));
        for edit in 1..args.edits_per_file {
            let start = 8 + (edit - 1) * 2;
            produced.push(ProducedEdit::new(
                ActionSpan {
                    source: source.clone(),
                    start: start as u64,
                    end: start as u64 + 1,
                },
                *b"b",
                ActionProducer::unordered(format!("scale-{index}-{edit}"))
                    .with_rule("source-mutations"),
            ));
        }
        actions.push(SourceAction::Replace {
            source,
            expected: content(before),
            edits: ProducedEditBatch::new(produced).into_text_edits().unwrap(),
        });
    }
    rss_samples.push(RssSample {
        phase: "producer",
        bytes: rss(&mut system),
    });
    let producer_ms = ms(producer_started);
    let request = StageRequest::new(root.clone(), actions);

    let planner_started = Instant::now();
    let plan = plan_mutations(&mut source_root, &request).context("plan source mutations")?;
    rss_samples.push(RssSample {
        phase: "planner",
        bytes: rss(&mut system),
    });
    let planner_ms = ms(planner_started);
    drop(request);
    drop(before_by_file);

    let mut store = DurableStageStore::open(&store_root)?;
    let stage_started = Instant::now();
    let stage = store.save(plan).context("persist source mutation stage")?;
    rss_samples.push(RssSample {
        phase: "stage",
        bytes: rss(&mut system),
    });
    let stage_ms = ms(stage_started);
    let loaded_started = Instant::now();
    let loaded = store
        .load(stage.id)?
        .context("reload durable source mutation stage")?;
    rss_samples.push(RssSample {
        phase: "load",
        bytes: rss(&mut system),
    });
    let load_ms = ms(loaded_started);
    ensure!(
        loaded.files.len() == args.files,
        "stage file count changed on reload"
    );

    let engine = CommitEngine::open(&target, &state_root)?;
    let commit_started = Instant::now();
    let interruption = engine.commit_with_failpoint(&loaded, Some(CommitFailpoint::AfterJournal));
    ensure!(
        matches!(
            interruption,
            Err(soopy::CommitRefusal::Failpoint {
                point: CommitFailpoint::AfterJournal
            })
        ),
        "commit did not leave a replayable journal"
    );
    let commit_ms = ms(commit_started);
    let replay_started = Instant::now();
    let receipt = engine
        .recover(stage.id)
        .context("recover interrupted commit")?;
    rss_samples.push(RssSample {
        phase: "replay",
        bytes: rss(&mut system),
    });
    let replay_ms = ms(replay_started);
    let replayed = engine.commit(&loaded).context("replay completed commit")?;
    ensure!(replayed == receipt, "completed commit was not idempotent");
    let updated = std::fs::read(target.join("file-00000.txt"))?;
    ensure!(
        updated != vec![b'a'; args.bytes_per_file],
        "commit did not update the target"
    );
    let staged_ids: BTreeSet<_> = loaded
        .files
        .iter()
        .filter_map(|file| file.staged_bytes)
        .collect();
    let unique_blobs = staged_ids.len();
    let unique_blob_bytes = staged_ids
        .iter()
        .filter_map(|id| {
            loaded
                .files
                .iter()
                .find(|file| file.staged_bytes == Some(*id))
        })
        .filter_map(|file| file.bytes_after.as_ref())
        .map(Vec::len)
        .sum();
    let manifest_path = store_root
        .join("manifests")
        .join(format!("{}.json", stage.id));
    let manifest_bytes = std::fs::metadata(manifest_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let rss_values: Vec<_> = rss_samples
        .iter()
        .filter_map(|sample| sample.bytes)
        .collect();
    let peak_rss_bytes = rss_values.into_iter().max();
    let output = Receipt {
        schema_version: 1,
        scenario: if args.files.saturating_mul(args.edits_per_file) == 100_000 {
            "source-mutations-directory-100k-edit-gate"
        } else {
            "source-mutations-directory-scale"
        },
        root_kind: "directory",
        repository_count: 0,
        root_count: 1,
        files: args.files,
        input_edits: args.files.saturating_mul(args.edits_per_file),
        source_bytes: args.files.saturating_mul(args.bytes_per_file),
        result_bytes: args.files.saturating_mul(args.bytes_per_file),
        phases: PhaseTimes {
            fixture_ms,
            producer_ms,
            planner_ms,
            stage_ms,
            load_ms,
            commit_ms,
            replay_ms,
            total_ms: ms(total_started),
        },
        rss_samples,
        peak_rss_bytes,
        allocation_bytes: None,
        stage: StageMetrics {
            id: stage.id.to_string(),
            unique_blobs,
            unique_blob_bytes,
            dedup_hits: args.files.saturating_sub(unique_blobs),
            manifest_bytes,
            previews: loaded.previews.len(),
        },
        commit: CommitMetrics {
            applied_files: receipt.applied_files,
            journal_bytes: receipt.journal_bytes,
            recovered_stage_id: receipt.stage_id.to_string(),
            idempotent_replay: true,
        },
    };
    if let Some(path) = args.receipt.as_deref() {
        write_receipt(path, &output)?;
    }
    println!("{}", serde_json::to_string_pretty(&output)?);
    let _ = std::fs::remove_dir_all(&target);
    if store_is_ephemeral {
        let _ = std::fs::remove_dir_all(&store_root);
    }
    let _ = std::fs::remove_dir_all(&state_root);
    Ok(())
}
