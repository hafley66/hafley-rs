---
created: 2026-09-18
updated: 2026-09-18
type: feature
status: in-progress
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: extract-resolver
---

# K1: kotlin phase-1 receiver plane and receiver leg

## Description


kotlin.rs:1898 is name-only ("no receiver typing"); aux.receivers is empty for kotlin. Add a receiver walk over the tree-sitter-kotlin tree (pattern: src/lang/ts_receivers.rs, rust_receivers.rs): `val x: T`, `T(...)` constructor, typed params, `this` inside class/object, property types, generic bound `<P : Proj>`; record ReceiverBinding per navigation-call site; add the `receiver` leg into the corpus class/object/interface member table ahead of module_plane and name-match. Fixtures under tests/fixtures/kotlin_receivers/ (never tests/fixtures/kotlin/, the scip corpus).

## Acceptance Criteria
- [ ] tests/137_kotlin_receiver_legs.rs: field, ctor-return, typed param, generic-bound, this legs bind with origin receiver
- [ ] shadowed local (param or val named like a corpus fn) declines with drop reason inferred, without reading the df plane
- [ ] full crate gate green
