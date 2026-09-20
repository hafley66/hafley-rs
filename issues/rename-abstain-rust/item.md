---
created: 2026-09-20
updated: 2026-09-20
type: improvement
status: open
priority: normal
epic: extract-parity-move-rename
related: ['@rename-abstain-record']
labels: [extract, artifact-cli, component-rename]
---

# rename: Rust arm emits abstain rows for untyped field accesses

## Description

## Description

`feat/rename-abstain` (38d0b2b9) gave the TS arm `RenameAbstain` rows: a site the arm found but could not type rides alongside the plan, exit 7. The Rust arm still returns the empty vec and stops the whole run.

`src/lang/rust_rename.rs:577`: `FieldSite::Access { ty: None }` pushes a `SymbolSeat { form: "untyped field" }`; `:112` turns any seat into `RenameStop::Dynamic`. When at least one field access typed to `owner` and at least one did not, the run should emit the plan for the typed sites plus one `RenameAbstain` per untyped access (`reason` from the `UnresolvedReason` vocabulary, `receiver` = source text of the receiver expression). `Dynamic` stays for the case where every reachable site is untyped. Glob-import seats (`:555`) stay stops: a glob that may write the name is not a per-site abstain.

Contract already in place: `Rename::occurrences -> Result<(Vec<SymbolRef>, Vec<RenameAbstain>), RenameStop>` (`ts_rename.rs:46`). Exit 7 and `--json` `abstains` array in `src/0_rename.rs:190-231` need no change.

## Acceptance Criteria
- [ ] Rust arm returns abstain rows instead of `Dynamic` when at least one field access typed
- [ ] `Dynamic` kept when zero sites typed; glob-import seats unchanged
- [ ] test `tests/150_rename_abstain_rust.rs` through `CARGO_BIN_EXE_ryi`: fixture with one typed and one untyped `.old` field access; exit 7, one seat, one abstain, inline snapshot
- [ ] `cargo test --features cli --no-fail-fast` green from `crates/sprefa-extract`

## Tests Run

## Implementation Notes

## Comments

## Decisions
