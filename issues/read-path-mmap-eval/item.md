---
created: 2026-09-25
updated: 2026-09-25
type: task
status: open
priority: normal
related: ['@fast-rows-columnar']
labels: [extract]
---

# read path: memmap2 and blake3 mmap+rayon measured at ~1%

## Description

## Description

Evaluated per user direction: memmap2 for the read path and blake3 `mmap` + `rayon` hashing (`Hasher::update_mmap_rayon`).

Measured, 3000 registry .rs files, 40.8 MB, warm page cache:

- `cat` of every file, serial: 40ms; 10-way parallel: 20ms
- ryi's read is now plain parallel `fs::read` inside the per-file extraction task (soopy's batched worktree read spent git subprocesses and was dropped for the syntax path), so the read overlaps parsing
- content hashing is per file on the extraction pool (`phase:"hash"` is not in the top 25 trace rows for this run)

The whole run is ~3.0s wall; the read + hash bound is about 1%. mmap would also hand `&[u8]` borrows to rows that today copy text (see the arena/columnar issue), which is where its value is, so it rides that change rather than landing alone. blake3 rayon hashing splits ONE large input across threads; inputs here are many small files already hashed in parallel, so it buys nothing measurable.

## Acceptance Criteria
- [ ] revisit when rows borrow from the source buffer (arena/columnar issue)
