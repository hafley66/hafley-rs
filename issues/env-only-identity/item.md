---
created: 2026-08-24
updated: 2026-08-24
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

- [ ] `whoami` has two rungs: `--as`, env
- [ ] a caller with neither gets one error naming `--as`
- [ ] pane and process sniffing code removed from `crates/boop/src/cli/` and `crates/boop-store/src/_0_session_graph.rs`
