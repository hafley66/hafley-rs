use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args, Default)]
pub struct Inputs {
  #[doc = "Files, directories, or globs; - reads a path list from stdin"]
  #[arg(value_name = "PATH")]
  #[serde(default)]
  pub paths: Vec<String>,
  #[doc = "Keep files matching GLOB under each directory input; repeatable"]
  #[arg(long = "pattern", value_name = "GLOB")]
  pub patterns: Vec<String>,
  #[doc = "Run on the files FILE reaches over imports (universe: the PATH inputs, else FILE's project)"]
  #[arg(long, value_name = "FILE")]
  pub entry: Vec<PathBuf>,
  #[doc = "Import hops from --entry"]
  #[arg(long, value_name = "N", requires = "entry")]
  pub depth: Option<u32>,
  #[doc = "Corpus root"]
  #[arg(long, value_name = "DIR")]
  pub root: Option<PathBuf>,
}
