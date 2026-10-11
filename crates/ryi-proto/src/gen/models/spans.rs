use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args, Default)]
pub struct Spans {
  #[doc = "Add 1-based line/col beside every span"]
  #[arg(long)]
  #[serde(default)]
  pub lines: bool,
}
