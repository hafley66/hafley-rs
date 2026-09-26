---
created: 2026-09-26
updated: 2026-09-27
type: bug
reporter: codex
status: fixed
priority: normal
---

# ryi cleave batch composes invalid Rust in soopy Pattern source

## Description

On a copied `crates/soopy` corpus, run `ryi cleave --list batch.tsv --root <copy>/crates/soopy --state <outside-state> --commit` from the copied crate directory. `batch.tsv`:

```text
src/_1_pattern.rs#Pattern	src/_1b_extract.rs
src/_0a_durable_write.rs#DeviceSyncCounts	src/_0b_counts.rs
```

The first row plans `Pattern` and updates its importers. The second row plans `DeviceSyncCounts` and its impl. The batch then exits 2 before staging with `cleave batch leaves invalid Rust in src/_1_pattern.rs: expected '!'`. A single dry-run cleave of `Pattern` exits 0, so the invalid text appears while composing the rows. The source tree remains untouched after the batch error.

## Root cause and fix

Batch `Plan::land` applied raw row respells to its overlay, while the single-row stage normalized equivalent edits before applying them. A repeated edit could be composed twice. Batch import cleanup also accepted a rewrite without checking that the intermediate Rust file still parsed. Normalize row edits before overlay and retain the last valid Rust text when an import cleanup candidate is malformed. The `cleave_ladder` Pattern then Counts row pins the source output and checks the resulting crate.

Fix commits: `ede155a3`, `ae8c373e`.

Receipt: `batch_keeps_every_row_source_parseable_while_composing`; `t_166_cleave_rust` passed (16 tests).
