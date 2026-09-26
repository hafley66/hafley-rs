use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactSummary {
  pub rows: u64,
  pub tables: u32,
}
