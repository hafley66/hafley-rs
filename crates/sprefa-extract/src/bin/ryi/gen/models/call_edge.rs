use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEdge {
  pub caller_path: PathBuf,
  pub caller_site_start: u32,
  pub callee_path: PathBuf,
  pub callee_start: u32,
  pub callee_end: u32,
  pub resolution_origin: String,
}
