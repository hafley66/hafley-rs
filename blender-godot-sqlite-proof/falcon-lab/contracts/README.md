# Presentation boundary contracts

`0_presentation.tsp` owns the row layout, generation/epoch envelope, error set,
and publish/read/ack signatures. `just generate` compiles the actual TypeSpec
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
- ReadBuffer/WriteBuffer template instances lower to borrowed Rust slices.
- Named Ok/Err unions lower to Rust Result aliases. Other union shapes fail.
- Arrays require equal positive minItems/maxItems and become fixed Rust arrays.
- Unsupported types, optional fields, defaults, and valued enums fail explicitly.

## Remaining work

Publish and acknowledgment traits are generated and compiled but do not yet
replace the existing publisher or Godot acknowledgment implementations. Epoch
zero identifies the current local query adapter convention, not a negotiated
cross-process identity. Process restart/lease semantics are not implemented.
The query adapter retains the existing allocating SQL reader internally.
GDScript emission, transport bindings, shared-memory layout, and zero-allocation
enforcement remain separate follow-up work. No zero-copy or zero-allocation
claim is made for this slice.
