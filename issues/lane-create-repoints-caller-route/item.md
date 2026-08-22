---
created: 2026-08-22
updated: 2026-08-22
type: bug
status: open
priority: high
labels: [domain-boop, intent-fix]
size: S
---

# `boop beep lane create` repoints the CALLER's coordinator route at the new lane's pane

## Receipt (sprefa-coordinator, 2026-08-22, twice)

Before: `boop beep lane get sprefa-coordinator` ->
`"tmux":"boop-turn-visibility-v2:0.0","cwd":"/Users/chrishafley/projects/sprefa"`.
Run `boop beep lane create --branch feature/shared-frontier-round2 ... --model opus`.
After: the same route reads `"tmux":"feature-shared-frontier-round2:0.0",
"cwd":".../.boop-worktrees/feature/shared-frontier-round2"`.

Every hail to the coordinator then lands in the lane's pane. Same happened at
03:37 with three spawns in a row (the third won). Workaround each time:
`boop adopt --name sprefa-coordinator --tmux boop-turn-visibility-v2:0.0 ...`.

## Expected

`lane create` registers a route for the NEW lane only; the caller's route
(resolved as `parent: sprefa-coordinator (from caller)` in the dry-run output)
is read, never written.
