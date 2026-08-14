use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use serde::Serialize;
use soopy::{GitFilesQuery, ReadRequest, Revision, SourceTree};
use sysinfo::{Pid, ProcessesToUpdate, System};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    repo: PathBuf,
    #[arg(long)]
    revision: String,
    #[arg(long, default_value_t = 500)]
    handles: usize,
    #[arg(long, default_value_t = 16)]
    batch: usize,
    #[arg(long = "pathspec")]
    pathspecs: Vec<String>,
}

#[derive(Serialize)]
struct PassReceipt {
    elapsed_ms: u128,
    files: usize,
    bytes: u64,
    batches: usize,
}

#[derive(Serialize)]
struct ScaleReceipt {
    repository: PathBuf,
    revision: String,
    pathspecs: Vec<String>,
    retained_handles: usize,
    handles_elapsed_ms: u128,
    rss_bytes_before: Option<u64>,
    rss_bytes_with_handles: Option<u64>,
    file_descriptors_before: Option<usize>,
    file_descriptors_with_handles: Option<usize>,
    enumeration_elapsed_ms: u128,
    enumerated_files: usize,
    enumerated_bytes: u64,
    rss_bytes_after_enumeration: Option<u64>,
    batch_size: usize,
    cold_read: PassReceipt,
    rss_bytes_after_cold_read: Option<u64>,
    warm_read: PassReceipt,
    rss_bytes_after_warm_read: Option<u64>,
}

fn descriptor_count() -> Option<usize> {
    std::fs::read_dir("/dev/fd")
        .ok()
        .map(|entries| entries.count())
}

fn rss_bytes(system: &mut System) -> Option<u64> {
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|process| process.memory())
}

fn read_pass(tree: &mut SourceTree, requests: &[ReadRequest], batch: usize) -> Result<PassReceipt> {
    let started = Instant::now();
    let mut bytes = 0_u64;
    let mut files = 0_usize;
    let mut batches = 0_usize;
    for chunk in requests.chunks(batch) {
        let answers = tree.read_many(chunk)?;
        batches += 1;
        files += answers.len();
        for answer in answers {
            bytes = bytes
                .checked_add(u64::try_from(answer.bytes.len()).context("byte count exceeds u64")?)
                .context("total byte count exceeds u64")?;
        }
    }
    Ok(PassReceipt {
        elapsed_ms: started.elapsed().as_millis(),
        files,
        bytes,
        batches,
    })
}

fn main() -> Result<()> {
    let args = Args::parse();
    anyhow::ensure!(args.batch > 0, "--batch must be greater than zero");
    let repository = soopy::discover(&args.repo)?;
    let mut system = System::new();
    let rss_bytes_before = rss_bytes(&mut system);
    let file_descriptors_before = descriptor_count();

    let handles_started = Instant::now();
    let handles: Vec<_> = (0..args.handles)
        .map(|_| SourceTree::open(repository.clone()))
        .collect();
    let handles_elapsed_ms = handles_started.elapsed().as_millis();
    let rss_bytes_with_handles = rss_bytes(&mut system);
    let file_descriptors_with_handles = descriptor_count();

    let mut tree = SourceTree::open(repository.clone());
    let enumeration_started = Instant::now();
    let entries = tree.git_files(&GitFilesQuery {
        revision: Revision::Named(args.revision.clone().into()),
        pathspecs: args.pathspecs.clone(),
    })?;
    let enumeration_elapsed_ms = enumeration_started.elapsed().as_millis();
    let rss_bytes_after_enumeration = rss_bytes(&mut system);
    let enumerated_bytes = entries.iter().try_fold(0_u64, |total, entry| {
        total
            .checked_add(entry.size)
            .context("enumerated byte count exceeds u64")
    })?;
    let requests: Vec<_> = entries
        .iter()
        .map(|entry| ReadRequest {
            source: entry.source.clone(),
            expected: Some(entry.content.clone()),
        })
        .collect();

    let cold_read = read_pass(&mut tree, &requests, args.batch)?;
    let rss_bytes_after_cold_read = rss_bytes(&mut system);
    let warm_read = read_pass(&mut tree, &requests, args.batch)?;
    let rss_bytes_after_warm_read = rss_bytes(&mut system);
    std::hint::black_box(&handles);
    std::thread::sleep(Duration::from_millis(100));

    println!(
        "{}",
        serde_json::to_string_pretty(&ScaleReceipt {
            repository: repository.root,
            revision: args.revision,
            pathspecs: args.pathspecs,
            retained_handles: handles.len(),
            handles_elapsed_ms,
            rss_bytes_before,
            rss_bytes_with_handles,
            file_descriptors_before,
            file_descriptors_with_handles,
            enumeration_elapsed_ms,
            enumerated_files: entries.len(),
            enumerated_bytes,
            rss_bytes_after_enumeration,
            batch_size: args.batch,
            cold_read,
            rss_bytes_after_cold_read,
            warm_read,
            rss_bytes_after_warm_read,
        })?
    );
    Ok(())
}
