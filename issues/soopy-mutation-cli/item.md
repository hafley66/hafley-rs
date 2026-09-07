---
created: 2026-09-07
updated: 2026-09-07
type: feature
reporter: chrishafley
status: untriaged
priority: normal
labels: [domain-soopy]
provenance: codex
source_ref: sprefa:4847:soopy-mutation-cli
---

# Expose Soopy stage commit and recovery through the CLI

## Description

## Request
User requested tracking all observed Soopy codemod gaps. Semantic selection and correctness belong to sprefa-extract + Sprefa; Soopy remains the source identity and mutation boundary.
## Evidence
Installed soopy --help exposes show-stage and discard-stage, plus source read/watch commands. It exposes no stage-producing, commit, or recovery command. Rust APIs exist: plan_mutations, stage_mutations, CommitEngine::commit and recover. See crates/soopy/src/_7d_mutation_plan.rs, _7e_stage_store.rs and _7f_commit.rs.
## Acceptance Criteria
- [ ] Expose the existing typed StageRequest through a CLI input boundary without adding language-specific transform semantics.
- [ ] Return a durable StageId and preview without changing target files.
- [ ] Commit only an explicitly named sealed stage; expose recovery and typed refusal results.
- [ ] End-to-end CLI fixtures cover stale input, conflicts, create/replace/move/delete, interrupted apply and replay.
## Tests Run
Help and source inspected; no new tests executed. Related: @soopy-staged-mutations.
