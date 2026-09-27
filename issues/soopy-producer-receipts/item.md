---
created: 2026-09-07
updated: 2026-09-26
type: task
reporter: chrishafley
status: open
priority: normal
labels: [domain-soopy]
provenance: codex
source_ref: sprefa:4847:soopy-producer-receipts
---

# Verify real codemod producer integration and comparable mutation receipts

## Description

## Evidence
crates/soopy/src/_7c_edit_producers.rs exposes from_ast_grep_parts over scalar fields; BiomeBatchMutationContract explicitly describes a dependency-free contract rather than a live Biome runtime type. tests/15_source_mutations.rs constructs edits labeled ast-grep and dl6 manually. This proves envelope/planner plumbing, not actual producer execution.
@soopy-edit-producers is done and already notes executable-versus-contract boundaries. This is a follow-up evidence task, not a reopening of that completed scope.
## Acceptance Criteria
- [x] Inventory actual producer adapters versus schema-only adapters: `ProducedEdit` wraps existing `TextEdit` / `Utf8TextEdit`; `from_ast_grep_parts` accepts scalar fields and does not call ast-grep; `BiomeBatchMutationContract` is schema-only; no runtime adapter exists for Biome, ast-grep, rust-analyzer or DL6.
- [ ] Add or locate a fixture that runs a real producer, stages its edits, applies them and checks exact output plus stale/conflict refusals.
- [x] Keep semantic decisions in producers, including Sprefa + Extract. Soopy's adapter module only converts edit payloads; it has no parser or type solver dependency.
- [ ] Record comparable phase timings and memory for the same edit set, separating matching, planning, staging and durable application.
- [ ] Record durability, corpus, hardware, versions and test commands with the comparable producer receipt.

## Remaining gates

- [ ] Real producer integration: add `t10_edit_producers::real_ast_grep_stage_apply_refusals` to `crates/soopy/tests/10_edit_producers.rs`, then run `CARGO_TARGET_DIR=$HOME/.cache/boop/cargo-target cargo nextest run -p soopy -j 2 --test main -E 'test(t10_edit_producers::real_ast_grep_stage_apply_refusals)'`. The current host has no `ast-grep` executable, so this gate needs the producer fixture/dependency setup.
- [ ] Comparable phase and memory receipt: `cargo run --release -p soopy --example 5_source_mutations_scale -- --files 1000 --edits-per-file 100 --bytes-per-file 4096 --receipt target/perf-source-mutations/producer-comparison.json`, extended to include real producer matching time before accepting the receipt.
## Tests Run
`CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo nextest run -p soopy -j 2 --test main -E 'test(t10_edit_producers::)'`: 5 passed. No real producer or comparative benchmark run.

## Repro receipt

2026-09-26: `ryi move /tmp/soopy-producer-repro/src/old.rs /tmp/soopy-producer-repro/src/new.rs --root /tmp/soopy-producer-repro --state /tmp/soopy-producer-repro-state` produced a built-in move preview and left the tree unchanged. The current run exercises no external producer and records no matching, planning, staging or durable-apply memory/timing comparison. Remaining gates above stay open.
