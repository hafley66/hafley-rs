---
created: 2026-09-14
updated: 2026-09-14
type: feature
status: open
priority: normal
---

# Choose recoverable TUI sessions from a revival picker

## Description

Preserved uncommitted candidate as archival commit d37eaecf1e0e72b70da6139fc929329214b955e0, ref archive/boop-cleanup-20260914/wip/feature/tui-revive. SessionDigest and revive_candidates add transcript summaries, a since window and candidate selection. Main already has a respawn loop; this picker remains absent. Port only this distinct behavior and its tests after reviewing identity semantics against current main. Snapshot is unvalidated; worktree removed after complete source capture.
