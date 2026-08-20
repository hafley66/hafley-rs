---
created: 2026-08-16
updated: 2026-08-17
type: task
assignee: terra
status: done
priority: high
epic: soopy-staged-mutations
labels: [domain-soopy, intent-verification]
commits:
- hash: 36df6f348
  summary: 'engine: compiled DL6 source mutation golden'
- hash: c75a51121
  summary: Carry typed host descriptors through ProgramJson
closed: 2026-08-17
closed_by: codex
---

# Run compiled DL6 mutation golden end to end

## Description

## Objective

Compile authored source-mutations.dl6, execute the emitted program through the Rust host runtime, and prove stage quiescence, exact approval, commit receipt, and target bytes in one integration fixture.

## Acceptance Criteria

- [x] The test compiles the authored DL6 fixture rather than hand-constructing HostPlanData.
- [x] Source evidence derives one proposal and preview without mutating the target.
- [x] A wrong proposal or StageId produces no commit demand.
- [x] Exact approval on a later tick produces an ordinary commit receipt and changed target bytes.
- [x] Restart and idempotent replay use the durable stage and commit stores.

## Tests Run

- [x] focused compiler golden
- [x] focused Rust integration
- [ ] cargo clippy --offline --all-targets -- -D warnings
- [ ] git diff --check

## Agent Runs

### 2026-08-17T22:19:03Z · @codex

Completion-first reconciliation found the implementation already present in /Users/chrishafley/projects/sprefa. Verified fresh SWI-Prolog compile of v6/dl/fixtures/source-mutations.dl6, focused Rust integration v6/sprefa-engine-rs/tests/15_source_mutation_hosts.rs (1 passed), focused TypeScript host test (1 passed), and git diff --check. Repo-wide clippy remains unchecked because unrelated pre-existing warnings occur in tests/dep_resolve.rs, tests/list_boundary.rs, and tests/live_hosts.rs. Full TSV2 TypeScript check is separately blocked by unrelated pre-existing errors in labs/1_rtkq-extraction-golden.ts and tests/listReadSurface.test.ts. No scoped code changes were required.

## Resolution

### 2026-08-17T22:19:03Z · @codex

All acceptance criteria are implemented by the recorded Sprefa commits and passed focused end-to-end verification. The unrelated repository-wide gate failures remain recorded in Agent Runs.
