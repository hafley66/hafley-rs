use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use serde::Serialize;
use soopy::{
    ActionProducer, ContentId, DurableStageStore, MutationPlan, NormalizedEdit, PlannedFile,
    PlannedFileKind, RootPath, SourcePath, SourceRootId, StageStore,
};
use sysinfo::{Pid, ProcessesToUpdate, System};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 1_000)]
    files: usize,
    #[arg(long, default_value_t = 100)]
    edits_per_file: usize,
    #[arg(long, default_value_t = 4_096)]
    bytes_per_file: usize,
    #[arg(long)]
    store: Option<PathBuf>,
}

#[derive(Serialize)]
struct Receipt {
    schema_version: u32,
    scenario: &'static str,
    stage_id: String,
    files: usize,
    input_edits: usize,
    source_bytes: usize,
    result_bytes: usize,
    unique_blobs: usize,
    unique_blob_bytes: usize,
    dedup_hits: usize,
    retained_stage_bytes: usize,
    fixture_ms: u128,
    seal_ms: u128,
    preview_ms: u128,
    persist_ms: u128,
    load_ms: u128,
    rss_before: Option<u64>,
    rss_after: Option<u64>,
    rss_samples: Vec<Option<u64>>,
    os_peak_rss_bytes: Option<u64>,
    os_peak_rss_kib: Option<u64>,
}

fn rss(system: &mut System) -> Option<u64> {
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|process| process.memory())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let store_root = args.store.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("soopy-stage-scale-{}", std::process::id()))
    });
    let fixture_started = Instant::now();
    let mut system = System::new();
    let rss_before = rss(&mut system);
    let root = SourceRootId::Directory {
        directory: soopy::DirectoryId(Arc::from("one-fixture-root")),
    };
    anyhow::ensure!(
        args.bytes_per_file >= args.edits_per_file.max(1) * 2,
        "--bytes-per-file must leave edit positions"
    );
    let before = b"a".repeat(args.bytes_per_file);
    let after = b"b".repeat(before.len());
    let files = (0..args.files)
        .map(|index| PlannedFile {
            kind: PlannedFileKind::Replace,
            source: None,
            path_before: Some(SourcePath::Directory {
                path: RootPath(Arc::from(format!("file-{index:05}.txt"))),
            }),
            path_after: Some(SourcePath::Directory {
                path: RootPath(Arc::from(format!("file-{index:05}.txt"))),
            }),
            content_before: Some(ContentId::Blake3(*blake3::hash(&before).as_bytes())),
            content_after: Some(ContentId::Blake3(*blake3::hash(&after).as_bytes())),
            mode_before: None,
            bytes_before: Some(before.clone()),
            bytes_after: Some(after.clone()),
            edits: (0..args.edits_per_file)
                .map(|edit| NormalizedEdit {
                    start: (edit * 2) as u64,
                    end: (edit * 2 + 1) as u64,
                    replacement: vec![b'b'],
                    producers: vec![ActionProducer::unordered(format!("scale-{index}-{edit}"))],
                })
                .collect(),
        })
        .collect();
    let fixture_ms = fixture_started.elapsed().as_millis();
    let plan = MutationPlan { root, files };
    let mut store = DurableStageStore::open(&store_root).context("open durable stage store")?;
    let seal_started = Instant::now();
    let stage = store.save(plan).context("save durable stage")?;
    let save_ms = seal_started.elapsed().as_millis();
    let preview_started = Instant::now();
    let preview_count = stage.previews.len();
    std::hint::black_box(preview_count);
    let preview_ms = preview_started.elapsed().as_millis();
    let persist_ms = save_ms;
    let load_started = Instant::now();
    let loaded = store.load(stage.id)?.context("load saved stage")?;
    let load_ms = load_started.elapsed().as_millis();
    std::hint::black_box(&loaded);
    let rss_after = rss(&mut system);
    let unique_blobs = store.blob_count()?;
    let source_bytes = args.files.saturating_mul(args.bytes_per_file);
    let unique_blob_bytes = store.blob_bytes()?;
    let retained_stage_bytes = stage
        .files
        .iter()
        .filter_map(|file| file.bytes_after.as_ref())
        .map(Vec::len)
        .sum();
    let os_peak_rss_bytes = os_peak_rss_bytes();
    let os_peak_rss_kib = os_peak_rss_bytes.map(|bytes| bytes / 1024);
    println!(
        "{}",
        serde_json::to_string(&Receipt {
            schema_version: 1,
            scenario: if args.files.saturating_mul(args.edits_per_file) == 100_000 {
                "durable-stage-100k-edits-one-root"
            } else {
                "durable-stage-one-root"
            },
            stage_id: stage.id.to_string(),
            files: args.files,
            input_edits: args.files.saturating_mul(args.edits_per_file),
            source_bytes,
            result_bytes: source_bytes,
            unique_blobs,
            unique_blob_bytes,
            dedup_hits: args.files.saturating_sub(unique_blobs),
            retained_stage_bytes,
            fixture_ms,
            seal_ms: save_ms,
            preview_ms,
            persist_ms,
            load_ms,
            rss_before,
            rss_after,
            rss_samples: vec![rss_before, rss_after],
            os_peak_rss_bytes,
            os_peak_rss_kib,
        })?
    );
    std::hint::black_box(stage.id);
    Ok(())
}

fn os_peak_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
        let kib = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
        return kib.checked_mul(1024);
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
