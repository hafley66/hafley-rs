---
created: 2026-09-18
updated: 2026-09-18
type: chore
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: repo-hygiene
---

# Remove dead extract-lane worktrees and fast-forward primary main

## Description


Eight worktrees under .boop-worktrees/feature/extract-lane-{b-meter,b-meter-2,c-gots,c-ktpy,c-ktpy-3,c-rust,c-rust-2,c-ts-shadow} plus extract-lane-d-untyped; branches b-meter 8de62717, c-ktpy 125f5e87, c-rust 68b15d3c are superseded wip. Primary checkout main is behind origin/main. The agent session is refused both operations by the permission classifier; user runs them.

## Acceptance Criteria
- [ ] worktrees removed, branches kept or deleted per user
- [ ] primary main fast-forwarded
