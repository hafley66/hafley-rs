---
created: 2026-08-18
updated: 2026-08-18
type: bug
status: open
priority: high
---

# boop adopt syncs the whole codex root under a fresh BOOP_DB; cargo test -p boop takes 14 minutes

## Description

## Description

Since #28 (69f00c1) `command_needs_startup_sync` (`crates/boop/src/main.rs:1019-1051`) lists `SubCmd::Adopt`. `boop adopt` writes a registry route and reads no transcript row, so the sync buys it nothing. Under a fresh `BOOP_DB` (as `tests/inbox_hooks.rs:73,83,411` and `coordinator_ping` do) every cursor starts at zero against the real `~/.codex/sessions` root (2.5 GB, 1034 `.jsonl`), re-parsed from offset 0 in `harness/codex.rs Codex::ingest -> project_line -> serde_json::from_slice` (541 of 684 samples), once per test process, eight in parallel, debug build.

Measured by the harness-model-spec lane 2026-08-18: `cargo test -p boop` 14 minutes; `coordinator_ping` 293.57s / 3 tests, `inbox_hooks` 530.85s / 8 tests, all of it this sync. Violates the 10-second law.

## Acceptance Criteria

- [x] `SubCmd::Adopt` (and any other verb that reads no transcript row: `hail`, `inbox`, `beep lane create`) is out of `command_needs_startup_sync`; the policy test at `main.rs:3378` pins it.
- [x] Tests that set a temp `BOOP_DB` also point every harness root at a temp transcript dir (one env or one `Registry` constructor), so no test can touch `~/.codex/sessions` or `~/.claude/projects`.
- [x] `cargo test -p boop --no-fail-fast` wall time reported before and after; `coordinator_ping` and `inbox_hooks` each under 30s.
- [x] `docs/failure-modes.md` entry: incident, RCA, fail-pre-fix test, rail.

## Comments

### 2026-08-19T01:32:21Z · @sprefa-coordinator

spawn-guards lane 2026-08-18 measured the same on the claude root: coordinator_ping 486s, inbox_hooks 465s, stack in serde_json::from_slice under ident::project_transcript re-parsing ~/.claude/projects (1.7 GB, 1620 .jsonl) once per boop invocation, nine in flight. Both roots, not only codex.

## Tests Run

Wall time, same machine, same tree, pre-built test binaries both times:

| target | daa2b0a | fix/boop-main-fixes |
|---|---|---|
| `cargo test -p boop --no-fail-fast` | 520.66s | 26.84s |
| `coordinator_ping` (3 tests) | 233.94s | 1.19s |
| `inbox_hooks` (8 tests) | 266.32s | 1.56s |

Both runs exit 0. After: 420 passed / 0 failed / 1 ignored.

## Implementation Notes

`command_needs_startup_sync` (`crates/boop/src/main.rs`) dropped `SubCmd::Adopt`,
`BeepCmd::Agent` and `LaneCmd::List`; each writes the registry or reads tmux and
touches no `agent_*` table. Every `tests/*.rs` that spawns the binary now sets
`HOME` to a directory under its own temp root, which is the one knob all four
adapter roots plus `bus::default_mail_dir` resolve through (`dirs::home_dir`).
Rail: `crates/boop/tests/temp_home_rail.rs`. Ledger: `docs/failure-modes.md`
entry 6.
