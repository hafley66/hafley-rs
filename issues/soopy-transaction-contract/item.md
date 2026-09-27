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
- [x] Document per-file operation guarantees, multi-file visibility, cooperating-writer lock scope and behavior after interruption.
- [x] Distinguish operation-boundary failpoint evidence from subprocess termination evidence; power loss is explicitly untested.
- [x] Exercise an external writer after preflight and before operation apply; recovery returns `RecoveryRequired`, preserves the journal and leaves external bytes unchanged.
- [x] Add subprocess interruption/recovery coverage; the child exits with status 86 after operation 0, then the parent recovers the journal.
- [x] Record tested environment: Darwin 23.6.0 arm64, APFS at `/tmp`; other platforms/filesystems remain unverified.
- [x] Keep whole-tree snapshot isolation an explicit separate requirement, not an implied guarantee.
- [ ] Power-loss/storage-controller evidence: run a VM or filesystem fault-injection harness against the target filesystem, then rerun `cargo nextest run -p soopy -j 2 --test main -E 'test(t14_commit_engine::)'` and attach the harness's cut-point receipt. No such harness is configured on this host.
## Tests Run
`CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo nextest run -p soopy -j 2 --test main -E 'test(t14_commit_engine::)'`: 15 passed, including external-writer refusal and subprocess termination/recovery. `stat -f /tmp` reported APFS; `uname -a` reported Darwin 23.6.0 arm64. No power-loss harness run. Related: @soopy-mutation-commit.

## Repro receipt

2026-09-26: the external writer changed the target after preflight while the journal was pending; `recover` returned `RecoveryRequired`, left the external bytes unchanged, and retained the journal. The subprocess test exited after operation 0 and recovered successfully in the parent. This documents recovery refusal after interruption; normal commits still have an external-writer race between preflight and apply. Power-loss durability remains unchecked with its required harness described above.
