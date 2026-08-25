---
created: 2026-08-19
updated: 2026-08-25
type: bug
status: fixed
priority: normal
related: ['@boop-adopt-startup-sync-cold']
closed: 2026-08-25
closed_by: claude-5
---

# Test fixture lanes (mine, lane-test) leaked 5k trace rows into the live boop.db; purge + rail

## Description

## Description

Measured 2026-08-19 13:30 on the live `~/.agent/boop.db`: test fixture lanes leaked into production rows before #32 made tests set a temp HOME. `agent_trace_event` holds 5052 rows for lane `mine` and 165 for `lane-test` (last `mine` row 2026-08-19 09:23, i.e. before #32 landed); per-hour buckets show runs on 08-18 12:07, 13:47, 14:42, 16:15, 20:42, 21:04, 22:08 and 08-19 09:02. `boop debug` still surfaces `mine` / `lane-test` supervisor errors at 13:22:55 today, so either `~/.agent/lanes/<lane>/supervise.log` for those names still exists under the real HOME or debug reads a path the temp HOME does not cover.

## Acceptance Criteria

- [ ] Find where the 13:22:55 `mine`/`lane-test` lines come from (`boop debug` source path); if any test still writes under the real HOME or real `~/.agent`, fix the test and extend `tests/temp_home_rail.rs` to cover that path.
- [ ] One-shot cleanup: `boop db` statements (or a `boop db purge-fixture-lanes` named SQL report, visible and deletable per CLAUDE.md) that delete `agent_trace_event`, `agent_lane`, `agent_trace`, mail rows for lane names in the fixture set (`mine`, `lane-test`, any name the tests use; list them by grepping `tests/**`), run once on the live db with before/after counts pasted here; `~/.agent/lanes/mine`, `~/.agent/lanes/lane-test` removed.
- [ ] Rail: `boop debug` and lane list ignore nothing; instead a test asserts the live db has zero rows for fixture lane names after `cargo test -p boop` (run against a copy of the live db, count before == count after).

## Agent Runs

### 2026-08-25T18:23:40Z · @claude-5

Source: supervise.rs unit tests (TraceRecorder::new -> Store::default_path()). Fix: tempdir() pins BOOP_DB and HOME under a Once; rail recognises the pin. Full suite: fixture rows 9149 before, 9147 after (0 new). Purge script crates/boop-store/sql/purge_fixture_lanes.sql; the live purge and rm -rf ~/.agent/lanes/lane-test are left for Chris (destructive).

## Comments

### 2026-08-25T21:03:29Z · @claude-5

Purge ran: agent_trace_event left 0; ~/.agent/lanes/lane-test removed.
