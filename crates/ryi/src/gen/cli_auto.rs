use crate::models::file_args::FileArgs;
use crate::ops_auto::CleaveArgs;
use crate::ops_auto::DiffArgs;
use crate::ops_auto::ExtractArgs;
use crate::ops_auto::FastArgs;
use crate::ops_auto::GraphArgs;
use crate::ops_auto::IngestArgs;
use crate::ops_auto::MoveArgs;
use crate::ops_auto::QueryArgs;
use crate::ops_auto::RegionArgs;
use crate::ops_auto::RenameArgs;
use crate::ops_auto::ScipArgs;
use crate::ops_auto::SlowArgs;
use crate::ops_auto::TrailArgs;
use crate::ops_auto::WatchArgs;

#[derive(clap::Parser, Debug)]
#[command(name = "ryi", version, about = "Source files -> flat graph facts (JSONL on stdout, or --sqlite)", after_help = concat!("Logging: RUST_LOG (default sprefa_extract=info,hafley_scm=info), HAFLEY_LOG_FORMAT=json|text\nBuild: git hash: ", env!("SPREFA_BUILD_GIT_HASH"), ", datetime: ", env!("SPREFA_BUILD_DATETIME"), ""), args_conflicts_with_subcommands = true, subcommand_negates_reqs = true, disable_help_subcommand = true)]
pub struct Ryi {
  #[command(subcommand)]
  pub cmd: Option<Cmd>,#[command(flatten)]
  pub file: FileArgs,#[doc = "Run without the resident daemon (server mode is already fresh)"]
  #[arg(long, global = true)]
  pub fresh: bool,
}

#[derive(clap::Subcommand, Debug)]
pub enum Cmd {
  #[doc = "Extract file facts with the root flags"]
  Extract(ExtractArgs),
  #[doc = "Syntax-only whole-project facts (no compiler)"]
  Fast(FastArgs),
  #[doc = "The SCIP oracle written as fast's tables"]
  Slow(SlowArgs),
  #[doc = "Raw SCIP index rows"]
  Scip(ScipArgs),
  #[doc = "Ask one question of the resolved call/type graph"]
  Graph(GraphArgs),
  #[doc = "Move one item into another file, with its imports"]
  Cleave(CleaveArgs),
  #[doc = "Move a file and repair every specifier that names it"]
  Move(MoveArgs),
  #[doc = "Rename a symbol and every occurrence bound to it"]
  #[command(after_help = "Exit codes: 2 plan error, 3 ambiguous (pass --at), 4 not found, 5 inexact, 6 dynamic, 7 plan has abstains")]
  Rename(RenameArgs),
  #[doc = "Run a tree-sitter query over files"]
  Query(QueryArgs),
  #[doc = "Replace a generated region between sprefa markers"]
  Region(RegionArgs),
  #[doc = "Stream fact deltas as the worktree changes"]
  Watch(WatchArgs),
  #[doc = "Fact delta between two commits"]
  Diff(DiffArgs),
  #[doc = "Validate and re-emit foreign TSI JSONL"]
  Ingest(IngestArgs),
  #[doc = "Print the output record schema"]
  Schema,
  #[doc = "Print the last N runs from the trail"]
  Trail(TrailArgs),
}
