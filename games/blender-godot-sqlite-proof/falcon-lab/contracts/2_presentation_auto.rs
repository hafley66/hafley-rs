// Generated from 0_presentation.tsp; sha256:1b173dcef9dc2f8f39ba9f8f57ffc74a2bcfef0390fd4c2937247cb551ddd60c
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Row {
  pub tick: i64,
  pub kind: i64,
  pub entity: i64,
  pub values: [f64; 24],
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GenerationId {
  pub epoch: u64,
  pub generation: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameRead {
  pub id: GenerationId,
  pub rows_written: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Acknowledgment {
  pub id: GenerationId,
  pub row_digest: u64,
  pub mesh_vertices: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BoundaryError {
  Capacity,
  SlotsPinned,
  MissingFrame,
  StaleGeneration,
  InvalidPayload,
}
pub type PublishResult = Result<GenerationId, BoundaryError>;
pub type ReadResult = Result<FrameRead, BoundaryError>;
pub type AckResult = Result<GenerationId, BoundaryError>;
pub trait RowPublisher {
  fn publish(&mut self, rows: &[Row]) -> PublishResult;
}
pub trait FrameQuery {
  fn read_frame(&mut self, tick: i64, output: &mut [Row]) -> ReadResult;
}
pub trait FrameAcknowledger {
  fn acknowledge(&mut self, receipt: Acknowledgment) -> AckResult;
}

