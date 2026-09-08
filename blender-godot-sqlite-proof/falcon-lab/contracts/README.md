# Presentation boundary contracts

`0_presentation.tsp` owns the row layout, generation/epoch envelope, error set,
publish/read/ack signatures, native boundary constants, and the `Latest` IPC
envelope. `just generate` compiles the actual TypeSpec
program and renders Rust through `@hafley66/alloy-rs` components. It also emits
a transport-neutral YAML description, not an OpenAPI HTTP document.

```sh
just contracts-setup
just generate
just check-generated
just test
just verify
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
- Named Ok/Err unions lower to Rust Result aliases. Other union shapes fail.
- Equal positive minItems/maxItems become fixed Rust arrays. A maximum-only
  array becomes a Vec; the existing IPC adapter enforces its row limit.
- Native integral constants generate Rust constants; YAML stores their decimal
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
GDScript emission, transport bindings, shared-memory layout, and zero-allocation
enforcement remain separate follow-up work. No zero-copy or zero-allocation
claim is made for this slice.
