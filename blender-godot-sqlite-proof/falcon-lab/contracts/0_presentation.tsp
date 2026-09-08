import "./0_rows.mjs";
using TypeSpec.Reflection;
namespace FalconBoundary;

// Field declaration order is the existing numeric row layout. Unused tail is zero.
extern dec row(target: Model, kind: valueof int64);

const WINDOW: int64 = 32;
const SLOTS: uint32 = 3;
const ROW_CAPACITY: uint32 = 1024;
const IPC_LIMIT: uint32 = 262144;
const PROTOCOL_VERSION: uint32 = 1;
const REPEAT_FLAG: string = "--repeat";
const REPEAT_TICKS: uint32 = 300;
const REPEAT_PERIOD: int64 = 120;

model RepeatProof {
  ticks: uint32;
  hits: uint32;
  damage: float64;
  replayed: uint32;
  @maxItems(2)
  hit_ticks: int64[];
}

// Buffer direction determines the generated Rust borrow. No transport is implied.
model ReadBuffer<T> { item: T; }
model WriteBuffer<T> { item: T; }

model Row {
  tick: int64;
  kind: int64;
  entity: int64;
  @minItems(24) @maxItems(24)
  values: float64[];
}

model Line {
  @minItems(3) @maxItems(3)
  a: float32[];
  @minItems(3) @maxItems(3)
  b: float32[];
  @minItems(4) @maxItems(4)
  color: float32[];
  width: float32;
}

@row(0)
model FrameValues {
  action: float64;
  pose: float64;
  root_x: float64;
  root_y: float64;
  root_z: float64;
  damage: float64;
  hits: float64;
  last_hit: float64;
  contact: float64;
  predicted: float64;
  input: float64;
  animation_x: float64;
  animation_y: float64;
  reserved_13: float64;
  reserved_14: float64;
  restored: float64;
  advances: float64;
  total_loads: float64;
  confirmed: float64;
}

@row(1)
model TargetValues {
  x: float64;
  y: float64;
  z: float64;
  vx: float64;
  vy: float64;
  vz: float64;
  stun: float64;
  phase: float64;
  grounded: float64;
}

@row(2)
model HurtValues {
  @minItems(16) @maxItems(16)
  matrix: float64[];
  offset_x: float64;
  offset_y: float64;
  offset_z: float64;
  stretch_x: float64;
  stretch_y: float64;
  stretch_z: float64;
  radius: float64;
  enabled: float64;
}

@row(3)
model AttackValues {
  x: float64;
  y: float64;
  z: float64;
  radius: float64;
  damage: float64;
  enabled: float64;
}

model GenerationId {
  epoch: uint64;
  generation: uint64;
}

// Keep declaration order identical to the existing bincode envelope.
model Latest {
  version: uint32;
  pid: uint32;
  generation: uint64;
  elapsed_us: uint64;
  @maxItems(ROW_CAPACITY)
  rows: Row[];
}

enum BoundaryError {
  Capacity,
  SlotsPinned,
  MissingFrame,
  StaleGeneration,
  InvalidPayload,
}

model FrameRead {
  id: GenerationId;
  rows_written: uint32;
}

model Acknowledgment {
  id: GenerationId;
  row_digest: uint64;
  mesh_vertices: uint32;
}

union PublishResult { Ok: GenerationId, Err: BoundaryError }
union ReadResult { Ok: FrameRead, Err: BoundaryError }
union AckResult { Ok: GenerationId, Err: BoundaryError }

interface RowPublisher {
  publish(rows: ReadBuffer<Row>): PublishResult;
}
interface FrameQuery {
  read_frame(tick: int64, output: WriteBuffer<Row>): ReadResult;
}
interface FrameAcknowledger {
  acknowledge(receipt: Acknowledgment): AckResult;
}
