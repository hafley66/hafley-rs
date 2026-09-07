---
created: 2026-09-07
updated: 2026-09-07
type: task
reporter: chrishafley
status: untriaged
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
- [ ] Inventory actual producer adapters versus schema-only adapters.
- [ ] Add or locate a fixture that runs a real producer, stages its edits, applies them and checks exact output plus stale/conflict refusals.
- [ ] Keep semantic decisions in producers, including Sprefa + Extract. No parser or type solver requirement for Soopy.
- [ ] Record comparable phase timings and memory for the same edit set, separating matching, planning, staging and durable application.
- [ ] State durability, corpus, hardware, versions and test commands; do not infer speed rankings from unrelated workloads.
## Tests Run
Source inspected; peer documentation compared; no comparative benchmark run. Related: @soopy-staged-mutations, @soopy-edit-producers, @soopy-multi-repo-refresh.
