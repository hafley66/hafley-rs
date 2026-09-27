---
created: 2026-09-14
updated: 2026-09-26
type: improvement
status: done
priority: normal
closed: 2026-09-26
---

# Render a compact lane list with optional columns

## Description

Preserved uncommitted candidate as archival commit 594cf20fb53001c75f53c7d97009286ca097ed59, ref archive/boop-cleanup-20260914/wip/fix/lane-list-liveness. render_lane_table formats state/name/kind/harness/model/age/tail and omits empty columns; includes an untracked lane_list_e2e.rs now captured in the commit. Main already has liveness reconciliation. Port the presentation behavior without reinstating older liveness heuristics. Snapshot is unvalidated; worktree removed.

## Resolution

### 2026-09-27T03:35:07Z · @issuectl

Replaced the fixed-width lane rows with a compact state/name/kind/harness/model/age/tail table that omits empty columns; cargo nextest run -p boop -j 2 -E test(lane_list_omits_empty_columns_and_bounds_model_width) passed.
