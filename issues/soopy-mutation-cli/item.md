---
created: 2026-09-07
updated: 2026-09-26
type: feature
reporter: chrishafley
status: fixed
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
- [x] Expose the existing typed StageRequest through a CLI input boundary without adding language-specific transform semantics.
- [x] Return a durable StageId and preview without changing target files.
- [x] Commit only an explicitly named sealed stage; expose recovery and typed refusal results.
- [x] CLI fixture covers non-writing stage preview, explicit-ID commit, interrupted apply recovery, and replay.
- [ ] Follow-up CLI fixtures cover stale input, conflicts, create, move, and delete actions.
## Tests Run
`cargo nextest run -p soopy -j 2 --test main -E 'test(t18_mutation_cli::)'`: 1 passed. Related: @soopy-staged-mutations.

## Repro receipt

2026-09-27: commit `a2daf1f7`; `cargo nextest run -p soopy -j 2 --test main -E 'test(t18_mutation_cli::)'` passed. The fixture verifies preview leaves target bytes unchanged, explicit-ID commit applies the sealed replace, and recover replays a failpoint-interrupted commit. Additional action/refusal fixtures remain a follow-up.
