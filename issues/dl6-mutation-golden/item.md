---
created: 2026-08-16
updated: 2026-08-16
type: task
assignee: terra
status: open
priority: high
epic: soopy-staged-mutations
labels: [domain-soopy, intent-verification]
---

# Run compiled DL6 mutation golden end to end

## Description

## Objective

Compile authored source-mutations.dl6, execute the emitted program through the Rust host runtime, and prove stage quiescence, exact approval, commit receipt, and target bytes in one integration fixture.

## Acceptance Criteria

- [ ] The test compiles the authored DL6 fixture rather than hand-constructing HostPlanData.
- [ ] Source evidence derives one proposal and preview without mutating the target.
- [ ] A wrong proposal or StageId produces no commit demand.
- [ ] Exact approval on a later tick produces an ordinary commit receipt and changed target bytes.
- [ ] Restart and idempotent replay use the durable stage and commit stores.

## Tests Run

- [ ] focused compiler golden
- [ ] focused Rust integration
- [ ] cargo clippy --offline --all-targets -- -D warnings
- [ ] git diff --check
