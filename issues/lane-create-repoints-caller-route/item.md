---
created: 2026-08-22
updated: 2026-08-23
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

## Comments

### 2026-08-23T05:40:07Z · @sprefa-coordinator

Receipt 2026-08-23 05:16: two lane creates repointed the route again; the workaround 'boop adopt --name sprefa-coordinator --tmux <pane>' with no other flags restored tmux but wrote harness=null cwd=null model=null and the NEW LANE's session_id onto the coordinator route. Consequence: 'boop beep hail sprefa-coordinator' from lane ordered-tick-recompute was refused ('route has no harness field'); the lane fell back to the cross-session socket. Adopt must preserve the fields it is not given, and lane create must not write its session_id onto the caller's route.

### 2026-08-23T18:41:57Z · @sprefa-coordinator

2026-08-23 18:38: 'boop beep lane create --harness codex --model gpt-5.6-sol@high' (and the preset 'sol' which expands to the same string) dies at ACP handshake: 'Invalid params: model gpt-5.6-sol@high (this agent takes: gpt-5.6-sol, ...)'. The @effort suffix reaches the codex ACP agent unstripped; boop-acp/src/channel/acp.rs:617 says the split exists for opencode. lane create prints 'dispatched' and exits 0 with no route and no tmux session. Workaround: bare gpt-5.6-sol; ~/.codex/config.toml model_reasoning_effort=high supplies the effort.

