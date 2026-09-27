---
created: 2026-08-16
updated: 2026-09-26
type: epic
owner: chrishafley
status: open
priority: high
labels: [domain-soopy, intent-architecture, artifact-runtime]
---

# 018 Soopy staged source mutations

## Description

## Goal

Provide a Git-optional source mutation boundary that accepts edits from DL6, Biome, ast-grep, rust-analyzer, and other producers; seals a deterministic preview; waits for approval of that exact stage; then applies and receipts the filesystem transaction.

## Contract

The detailed archaeology, type proposal, library survey, lifecycle, and implementation slices live in `sprefa/plans/2026-08-16-soopy-stage-commit-source-actions.RESEARCH.md`.

`stage` may persist immutable staged content but does not change target files, Git index, refs, or commits. `commit` revalidates the sealed stage and applies it to Git worktrees or plain directories.

## Acceptance Criteria

- [x] Byte-span edits from several producers normalize into one typed action algebra.
- [x] Per-file grouping rejects overlaps and stale source identities before target mutation.
- [x] A durable StageId identifies inputs, normalized actions, and resulting content.
- [x] Preview and explicit approval reference the exact StageId.
- [x] Commit supports Git worktrees and directories without Git.
- [x] Partial multi-file application has a typed journal and recovery path.
- [x] DL6 can derive proposals and consume commit receipts without owning filesystem mechanics.
- [ ] Large edit set measurement: `just perf-source-mutations files=1000 edits_per_file=100 bytes_per_file=4096 receipt=target/perf-source-mutations/100k-edits.json` (100,000 edits).
- [ ] Large file measurement: `just perf-source-mutations files=1 edits_per_file=100 bytes_per_file=16777216 receipt=target/perf-source-mutations/16mib-file.json` (one 16 MiB source file).
- [ ] Many-repository refresh measurement: `cargo run --release -p soopy --example 6_multi_repo_refresh -- --repositories 100 --rounds 3 --concurrency 4`.

## Tests Run

- [x] cargo test -p soopy
- [x] cargo clippy -p soopy --all-targets -- -D warnings
- [x] just test-source-mutations
- [x] just perf-source-mutations

## Repro receipt

2026-09-26: the remaining gates are measurement runs; the exact 100,000-edit, 16 MiB-file, and 100-repository commands are listed above. Status remains open until each has a receipt.
