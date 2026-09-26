use std::io::BufRead;
use std::io::Write;

use crate::models::file_args::FileArgs;
use crate::ops_auto::CleaveArgs;
use crate::ops_auto::DiffArgs;
use crate::ops_auto::FastArgs;
use crate::ops_auto::GraphArgs;
use crate::ops_auto::IngestArgs;
use crate::ops_auto::MoveArgs;
use crate::ops_auto::OpResult;
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
  pub file: FileArgs,
}

#[derive(clap::Subcommand, Debug)]
pub enum Cmd {
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

pub fn run(cli: Ryi, input: &mut dyn BufRead, out: &mut dyn Write) -> OpResult<()> {
  match cli.cmd {
          Some(Cmd::Fast(args)) => { for item in crate::ops::fast(&args) { write_json(out, &item?)?; } }
          Some(Cmd::Slow(args)) => { write_json(out, &crate::ops::slow(&args)?)?; }
          Some(Cmd::Scip(args)) => { write_json(out, &crate::ops::scip(&args)?)?; }
          Some(Cmd::Graph(args)) => { write_json(out, &crate::ops::graph(&args)?)?; }
          Some(Cmd::Cleave(args)) => { write_json(out, &crate::ops::cleave(&args)?)?; }
          Some(Cmd::Move(args)) => { write_json(out, &crate::ops::r#move(&args)?)?; }
          Some(Cmd::Rename(args)) => { write_json(out, &crate::ops::rename(&args)?)?; }
          Some(Cmd::Query(args)) => { write_json(out, &crate::ops::query(&args)?)?; }
          Some(Cmd::Region(args)) => { write_json(out, &crate::ops::region(&args)?)?; }
          Some(Cmd::Watch(args)) => { write_json(out, &crate::ops::watch(&args)?)?; }
          Some(Cmd::Diff(args)) => { write_json(out, &crate::ops::diff(&args)?)?; }
          Some(Cmd::Ingest(args)) => { write_json(out, &crate::ops::ingest(&args, read_jsonl(&mut *input))?)?; }
          Some(Cmd::Schema) => { let args = Default::default(); write_json(out, &crate::ops::schema(&args)?)?; }
          Some(Cmd::Trail(args)) => { write_json(out, &crate::ops::trail(&args)?)?; }
          None => { let _ = cli.file; }
  }
  Ok(())
}

pub fn main(cli: Ryi) -> std::process::ExitCode {
  let stdin = std::io::stdin();
  let stdout = std::io::stdout();
  match run(cli, &mut stdin.lock(), &mut stdout.lock()) {
      Ok(()) => std::process::ExitCode::SUCCESS,
      Err(e) => {
          eprintln!("error: {e}");
          std::process::ExitCode::FAILURE
      }
  }
}

fn write_json<T: serde::Serialize>(out: &mut dyn Write, value: &T) -> OpResult<()> {
    serde_json::to_writer(&mut *out, value)?;
    out.write_all(b"\n")?;
    Ok(())
}

fn read_jsonl<'a, T: serde::de::DeserializeOwned>(input: &'a mut dyn BufRead) -> impl Iterator<Item = OpResult<T>> + 'a {
    input.lines().map(|line| Ok(serde_json::from_str(&line?)?))
}
