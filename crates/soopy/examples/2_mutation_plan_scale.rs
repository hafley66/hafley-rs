use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{ensure, Context, Result};
use clap::Parser;
use serde::Serialize;
use soopy::{
    plan_mutations, ActionProducer, ActionSource, ActionSpan, ContentId, FileRef, RootPath,
    SourceAction, SourceRoot, SourceRootId, StageRequest, TextEdit,
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
}

#[derive(Serialize)]
struct ScaleReceipt {
    schema_version: u32,
    scenario: &'static str,
    phase: &'static str,
    source_bytes: usize,
    fixture_elapsed_ms: u128,
    planner_elapsed_ms: u128,
    input_files: usize,
    input_edits: usize,
    output_files: usize,
    output_edits: usize,
    files: usize,
    edits_per_file: usize,
    bytes_per_file: usize,
    output_bytes: usize,
    rss_bytes_before_fixture: Option<u64>,
    rss_bytes_after_fixture: Option<u64>,
    rss_bytes_after_plan: Option<u64>,
    steady_rss_bytes: Option<u64>,
}

fn rss_bytes(system: &mut System) -> Option<u64> {
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|process| process.memory())
}

fn content(bytes: &[u8]) -> ContentId {
    ContentId::Blake3(*blake3::hash(bytes).as_bytes())
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(args.files > 1, "--files must describe many files");
    ensure!(args.edits_per_file > 0, "--edits-per-file must be positive");
    ensure!(
        args.bytes_per_file >= args.edits_per_file * 2,
        "--bytes-per-file must leave non-overlapping edit positions"
    );
    let total_edits = args
        .files
        .checked_mul(args.edits_per_file)
        .context("edit count overflow")?;
    let fixture_started = Instant::now();
    let temporary = std::env::temp_dir().join(format!(
        "soopy_mutation_scale_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("clock before UNIX epoch")?
            .as_nanos(),
    ));
    std::fs::create_dir_all(&temporary).context("create scale fixture root")?;
    let mut system = System::new();
    let rss_bytes_before_fixture = rss_bytes(&mut system);
    let source_bytes: Vec<u8> = (0..args.bytes_per_file).map(|index| index as u8).collect();
    for index in 0..args.files {
        std::fs::write(
            temporary.join(format!("file_{index:05}.txt")),
            &source_bytes,
        )
        .context("write scale fixture")?;
    }
    let fixture_elapsed_ms = fixture_started.elapsed().as_millis();
    let rss_bytes_after_fixture = rss_bytes(&mut system);

    let mut root = SourceRoot::open_directory(&temporary)?;
    let root_id = SourceRootId::Directory {
        directory: root.directory().identity.clone(),
    };
    let expected = content(&source_bytes);
    let actions = (0..args.files)
        .map(|file_index| {
            let source = ActionSource::Directory {
                file: FileRef {
                    directory: root.directory().identity.clone(),
                    path: RootPath(Arc::from(format!("file_{file_index:05}.txt"))),
                },
            };
            let edits = (0..args.edits_per_file)
                .map(|edit_index| {
                    let position = u64::try_from(edit_index * 2).expect("fixture offset fits u64");
                    TextEdit {
                        range: ActionSpan {
                            source: source.clone(),
                            start: position,
                            end: position + 1,
                        },
                        replacement: vec![b'X'],
                        producer: ActionProducer::unordered(format!(
                            "scale-{file_index}-{edit_index}"
                        )),
                    }
                })
                .collect();
            SourceAction::Replace {
                source,
                expected: expected.clone(),
                edits,
            }
        })
        .collect();
    let request = StageRequest::new(root_id, actions);
    let started = Instant::now();
    let plan = plan_mutations(&mut root, &request)?;
    let planner_elapsed_ms = started.elapsed().as_millis();
    let rss_bytes_after_plan = rss_bytes(&mut system);
    let output_bytes = plan
        .files
        .iter()
        .filter_map(|file| file.bytes_after.as_ref())
        .map(Vec::len)
        .sum();
    let output_edits = plan.files.iter().map(|file| file.edits.len()).sum();
    std::hint::black_box(&plan);
    std::thread::sleep(Duration::from_millis(100));
    let steady_rss_bytes = rss_bytes(&mut system);
    let receipt = ScaleReceipt {
        schema_version: 1,
        scenario: "synthetic_nonoverlapping_byte_replacements",
        phase: "source_mutations_planner",
        source_bytes: args.files * args.bytes_per_file,
        fixture_elapsed_ms,
        planner_elapsed_ms,
        input_files: args.files,
        input_edits: total_edits,
        output_files: plan.files.len(),
        output_edits,
        files: args.files,
        edits_per_file: args.edits_per_file,
        bytes_per_file: args.bytes_per_file,
        output_bytes,
        rss_bytes_before_fixture,
        rss_bytes_after_fixture,
        rss_bytes_after_plan,
        steady_rss_bytes,
    };
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    std::fs::remove_dir_all(temporary).context("remove scale fixture root")?;
    Ok(())
}
