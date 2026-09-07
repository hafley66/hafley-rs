---
created: 2026-09-07
updated: 2026-09-07
type: task
reporter: chrishafley
status: untriaged
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
Source and failpoint tests inspected; no execution or reproduced data-loss claim. Related: @soopy-mutation-commit.
