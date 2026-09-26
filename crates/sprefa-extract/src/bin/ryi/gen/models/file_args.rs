use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

use crate::models::inputs::Inputs;

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct FileArgs {
  #[command(flatten)]
  pub inputs: Inputs,
  #[arg(skip)]
  pub paths: Vec<PathBuf>,
  #[doc = "Fact kinds per file (cst,type,call,df,data,cfg)"]
  #[arg(long, value_name = "KINDS", value_delimiter = ',')]
  pub kinds: Option<Vec<String>>,
  #[doc = "Resolve arms under --resolve (call,type,flow)"]
  #[arg(long, value_name = "ARMS", requires = "resolve", value_delimiter = ',')]
  pub arms: Option<Vec<String>>,
  #[doc = "Write to a new SQLite database instead of stdout"]
  #[arg(long, value_name = "PATH", conflicts_with = "bench")]
  pub sqlite: Option<PathBuf>,
  #[doc = "Time extraction and print per-family counts to stderr"]
  #[arg(long)]
  pub bench: bool,
  #[doc = "Resolve cross-file edges over all given paths"]
  #[arg(long, conflicts_with = "bench")]
  pub resolve: bool,
  #[doc = "Load this index.scip"]
  #[arg(long, value_name = "FILE", conflicts_with = "scip_build")]
  pub scip_index: Option<PathBuf>,
  #[doc = "Use rust-analyzer for Rust call/type targets"]
  #[arg(long, requires = "root")]
  pub rust_checker: bool,
  #[doc = "Use the TypeScript compiler for TS call/type targets"]
  #[arg(long, requires = "root")]
  pub ts_checker: bool,
  #[doc = "Use go/types for Go call/type targets"]
  #[arg(long, requires = "root")]
  pub go_checker: bool,
  #[doc = "Build the index with the language's indexer, then load it"]
  #[arg(long, requires = "root")]
  pub scip_build: bool,
  #[doc = "Seconds allowed for one indexer run under --scip-build"]
  #[arg(long, value_name = "SECS")]
  pub scip_timeout: Option<u64>,
  #[doc = "File-to-file edges folded from a SCIP index"]
  #[arg(long, requires = "root", conflicts_with_all = ["bench", "resolve", "file_fact"])]
  pub scip_deps: bool,
  #[doc = "File-to-file edges resolved from syntax, no index"]
  #[arg(long, requires = "root", conflicts_with_all = ["bench", "resolve", "scip_deps", "file_fact"])]
  pub deps: bool,
  #[doc = "Manifest-to-manifest workspace edges"]
  #[arg(long, requires = "root", conflicts_with_all = ["bench", "resolve", "scip_deps", "deps", "file_fact"])]
  pub package_deps: bool,
  #[doc = "Prepend one file record: path, digest, bytes, lines"]
  #[arg(long, conflicts_with = "resolve")]
  pub file_fact: bool,
  #[doc = "Add 1-based line/col beside every span"]
  #[arg(long)]
  pub lines: bool,
  #[doc = "Wrap output in the TSI envelope"]
  #[arg(long, conflicts_with_all = ["bench", "deps", "package_deps", "scip_deps", "file_fact"])]
  pub witness: bool,
  #[doc = "Skip inputs over this many bytes (0 = no limit)"]
  #[arg(long, value_name = "BYTES")]
  pub max_bytes: Option<u64>,
}
