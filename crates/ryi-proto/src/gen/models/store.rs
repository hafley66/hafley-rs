use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args, Default)]
pub struct Store {
  #[doc = "Write the fact store to a new SQLite database at PATH"]
  #[arg(long, value_name = "PATH")]
  pub sqlite: Option<PathBuf>,
}
