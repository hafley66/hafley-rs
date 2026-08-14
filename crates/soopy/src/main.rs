use std::io::Write;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
use soopy::{Pattern, ReadRequest, Revision, SourceDelta, SourceEntry, SourceQuery, SourceTree};

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
    let repository = soopy::discover(&cli.repo)?;
    let mut tree = SourceTree::open(repository);
    match cli.command {
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
