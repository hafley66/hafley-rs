---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: fixed
priority: normal
labels: [extract]
closed: 2026-09-25
---

# graph grades SCIP and checker edges as guesses

## Description

## Description
grade_sql (crates/sprefa-extract/src/bin/ryi/0_sqlite.rs:94) maps only resolution_origin module_plane to "+". Edges answered by the SCIP oracle (origin scip) or a compiler checker (origin checker) grade "~", the same as a name-match guess. `ryi graph --slow --callers fetch_ref` over tests/fixtures/ratchet_soopy prints grade "~" for a SCIP-answered edge.

## Acceptance Criteria
- [x] scip and checker origins grade "+"
- [x] a graph test pins the grade of a --slow edge
