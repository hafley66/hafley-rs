// Generated from 0_presentation.tsp; sha256:644d771e64f1650f62add21f95c9815731b03675ea7cfe5e517dce3ee2eef4a7
use serde::Deserialize;
use serde::Serialize;

pub const WINDOW: i64 = 32;
pub const SLOTS: u32 = 3;
pub const ROW_CAPACITY: u32 = 1024;
pub const IPC_LIMIT: u32 = 262144;
pub const PROTOCOL_VERSION: u32 = 1;
pub const REPEAT_FLAG: &'static str = "--repeat";
pub const REPEAT_TICKS: u32 = 300;
pub const REPEAT_PERIOD: i64 = 120;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepeatProof {
  pub ticks: u32,
  pub hits: u32,
  pub damage: f64,
  pub replayed: u32,
  pub hit_ticks: Vec<i64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Row {
  pub tick: i64,
  pub kind: i64,
  pub entity: i64,
  pub values: [f64; 24],
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Line {
  pub a: [f32; 3],
  pub b: [f32; 3],
  pub color: [f32; 4],
  pub width: f32,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameValues {
  pub action: f64,
  pub pose: f64,
  pub root_x: f64,
  pub root_y: f64,
  pub root_z: f64,
  pub damage: f64,
  pub hits: f64,
  pub last_hit: f64,
  pub contact: f64,
  pub predicted: f64,
  pub input: f64,
  pub animation_x: f64,
  pub animation_y: f64,
  pub reserved_13: f64,
  pub reserved_14: f64,
  pub restored: f64,
  pub advances: f64,
  pub total_loads: f64,
  pub confirmed: f64,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct TargetValues {
  pub x: f64,
  pub y: f64,
  pub z: f64,
  pub vx: f64,
  pub vy: f64,
  pub vz: f64,
  pub stun: f64,
  pub phase: f64,
  pub grounded: f64,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct HurtValues {
  pub matrix: [f64; 16],
  pub offset_x: f64,
  pub offset_y: f64,
  pub offset_z: f64,
  pub stretch_x: f64,
  pub stretch_y: f64,
  pub stretch_z: f64,
  pub radius: f64,
  pub enabled: f64,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct AttackValues {
  pub x: f64,
  pub y: f64,
  pub z: f64,
  pub radius: f64,
  pub damage: f64,
  pub enabled: f64,
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
impl FrameValues {
    pub const KIND: i64 = 0;
    pub fn from_row(row: &Row) -> Option<Self> {
        if row.kind != Self::KIND { return None; }
        Some(Self {
            action: row.values[0],
            pose: row.values[1],
            root_x: row.values[2],
            root_y: row.values[3],
            root_z: row.values[4],
            damage: row.values[5],
            hits: row.values[6],
            last_hit: row.values[7],
            contact: row.values[8],
            predicted: row.values[9],
            input: row.values[10],
            animation_x: row.values[11],
            animation_y: row.values[12],
            reserved_13: row.values[13],
            reserved_14: row.values[14],
            restored: row.values[15],
            advances: row.values[16],
            total_loads: row.values[17],
            confirmed: row.values[18],
        })
    }
    pub fn write_row(&self, row: &mut Row) {
        assert_eq!(row.kind, Self::KIND);
        row.values[0] = self.action;
        row.values[1] = self.pose;
        row.values[2] = self.root_x;
        row.values[3] = self.root_y;
        row.values[4] = self.root_z;
        row.values[5] = self.damage;
        row.values[6] = self.hits;
        row.values[7] = self.last_hit;
        row.values[8] = self.contact;
        row.values[9] = self.predicted;
        row.values[10] = self.input;
        row.values[11] = self.animation_x;
        row.values[12] = self.animation_y;
        row.values[13] = self.reserved_13;
        row.values[14] = self.reserved_14;
        row.values[15] = self.restored;
        row.values[16] = self.advances;
        row.values[17] = self.total_loads;
        row.values[18] = self.confirmed;
    }
    pub fn into_row(self, tick: i64, entity: i64) -> Row {
        let mut row = Row { tick, kind: Self::KIND, entity, values: [0.0; 24] };
        self.write_row(&mut row);
        row
    }
}
impl TargetValues {
    pub const KIND: i64 = 1;
    pub fn from_row(row: &Row) -> Option<Self> {
        if row.kind != Self::KIND { return None; }
        Some(Self {
            x: row.values[0],
            y: row.values[1],
            z: row.values[2],
            vx: row.values[3],
            vy: row.values[4],
            vz: row.values[5],
            stun: row.values[6],
            phase: row.values[7],
            grounded: row.values[8],
        })
    }
    pub fn write_row(&self, row: &mut Row) {
        assert_eq!(row.kind, Self::KIND);
        row.values[0] = self.x;
        row.values[1] = self.y;
        row.values[2] = self.z;
        row.values[3] = self.vx;
        row.values[4] = self.vy;
        row.values[5] = self.vz;
        row.values[6] = self.stun;
        row.values[7] = self.phase;
        row.values[8] = self.grounded;
    }
    pub fn into_row(self, tick: i64, entity: i64) -> Row {
        let mut row = Row { tick, kind: Self::KIND, entity, values: [0.0; 24] };
        self.write_row(&mut row);
        row
    }
}
impl HurtValues {
    pub const KIND: i64 = 2;
    pub fn from_row(row: &Row) -> Option<Self> {
        if row.kind != Self::KIND { return None; }
        Some(Self {
            matrix: std::array::from_fn(|i| row.values[0 + i]),
            offset_x: row.values[16],
            offset_y: row.values[17],
            offset_z: row.values[18],
            stretch_x: row.values[19],
            stretch_y: row.values[20],
            stretch_z: row.values[21],
            radius: row.values[22],
            enabled: row.values[23],
        })
    }
    pub fn write_row(&self, row: &mut Row) {
        assert_eq!(row.kind, Self::KIND);
        row.values[0..16].copy_from_slice(&self.matrix);
        row.values[16] = self.offset_x;
        row.values[17] = self.offset_y;
        row.values[18] = self.offset_z;
        row.values[19] = self.stretch_x;
        row.values[20] = self.stretch_y;
        row.values[21] = self.stretch_z;
        row.values[22] = self.radius;
        row.values[23] = self.enabled;
    }
    pub fn into_row(self, tick: i64, entity: i64) -> Row {
        let mut row = Row { tick, kind: Self::KIND, entity, values: [0.0; 24] };
        self.write_row(&mut row);
        row
    }
}
impl AttackValues {
    pub const KIND: i64 = 3;
    pub fn from_row(row: &Row) -> Option<Self> {
        if row.kind != Self::KIND { return None; }
        Some(Self {
            x: row.values[0],
            y: row.values[1],
            z: row.values[2],
            radius: row.values[3],
            damage: row.values[4],
            enabled: row.values[5],
        })
    }
    pub fn write_row(&self, row: &mut Row) {
        assert_eq!(row.kind, Self::KIND);
        row.values[0] = self.x;
        row.values[1] = self.y;
        row.values[2] = self.z;
        row.values[3] = self.radius;
        row.values[4] = self.damage;
        row.values[5] = self.enabled;
    }
    pub fn into_row(self, tick: i64, entity: i64) -> Row {
        let mut row = Row { tick, kind: Self::KIND, entity, values: [0.0; 24] };
        self.write_row(&mut row);
        row
    }
}
pub fn pack_rows(rows: &[Row]) -> Vec<f64> {
    rows.iter().flat_map(|row| [row.tick as f64, row.kind as f64, row.entity as f64].into_iter().chain(row.values)).collect()
}

