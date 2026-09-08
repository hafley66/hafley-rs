# Presentation boundary contracts

`0_presentation.tsp` owns the row layout, generation/epoch envelope, error set,
publish/read/ack signatures, native boundary constants, and the `Latest` IPC
envelope. `just generate` compiles the actual TypeSpec
program and renders Rust through `@hafley66/alloy-rs` components. It also emits
a transport-neutral YAML description, not an OpenAPI HTTP document.

`@row(kind)` models name the existing numeric payloads. Field declaration order
is the packed layout, including explicit reserved fields. The generator rejects
duplicate kinds, unsupported packed field types, and layouts wider than Row.
It emits Rust read/write/packing adapters and `godot/1_rows_auto.gd` readers from
the same layout. Existing wire order, numeric representations, and padding are
preserved. This is a versioned packing convention, not Rust shared-memory ABI.

```sh
just contracts-setup
just generate
just check-generated
just test
just verify
just test-godot
```

Run from the Falcon lab. `check-generated` performs no writes and fails when
outputs are stale. Identical regeneration preserves output mtimes. Generated
files are committed; runtime builds do not require Node. The just test workflow
does require the contract toolchain.

The emitter is pinned to `0.1.0-dev.1788882678403`, shipped from the existing
`hafley-tsp/packages/rust` checkout through the loopback registry. That checkout
had pre-existing edits; this task did not modify its source or commit them.
The lockfile records the installed artifact. A new machine needs access to that
artifact or an explicitly republished/re-pinned emitter before installing.

## Implemented integration

- Existing SQL, IPC, and rendering code uses the generated `Row` without a wire
  layout change. The handwritten constructor remains outside generated code.
- The row producer, geometry renderer, rollback metadata stamping, and Godot HUD
  consume named fields. `Line` is also generated. Rust tests preserve unused row
  tails; Godot tests reorder kinds and check entity lookup. The live recording
  suite runs the Godot reader test before capture.
- `Boundary` implements the generated `FrameQuery` trait. Query errors map to
  declared errors, insufficient capacity preserves the caller's buffer, and
  success fills only the reported prefix.
- `Boundary` implements `RowPublisher`; borrowed rows contain an ordered replay
  batch. Publication preserves earlier history within the existing window and
  atomically replaces replayed ticks. The legacy nested-frame API shares this
  implementation. The live Godot adapter calls the generated publisher directly.
- Godot's pending packet implements `FrameAcknowledger`, validating the local
  generation/epoch, row digest, and vertex count. The existing Godot entry point
  checks every row and vertex before accepting that receipt and consuming the
  packet. The trait itself validates without consuming or recording a receipt.
- ReadBuffer/WriteBuffer template instances lower to borrowed Rust slices.
- Named Ok/Err unions lower to Rust Result aliases. Annotated model unions
  generate Rust enums and Godot shape dispatch.
- FramePayload, fixture/scheduled/external status variants, and MeshReceipt
  generate Rust dictionary conversions and typed GDScript classes. Packed
  arrays remain packed at the Godot boundary. Nullable fields preserve nil.
  The stage uses these classes for status and mesh acknowledgment; audit files
  retain JSON serialization. Boundary tests check fixture, scheduled, external,
  and controlled status variants.
- Equal positive minItems/maxItems become fixed Rust arrays. A maximum-only
  array becomes a Vec; the existing IPC adapter enforces its row limit.
- Native integral and string constants generate Rust constants; YAML stores integral decimal
  values as strings to preserve uint64 precision. `0_constants.mjs` isolates the
  pinned compiler's AST/internal value API, covered by literal/reference/range tests.
- `Latest` replaces the handwritten IPC envelope. A frozen v1 bincode fixture
  checks byte compatibility in both directions. Limits and protocol version are
  consumed by the SQLite/IPC adapters and peer publisher.
- Unsupported types, optional fields, defaults, and valued enums fail explicitly.

## Remaining work

Epoch zero identifies the current local adapter convention, not a negotiated
cross-process identity. Process restart/lease semantics are not implemented.
The query adapter retains the existing allocating SQL reader internally.
SQL view definitions, remaining transport bindings, shared-memory layout, and
zero-allocation enforcement remain follow-ups. Generated GDScript readers materialize dictionaries
(and arrays for matrix fields); their allocation cost is not benchmarked.
No zero-copy or zero-allocation claim is made for this slice.

## Repeated-hit proof

`just record repeat` runs 300 ticks in one continuous world using the existing
stationary-target mode, then records through generated rows, SQLite, and wgpu.
Inputs repeat the existing jump/attack fixture every 120 ticks. Assertions require
hits at 91 and 211, cumulative damage 36, exact replay of all 120 states from a
snapshot at tick 180, and exact SQL row readback for every displayed tick.
The report uses generated `RepeatProof`; timings and the CLI flag are native TSP
constants. Outputs live in a fresh temporary directory printed by the command.

This verifies repeated attack registration and snapshot replay. It does not
exercise a launched-target combo, a second network rollback, or new combat rules.
The separate live suite continues to cover the original launched-target scenario.

## Controlled-input proof

`just play` runs local keyboard input: A/D or arrows move, Space jumps, J starts
fair, Escape exits. Godot samples at its 60 Hz physics callback and sends generated
`ControlInput` dictionaries into Rust. TSP declares button and axis bounds; both
generated decoders enforce them. The Rust simulation also validates inputs.
World owns horizontal position, so snapshots include movement state.

`just record control` replays 300 input ticks through the same endpoint and
records H.264 at half-speed. Assertions require a hit at 91, damage 18, launched
bag motion, movement reversal, a second jump, exact SQL/mesh acknowledgments,
and 120 exact replayed states after restoring tick 180. The second fair misses.
`just test-controls` injects keyboard events through the Godot input API and
checks resulting movement, jump, fair, and mesh acknowledgment.

Horizontal motion is constant-speed, facing remains right, and vertical motion
uses the existing lab jump curve. These controls have no new netplay integration.
Manual mode retains no per-tick world/audit history; recording retains 300 worlds.
