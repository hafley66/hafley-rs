---
created: 2026-09-07
updated: 2026-09-26
type: task
reporter: chrishafley
status: open
priority: normal
labels: [domain-soopy]
provenance: codex
source_ref: sprefa:4847:soopy-transaction-contract
---

# Document and verify Soopy transaction visibility and recovery guarantees

## Description

## Evidence
CommitEngine uses a root lock, preflight, journal and sequential file operations. Tests in crates/soopy/tests/14_commit_engine.rs cover operation-boundary failpoints and recovery. Multi-file changes can be observed partially by external readers. A cooperative root lock does not itself stop unrelated filesystem writers.
## Acceptance Criteria
- [ ] Document per-file operation guarantees, multi-file visibility, cooperating-writer lock scope and behavior after interruption.
- [ ] Distinguish operation-boundary failpoint evidence from process-kill and power-loss evidence.
- [ ] Exercise an external writer between preflight and apply and record the actual refusal or overwrite behavior.
- [ ] Add subprocess interruption/recovery coverage where absent and record supported platforms/filesystems.
- [ ] Keep whole-tree snapshot isolation an explicit separate requirement, not an implied guarantee.
## Tests Run
`crates/soopy/tests/14_commit_engine.rs` contains operation-boundary failpoint tests; the external-writer race and subprocess interruption acceptance items have not been run. Related: @soopy-mutation-commit.

## Repro receipt

2026-09-26: `ryii move` dry-run checks preview bytes only and does not exercise commit interruption or concurrent external writers; transaction visibility and recovery evidence remains unchecked.
