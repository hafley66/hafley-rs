use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use serde::Serialize;
use soopy::{
    ActionProducer, ActionSource, ActionSpan, DirectoryId, FileRef, ProducedEdit, RootPath,
    Utf8TextEdit, PRODUCED_EDIT_SCHEMA_VERSION,
};
use sysinfo::{Pid, ProcessesToUpdate, System};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 100_000)]
    edits: usize,
}
#[derive(Serialize)]
struct ScaleReceipt {
    schema_version: u32,
    input_edits: usize,
    unique_edits: usize,
    provenance_records: usize,
    conversion_ms: u128,
    dedup_ms: u128,
    wall_ms: u128,
    rss_bytes_before: Option<u64>,
    rss_bytes_after_conversion: Option<u64>,
    rss_bytes_after_dedup: Option<u64>,
}

fn rss_bytes(system: &mut System) -> Option<u64> {
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|process| process.memory())
}

fn main() {
    let args = Args::parse();
    let mut system = System::new();
    let rss_bytes_before = rss_bytes(&mut system);
    let source = ActionSource::Directory {
        file: FileRef {
            directory: DirectoryId(Arc::from("scale")),
            path: RootPath(Arc::from("src/generated.rs")),
        },
    };
    let started = Instant::now();
    let conversion_started = Instant::now();
    let produced: Vec<ProducedEdit> = (0..args.edits)
        .map(|index| {
            ProducedEdit::from_utf8_text_edit(Utf8TextEdit {
                range: ActionSpan {
                    source: source.clone(),
                    start: (index % 512) as u64,
                    end: (index % 512) as u64,
                },
                replacement: "value".to_owned(),
                producer: ActionProducer::unordered("rust-analyzer").with_rule("scale-fixture"),
            })
        })
        .collect();
    let conversion_ms = conversion_started.elapsed().as_millis();
    let rss_bytes_after_conversion = rss_bytes(&mut system);
    let dedup_started = Instant::now();
    let deduplicated = soopy::deduplicate_equivalent_edits(produced);
    let dedup_ms = dedup_started.elapsed().as_millis();
    let rss_bytes_after_dedup = rss_bytes(&mut system);
    let unique_edits = deduplicated.len();
    let provenance_records = deduplicated.iter().map(|edit| edit.producers.len()).sum();
    std::hint::black_box(deduplicated);
    println!(
        "{}",
        serde_json::to_string_pretty(&ScaleReceipt {
            schema_version: PRODUCED_EDIT_SCHEMA_VERSION,
            input_edits: args.edits,
            unique_edits,
            provenance_records,
            conversion_ms,
            dedup_ms,
            wall_ms: started.elapsed().as_millis(),
            rss_bytes_before,
            rss_bytes_after_conversion,
            rss_bytes_after_dedup,
        })
        .expect("scale receipt serializes")
    );
}
