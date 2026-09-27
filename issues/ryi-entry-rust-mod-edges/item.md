---
created: 2026-09-25
updated: 2026-09-26
type: bug
status: obsolete
priority: normal
labels: [extract]
closed: 2026-09-25
---

# ryi --entry misses Rust mod edges: resolved_import carries no mod x; rows

## Description

## Description
`ryi ... --entry soopy/src/lib.rs` (branch ryi/inputs-cli, stub `reach_files` in crates/sprefa-extract/src/project.rs) reaches 19 of soopy's 29 src files. The crawl follows `resolved_import` target_path, and `RustModuleIndex::bindings` emits `use` bindings only: `mod x;` edges live in `RustModuleIndex.module_paths` (crates/sprefa-extract/src/lang/rust_modules.rs ~429) and never become rows. The other 10 files hang off `mod` declarations.

## Acceptance Criteria
- [ ] one `resolved_import` row of kind `module` per `mod x;` whose target is in the universe (ResolvedImportKind::Module already exists; python and kotlin emit it)
- [ ] `--entry soopy/src/lib.rs` over crates/soopy reaches all 29 src files
- [ ] ratchet 170 unchanged

## Tests Run

## Implementation Notes
Planned as the modrows milestone of plans/2026-09-25-ryi-cli-cleanup.md.

## Repro receipt

2026-09-26: current `ryii fast --entry crates/soopy/src/lib.rs crates/soopy/src` reaches 28/29 Rust files; the sole missing file is independent `main.rs`, so `mod x;` edges are present.
