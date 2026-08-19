---
created: 2026-08-19
updated: 2026-08-19
type: bug
status: open
priority: normal
related: ['@boop-adopt-startup-sync-cold']
---

# Test fixture lanes (mine, lane-test) leaked 5k trace rows into the live boop.db; purge + rail

## Description

## Description

Measured 2026-08-19 13:30 on the live `~/.agent/boop.db`: test fixture lanes leaked into production rows before #32 made tests set a temp HOME. `agent_trace_event` holds 5052 rows for lane `mine` and 165 for `lane-test` (last `mine` row 2026-08-19 09:23, i.e. before #32 landed); per-hour buckets show runs on 08-18 12:07, 13:47, 14:42, 16:15, 20:42, 21:04, 22:08 and 08-19 09:02. `boop debug` still surfaces `mine` / `lane-test` supervisor errors at 13:22:55 today, so either `~/.agent/lanes/<lane>/supervise.log` for those names still exists under the real HOME or debug reads a path the temp HOME does not cover.

## Acceptance Criteria

- [ ] Find where the 13:22:55 `mine`/`lane-test` lines come from (`boop debug` source path); if any test still writes under the real HOME or real `~/.agent`, fix the test and extend `tests/temp_home_rail.rs` to cover that path.
- [ ] One-shot cleanup: `boop db` statements (or a `boop db purge-fixture-lanes` named SQL report, visible and deletable per CLAUDE.md) that delete `agent_trace_event`, `agent_lane`, `agent_trace`, mail rows for lane names in the fixture set (`mine`, `lane-test`, any name the tests use; list them by grepping `tests/**`), run once on the live db with before/after counts pasted here; `~/.agent/lanes/mine`, `~/.agent/lanes/lane-test` removed.
- [ ] Rail: `boop debug` and lane list ignore nothing; instead a test asserts the live db has zero rows for fixture lane names after `cargo test -p boop` (run against a copy of the live db, count before == count after).

## Tests Run

`cargo test -p boop --no-fail-fast` on 2672085: 22 targets, 453 passed, 0
failed, 1 ignored (`concatmap_e2e`), 34.2s wall. `cargo clippy -p boop`: 0
warnings.

New rail `no_new_src_unit_test_reaches_the_machine_s_own_agent_root` in
`crates/boop/tests/temp_home_rail.rs`, 0.05s for the file's 2 tests. Two
sabotages fire it:

- delete `"harness/claude.rs"` from `SPAWN_WAIVED` ->
  `these src modules spawn a SpawnSpec with env_stamp: None, so the supervisor inherits the real HOME: ["harness/claude.rs"]`
- add an unmatched `"channel.rs"` to `STORE_WAIVED` ->
  `these waivers no longer match anything and must be deleted: ["channel.rs"]`

## Implementation Notes

**Where the 13:22:55 lines come from.** `boop debug` reads
`trail::lanes_root()` = `dirs::home_dir()/.agent/lanes/<lane>/supervise.log`
(`trail.rs:13-21`), no env override. The four 13:22:55 lines are in the real
`~/.agent/lanes/lane-test/supervise.log`, one of them
`WARN boop::supervise: lane supervisor signalled lane="lane-test" signal=1 name="SIGHUP"`.
Each carries `cwd=/private/var/.../T/boop-temprepo-wt-2289-{0,1,2}`, which is
`TempRepo::new()`'s naming at `src/test_support.rs:27-29`: one `cargo test
--lib` run, three harnesses, PID 2289.

**Both leaks are live at 2672085, and both are in `src/` unit tests.** Counting
`agent_trace_event` rows and the log size around each cargo target:

| target | `mine` rows | `lane-test` rows | `supervise.log` bytes |
|---|---|---|---|
| `--lib` | +37 | 0 | +663 |
| `--lib supervise::` | +37 | 0 | 0 |
| `--lib harness::claude` | 0 | 0 | +215 |
| `--lib harness::codex` | 0 | 0 | +233 |
| `--lib harness::opencode` | 0 | 0 | +218 |
| `--lib trail:: / worktree:: / runtime:: / lane::` | 0 | 0 | 0 |
| `--test parent_death`, `parent_failure_hail`, `lane_completion_row`, `lane_wait_exit`, `lane_carcass`, `wait_mail`, `native_agent_liveness`, `coordinator_ping`, `inbox_hooks` | 0 | 0 | 0 |

Store side: `supervise.rs:997` `remember_conversation` calls
`Store::default_path()`, which reads `BOOP_DB` and otherwise `dirs::home_dir()`
(`ident.rs:526-531`). The lib test binary sets neither. Seven
`supervise::tests::*` reach it, +37 rows per run.

Trail side: `spec()` at `harness/claude.rs:556`, `harness/codex.rs:715`,
`harness/opencode.rs:862` sets `env_stamp: None`, so the
`boop beep lane run --lane lane-test` string that `harness.rs:141`
`supervisor_command` builds runs under the test process `HOME`.

**Why the #32 rail could not see either.** `tests/temp_home_rail.rs` reads only
`tests/*.rs`, and only files containing `CARGO_BIN_EXE_boop`. The `src/`
modules are neither: they compile into the lib test binary, and the harness
tests reach the binary through tmux, never through a `Command` on
`CARGO_BIN_EXE_boop`.

**Fixture set.** `mine`, `lane-test`, `lane-a`, `lane-x`, `test-lane`,
`fake-lane`, `some-lane`, `orphan-lane`, `durable-lane`, `sibling`, `chore-x`.
Only `mine`, `lane-test` and `fake-lane` are interned in `dict_session` at all,
and only the first two carry rows. `coordinator`, `sprefa-coordinator`,
`shell` and every `feature-*` name are used by tests AND by the live machine,
so they are excluded by name in the SQL comment.

**Counts, `sqlite3 -header -column ~/.agent/boop.db < crates/boop/sql/fixture_lanes.sql`.**

| table | lane | rows at 13:30 | rows at 14:20 |
|---|---|---|---|
| agent_trace_event | mine | 5052 | 5284 |
| agent_trace_event | lane-test | 165 | 165 |
| agent_trace | * | 1 | 1 |
| agent_trace_span | * | 1 | 1 |
| agent_edge | * | 1 | 2 |
| agent_lane | * | 0 | 0 |
| agent_session, agent_turn, agent_usage, agent_live, agent_live_span | * | 0 | 0 |
| dict_session | * | 3 | 3 |

The second column is after this lane's own attribution runs, which is the leak
reproducing on demand. `agent_trace_event` total 5774, so the fixture share is
90%.

**Not done: the purge.** `crates/boop/sql/purge_fixture_lanes.sql` and its one
run against `~/.agent/boop.db` are outstanding. The store as it stood is backed
up at `~/.agent/boop.db.bak-2026-08-19` (sqlite3 `.backup`, integrity_check
ok), and `~/.agent/lanes/lane-test/` (`child.stderr`, `supervise.log`) is still
on disk. Nothing else under `~/.agent` was touched.
