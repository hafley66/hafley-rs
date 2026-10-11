use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args, Default)]
pub struct Edit {
  #[doc = "Soopy state root, outside the corpus"]
  #[arg(long, value_name = "DIR")]
  pub state: Option<PathBuf>,
  #[doc = "Write the planned edits (default: dry run)"]
  #[arg(long)]
  #[serde(default)]
  pub apply: bool,
  #[doc = "Report leftover old spellings in plain text"]
  #[arg(long)]
  #[serde(default)]
  pub text_refs: bool,
  #[doc = "Plan over git-tracked files only (default: every file .gitignore keeps)"]
  #[arg(long)]
  #[serde(default)]
  pub tracked: bool,
}
