use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditPlan {
  pub files: Vec<PathBuf>,
  pub edits: u32,
  pub committed: bool,
}
