---
created: 2026-09-14
updated: 2026-09-14
type: improvement
status: open
priority: normal
---

# Render a compact lane list with optional columns

## Description

Preserved uncommitted candidate as archival commit 594cf20fb53001c75f53c7d97009286ca097ed59, ref archive/boop-cleanup-20260914/wip/fix/lane-list-liveness. render_lane_table formats state/name/kind/harness/model/age/tail and omits empty columns; includes an untracked lane_list_e2e.rs now captured in the commit. Main already has liveness reconciliation. Port the presentation behavior without reinstating older liveness heuristics. Snapshot is unvalidated; worktree removed.
