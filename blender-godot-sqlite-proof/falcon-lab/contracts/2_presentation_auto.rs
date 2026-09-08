// Generated from 0_presentation.tsp; sha256:e22f632bf4c97d3d819c510115bde62529b3863410700004705fe597a6dfac0e
use serde::Deserialize;
use serde::Serialize;

pub const WINDOW: i64 = 32;
pub const SLOTS: u32 = 3;
pub const ROW_CAPACITY: u32 = 1024;
pub const IPC_LIMIT: u32 = 262144;
pub const PROTOCOL_VERSION: u32 = 1;
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Latest {
  pub version: u32,
  pub pid: u32,
  pub generation: u64,
  pub elapsed_us: u64,
  pub rows: Vec<Row>,
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

