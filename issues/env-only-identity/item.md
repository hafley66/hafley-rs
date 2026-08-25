---
created: 2026-08-24
updated: 2026-08-25
type: improvement
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
size: M
---

# Identity is BOOP_SESSION only; natives pass --as

## Description

## Description

Five rungs name the caller: env `BOOP_LANE`/`BOOP_SESSION`, registered pane, harness process sniffing, `--as`, `--from`. Run cx-a (plan addendum 02:25): native-n1 resolved as `feature-cx-a` and watched the wrong inbox.

Cut: boop sets `BOOP_SESSION` on every process it spawns; `--as` overrides; everything else in `boop whoami` is deleted. `--from` becomes a hidden alias of `--as`.

## Acceptance Criteria

- [x] `whoami` has two rungs: `--as`, env
- [x] a caller with neither gets one error naming `--as`
- [x] pane and process sniffing code removed from `crates/boop/src/cli/` and `crates/boop-store/src/_0_session_graph.rs`

## Agent Runs

### 2026-08-25T04:32:20Z · @feat-identity-presets

843f0ae (branch feat/identity-presets, rebased onto main 8bbbd09).

| rung | source | confidence |
|---|---|---|
| 1 | `--as <name>` (`--from` hidden alias) | named |
| 2 | env `BOOP_SESSION`, `BOOP_LANE` for pre-stamp spawns | stamped |
| none | one line naming `--as`, exit 2 | unresolved |

Deleted: `Harness::identity_process` and its claude/codex/kimi impls, `from_pane`, `caller_pane`, `route_owns_pane`, `reject_two_routes_on_one_pane`, `live_session_for_route`, `named_by_route`, `register_fresh_codex_spawner` (cli/job.rs), and `native_codex_shell_for_focus` + `codex_rollout_session` + `owned_codex_root` from boop-store/src/_0_session_graph.rs (pane pid -> descendant env vars -> open rollout files). 717 lines net removed.

Receipt, rebuilt binary:
- `boop whoami --as feat-identity-presets` -> `rung as (named)` and `rungs --as (hit), env BOOP_SESSION (miss)`
- `env -u BOOP_SESSION -u BOOP_LANE boop whoami` -> `boop cannot tell who is calling: this process carries no BOOP_SESSION stamp; name yourself with --as <name>`, rc=2
- a native subagent inside the chore-luna-receipt codex lane registered `luna-native` and its `boop whoami --as luna-native` printed both rungs with `parent feat-identity-presets` read from the stamp.

`cargo test --workspace` green.
