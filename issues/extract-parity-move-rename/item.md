---
created: 2026-09-18
updated: 2026-09-18
type: epic
status: open
priority: normal
labels: [extract]
---

# sprefa-extract: fast/slow parity, kotlin receivers, move/rename

## Description

Plan: crates/sprefa-extract/plans/2026-09-17-fast-slow-parity-and-move-rename.md. Lanes A, B, C, D, F landed on main at 794cc623 (lane D report: plans/reviews/2026-09-18-lane-d-untyped-REPORT.md). Division of labor with dl8: crates/sprefa-extract/AGENTS.md. Model routing for lanes: preset glm53f-omp, ONE lane and ONE cargo at a time.

## Acceptance Criteria
- [ ] RATCHET.tsv wrong_target 0 on rust, ts, kotlin rows, on the sprefa bench corpora
- [ ] kotlin receiver plane at the rust/ts bar (K1, K2, K3)
- [ ] ts field rename and rust rename seats (lane E) land
