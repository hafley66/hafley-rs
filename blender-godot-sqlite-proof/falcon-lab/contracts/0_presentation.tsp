namespace FalconBoundary;

const WINDOW: int64 = 32;
const SLOTS: uint32 = 3;
const ROW_CAPACITY: uint32 = 1024;
const IPC_LIMIT: uint32 = 262144;
const PROTOCOL_VERSION: uint32 = 1;

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
