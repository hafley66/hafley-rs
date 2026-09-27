---
created: 2026-09-18
updated: 2026-09-18
type: feature
status: done
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: extract-rename
closed: 2026-09-18
---

# Lane E: rust rename seats (#[path], field/variant, serde spellings, fn-body use)

## Description


Plan section "Lane E". rust_rename.rs:1035 #[path] layout; field and enum-variant seats; serde rename text_spellings at types.rs:2982; a `use` inside a fn body scopes the name. Brief unwritten; write TASKS/lane-e-rust-rename.BRIEF.md first.

## Acceptance Criteria
- [x] brief written with receipts per seat
- [x] tests/5_rename_rust.rs gains one case per seat
- [x] scip_verify agrees
