use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

use crate::models::type_edge_kind::TypeEdgeKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeEdge {
  pub owner_path: PathBuf,
  pub owner_name: Option<String>,
  pub owner_start: u32,
  pub owner_end: u32,
  pub target_path: PathBuf,
  pub target_name: Option<String>,
  pub kind: TypeEdgeKind,
  pub resolution_origin: String,
}
