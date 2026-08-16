use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use soopy::{run_multi_repo_refresh, MultiRepoRefreshConfig};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value_t = 32)]
    repositories: usize,
    #[arg(long, default_value_t = 3)]
    rounds: usize,
    #[arg(long, default_value_t = 4)]
    concurrency: usize,
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long)]
    keep: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let receipt = run_multi_repo_refresh(&MultiRepoRefreshConfig {
        repositories: args.repositories,
        rounds: args.rounds,
        concurrency: args.concurrency,
        root: args.root,
        keep: args.keep,
    })?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
