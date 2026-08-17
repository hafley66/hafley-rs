---
created: 2026-08-17
updated: 2026-08-17
type: bug
status: open
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-observability, needs-chris]
size: M
---

# Native Claude Code subagents register nowhere in boop

## Description

Chris, 2026-08-17: "when I cmd+period in Claude Code I only see the main ones,
not all of them; I don't think boop is working well."

Four registries hold agent-like things and none of them is a union of the
others. `beep lane list` reads ONE of them.

| agent / lane | `beep lane list` | live tmux pane | `~/.agent/lanes/<name>/` | `.claude/worktrees/agent-*` | `boop.db agent_lane` |
|---|---|---|---|---|---|
| sprefa-coordinator | live, coordinator | yes (`sprefa-5`) | no | n/a | no |
| extract-driver | live, kind=native | no | no | no | no |
| codex-grid-await | live, coordinator | no | no | no | no |
| agent-a02500df67080336d (`fix/typegen-emitter-s-findings`) | NO | no | no | yes, locked | no |
| agent-a0f89d9d244a15646 (`chore/gate-legs-...`) | NO | no | no | yes, locked | no |
| agent-a8e0dc6e82d870d3d (`fix/lang-modulepath-in-wrapper`) | NO | no | no | yes, locked | no |
| agent-a90d5ea54986d77de (stale, Aug 14) | no | no | no | yes | no |
| tmux `sprefa` (codex), `projects-3` (codex), `projects-2` (node) | NO | yes | no | n/a | no |
| tmux `sprefa-2`, `sprefa-3` (bash) | no | yes | no | n/a | no |
| feature-capitalized-relation-names | NO (route gone) | no | yes, supervise.log under 60 min old | no | yes (spawn_id 254) |
| 34 other `~/.agent/lanes/*` | 4 shown as `dead` | no | yes | no | mostly yes |
| 5 dead routes (codex-590, codex-707, 3 soopy) | dead, DEAD=no-trail | no | no | no | mixed |

Counts: `registry.json` 8 keys, `~/.agent/lanes` 35 dirs, `agent_lane` 254 rows.

cmd+period is Claude Code's own in-session task list. It reads none of these,
so boop is not the cause of that specific list being short; the finding is that
boop cannot see the native subagents either.

## Sites

- `beep lane list` reads `bus::read_routes(&dir)` = `~/.agent/mail/registry.json` ONLY, at `crates/boop/src/main.rs:4780` / `crates/boop/src/bus.rs:57`.
- Its one reconcile against reality is liveness: `tmux::mux().live_sessions` at `main.rs:4781`, `lane_state` at `main.rs:4842`. Pane-less `coordinator` and `native` routes are hardcoded live forever at `main.rs:4847`.
- No union with tmux sessions, with `~/.agent/lanes`, or with the store.
- `agent_lane` is written by exactly one path: `record_lane_purpose` (`main.rs:1968`), called once from lane create (`main.rs:2524`).
- Native Claude Code subagents DO reach `agent_session` / `agent_edge` (327 sessions with cwd under `sprefa/.claude/worktrees/agent-%`), but only through `boop db sync`, run by launchd every 600 s (`~/Library/LaunchAgents/com.hafley.agentperf.sync.plist`), whose last exit was `-9`.
- `boop adopt` (`main.rs:2600`) requires a live tmux session (`main.rs:2622`), so it cannot take a pane-less native subagent.
- `boop agent register --kind native` (`main.rs:4549`, CLI decl `main.rs:4068`) can take one, and writes a route only, never `agent_lane`.

## Fix options, not picked

| option | site | cost |
|---|---|---|
| union live tmux sessions with no route into `lane list` as `unrouted` | `main.rs:4781`, the session list is already in hand | S, display only |
| union `~/.agent/lanes/*` dirs with no route as `orphan-trail` | `main.rs:4835` already opens `trail::lanes_root()` | S |
| union `agent_session` rows whose cwd matches `%/.claude/worktrees/agent-%` | new query beside `crates/boop/src/query.rs:503` | M, gated on sync freshness |
| Claude Code PostToolUse hook on the `Agent` tool calling `boop agent register --kind native`, plus a done-side hook | `~/.claude/settings.json` (3 hook entries today, none boop); verbs at `main.rs:4551` and `main.rs:4592` | S |
| relax adopt's tmux requirement so it accepts a pane-less agent | `main.rs:2622` | S, weakens the guard |
| make `agent register` also write `agent_lane` | call `record_lane_spawn` (`crates/boop/src/ident.rs:1092`) from `main.rs:4551`, the way `main.rs:2524` does | M |
| fix the sync job (last exit -9) so the store-derived views are current | `com.hafley.agentperf.sync.plist`, `StartInterval 600` | unknown until the kill cause is read |

## Acceptance Criteria

- [ ] One command answers "what agents exist right now" across all four registries; the table above is reproducible from its output.
- [ ] A native Claude Code subagent with a `.claude/worktrees/agent-*` tree appears in that output while it is alive.
- [ ] A live tmux session with no boop route is reported, not silently absent.
- [ ] Pane-less `coordinator` / `native` routes stop being hardcoded live (`main.rs:4847`); liveness comes from something measurable.
- [ ] The `boop db sync` launchd job's `-9` exit is diagnosed and recorded, or the fix does not depend on that job.
- [ ] Which option above is taken is Chris's call; this card is triage.

## Tests Run

## Implementation Notes

Triage only, read-only. Nothing was changed to produce this table.
