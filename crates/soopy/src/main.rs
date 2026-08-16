use std::io::Write;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
use soopy::{
    DurableStageStore, GitFileQuery, Pattern, ReadRequest, Revision, SourceDelta, SourceEntry,
    SourceQuery, SourceRoot, SourceTree, StageId, StageStore,
};
use sysinfo::{Pid, ProcessesToUpdate, System};

/// Query repository worktrees and immutable Git revisions.
///
/// `files`, `read`, and `watch` use Soopy's Rust source API. `query` is an
/// interactive CLI adapter over ripgrep, and optional `--fzf` selection stays
/// outside the library data model.
#[derive(Parser)]
#[command(name = "soopy", version, about, long_about = None)]
struct Cli {
    /// Repository root or any path inside the repository.
    #[arg(long, global = true, default_value = ".", value_name = "PATH")]
    repo: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Display a sealed stage and its stored result bytes without reading the target root.
    ShowStage {
        id: String,
        #[arg(long, value_name = "PATH")]
        store: PathBuf,
    },
    /// Remove one stage manifest. Content blobs remain until explicit cleanup.
    DiscardStage {
        id: String,
        #[arg(long, value_name = "PATH")]
        store: PathBuf,
    },
    /// Observe one cold and three warm tracked-state snapshots and emit one
    /// JSON record for the Just performance gates.
    StatusMetrics,
    /// Resolve WORK, a branch, tag, or commit to the repository's stable source coordinate.
    Resolve {
        /// Revision name. `WORK` names the mutable worktree.
        revision: String,
    },
    /// Enumerate matching files with content IDs and byte sizes.
    #[command(visible_alias = "enumerate")]
    Files {
        #[command(flatten)]
        selection: Selection,

        /// Output records as tab-separated fields or one JSON value per line.
        #[arg(long, value_enum, default_value_t = ListingFormat::Tsv)]
        format: ListingFormat,
    },
    /// Read matching files. JSONL carries bytes as a JSON byte array; raw preserves bytes.
    Read {
        #[command(flatten)]
        selection: Selection,

        /// `raw` writes a path/size header then file bytes. `jsonl` writes one structured record per file.
        #[arg(long, value_enum, default_value_t = ReadFormat::Raw)]
        format: ReadFormat,
    },
    /// Watch a WORK query, coalesce filesystem events, and emit source-level deltas.
    Watch {
        /// Glob pattern. Repeat to union patterns. The default selects all non-ignored files.
        #[arg(long = "glob", default_value = "**/*", action = clap::ArgAction::Append)]
        patterns: Vec<String>,

        /// Output source deltas as readable text or one JSON value per line.
        #[arg(long, value_enum, default_value_t = WatchFormat::Text)]
        format: WatchFormat,
    },
    /// Run ripgrep over the selected worktree. Ripgrep and fzf remain CLI adapters.
    Query {
        /// Text pattern forwarded to ripgrep.
        pattern: String,

        /// Restrict ripgrep with a glob. Repeat to pass several ripgrep globs.
        #[arg(long = "glob", action = clap::ArgAction::Append)]
        patterns: Vec<String>,

        /// Ask ripgrep for structured JSONL output.
        #[arg(long, value_enum, default_value_t = QueryFormat::Text)]
        format: QueryFormat,

        /// Pipe ripgrep output through fzf. Requires `fzf` on PATH.
        #[arg(long)]
        fzf: bool,
    },
}

#[derive(clap::Args)]
struct Selection {
    /// Revision name. `WORK` reads the filesystem; another value reads that Git commit tree.
    #[arg(long, default_value = "WORK")]
    revision: String,

    /// Glob pattern. Repeat this option to union patterns in one traversal.
    #[arg(long = "glob", default_value = "**/*", action = clap::ArgAction::Append)]
    patterns: Vec<String>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum ListingFormat {
    Tsv,
    Jsonl,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum ReadFormat {
    Raw,
    Jsonl,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum WatchFormat {
    Text,
    Jsonl,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum QueryFormat {
    Text,
    Jsonl,
}

fn revision(value: String) -> Revision {
    if value == "WORK" {
        Revision::Worktree
    } else {
        Revision::Named(Arc::from(value))
    }
}

fn query(selection: Selection) -> SourceQuery {
    SourceQuery {
        revision: revision(selection.revision),
        patterns: selection.patterns.into_iter().map(Pattern).collect(),
    }
}

fn entry_json(entry: &SourceEntry) -> serde_json::Value {
    json!({
        "path": entry.source.path.0.as_ref(),
        "revision": format!("{:?}", entry.source.revision),
        "content": entry.content.to_string(),
        "size": entry.size,
    })
}

fn delta_json(delta: &SourceDelta) -> serde_json::Value {
    match delta {
        SourceDelta::Added(entry) => json!({ "kind": "added", "entry": entry_json(entry) }),
        SourceDelta::Changed { before, after } => {
            json!({ "kind": "changed", "before": entry_json(before), "after": entry_json(after) })
        }
        SourceDelta::Removed(source) => json!({
            "kind": "removed",
            "path": source.path.0.as_ref(),
            "revision": format!("{:?}", source.revision),
        }),
        SourceDelta::RevisionChanged { before, after } => {
            json!({ "kind": "revision_changed", "before": format!("{:?}", before), "after": format!("{:?}", after) })
        }
        SourceDelta::RescanRequired => json!({ "kind": "rescan_required" }),
    }
}

fn run_rg(
    root: &std::path::Path,
    pattern: &str,
    patterns: &[String],
    format: QueryFormat,
    fzf: bool,
) -> Result<()> {
    let mut rg = ProcessCommand::new("rg");
    rg.current_dir(root)
        .arg("--line-number")
        .arg("--no-heading")
        .arg("--color=never");
    if matches!(format, QueryFormat::Jsonl) {
        rg.arg("--json");
    }
    for glob in patterns {
        rg.args(["--glob", glob]);
    }
    rg.arg(pattern);
    if !fzf {
        let status = rg.status().context("run rg")?;
        if status.success() || status.code() == Some(1) {
            return Ok(());
        }
        bail!("rg exited with {status}");
    }
    let output = rg.output().context("run rg for fzf")?;
    if !output.status.success() && output.status.code() != Some(1) {
        bail!("rg exited with {}", output.status);
    }
    let mut fzf_child = ProcessCommand::new("fzf")
        .stdin(Stdio::piped())
        .spawn()
        .context("run fzf")?;
    fzf_child
        .stdin
        .as_mut()
        .context("open fzf stdin")?
        .write_all(&output.stdout)
        .context("write ripgrep records to fzf")?;
    let status = fzf_child.wait().context("wait for fzf")?;
    if status.success() || status.code() == Some(130) {
        Ok(())
    } else {
        bail!("fzf exited with {status}")
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Command::ShowStage { id, store } = &cli.command {
        let store = DurableStageStore::open(store)?;
        let stage = store.load(id.parse::<StageId>().map_err(anyhow::Error::msg)?)?;
        println!("{}", serde_json::to_string_pretty(&stage)?);
        return Ok(());
    }
    if let Command::DiscardStage { id, store } = &cli.command {
        let mut store = DurableStageStore::open(store)?;
        let removed = store.discard(id.parse::<StageId>().map_err(anyhow::Error::msg)?)?;
        println!("{}", serde_json::json!({"discarded": removed}));
        return Ok(());
    }
    let repository = soopy::discover(&cli.repo)?;
    let mut tree = SourceTree::open(repository);
    match cli.command {
        Command::ShowStage { .. } | Command::DiscardStage { .. } => unreachable!(),
        Command::StatusMetrics => {
            let mut source = SourceRoot::discover_git(&cli.repo)?;
            let git = source
                .git_mut()
                .context("status metrics requires Git discovery")?;
            let mut system = System::new();
            let started = Instant::now();
            let cold_started = Instant::now();
            let cold = git.tracked_state_with_metrics(&GitFileQuery::default())?;
            let cold_ms = cold_started.elapsed().as_secs_f64() * 1_000.0;
            let cold_rss = resident_set_bytes(&mut system);
            let mut warm = Vec::with_capacity(3);
            for _ in 0..3 {
                let warm_started = Instant::now();
                let result = git.tracked_state_with_metrics(&GitFileQuery::default())?;
                warm.push(json!({
                    "wall_ms": warm_started.elapsed().as_secs_f64() * 1_000.0,
                    "rss_bytes": resident_set_bytes(&mut system),
                    "metrics": result.metrics,
                }));
            }
            let peak_rss = cold_rss
                .into_iter()
                .chain(
                    warm.iter()
                        .filter_map(|receipt| receipt["rss_bytes"].as_u64()),
                )
                .max();
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "files": cold.observations.len(),
                    "cold": {
                        "wall_ms": cold_ms,
                        "rss_bytes": cold_rss,
                        "metrics": cold.metrics,
                    },
                    "warm": warm,
                    "wall_ms": started.elapsed().as_secs_f64() * 1_000.0,
                    "peak_rss_bytes": peak_rss,
                    "open_file_descriptors": open_file_descriptors(),
                }))?
            );
        }
        Command::Resolve { revision: name } => {
            println!("{:?}", tree.resolve_revision(revision(name))?)
        }
        Command::Files { selection, format } => {
            let snapshot = tree.snapshot(&query(selection))?;
            for entry in snapshot.files {
                match format {
                    ListingFormat::Tsv => {
                        println!("{}\t{}\t{}", entry.source.path.0, entry.content, entry.size)
                    }
                    ListingFormat::Jsonl => {
                        println!("{}", serde_json::to_string(&entry_json(&entry))?)
                    }
                }
            }
        }
        Command::Read { selection, format } => {
            let snapshot = tree.snapshot(&query(selection))?;
            let requests: Vec<ReadRequest> = snapshot
                .files
                .iter()
                .map(|entry| ReadRequest {
                    source: entry.source.clone(),
                    expected: Some(entry.content.clone()),
                })
                .collect();
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            let mut buffer = Vec::new();
            tree.read_each(&requests, &mut buffer, |answer| {
                match format {
                    ReadFormat::Raw => {
                        writeln!(stdout, "{}\t{}", answer.source.path.0, answer.bytes.len())?;
                        stdout.write_all(answer.bytes)?;
                        writeln!(stdout)?;
                    }
                    ReadFormat::Jsonl => {
                        writeln!(
                            stdout,
                            "{}",
                            serde_json::to_string(&json!({
                                "path": answer.source.path.0.as_ref(),
                                "content": answer.content.to_string(),
                                "bytes": answer.bytes,
                            }))?
                        )?;
                    }
                }
                Ok(())
            })?;
        }
        Command::Watch { patterns, format } => {
            let mut watcher = tree.watch(SourceQuery {
                revision: Revision::Worktree,
                patterns: patterns.into_iter().map(Pattern).collect(),
            })?;
            loop {
                for delta in watcher.recv()? {
                    match format {
                        WatchFormat::Text => println!("{:?}", delta),
                        WatchFormat::Jsonl => {
                            println!("{}", serde_json::to_string(&delta_json(&delta))?)
                        }
                    }
                }
            }
        }
        Command::Query {
            pattern,
            patterns,
            format,
            fzf,
        } => {
            run_rg(
                tree.repository().root.as_path(),
                &pattern,
                &patterns,
                format,
                fzf,
            )?;
        }
    }
    Ok(())
}

fn open_file_descriptors() -> Option<usize> {
    std::fs::read_dir("/dev/fd")
        .ok()
        .map(|entries| entries.count())
}

fn resident_set_bytes(system: &mut System) -> Option<u64> {
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(sysinfo::Process::memory)
}
