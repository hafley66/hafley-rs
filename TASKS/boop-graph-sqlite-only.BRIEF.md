# Brief: the session-graph read is a pure SQLite read

Work in `$PWD` (your worktree). Use `CARGO_TARGET_DIR=$PWD/target`. Base: branch
`perf/boop-graph-incremental` (commits 6ce4f40a, ca2293bd, 9bf91bb6).

## Problem (measured)
`load_agent_session_graph_with_runtime` / `runtime_snapshot` (`crates/boop-store/src/runtime.rs`)
compute liveness at read time: per call ~21 `tmux` + ~10 `bash` spawns, a `sysinfo` process
snapshot, and a parse of every mailbox file; ~1 s per call. instant polled it every 3 s
from boot and the macOS kernel zone `data.kalloc.1024` grew ~100/s until reboot
(instant issues/kernel-wired-growth). Readers must not cause side effects.

## Required design
1. Liveness is WRITTEN, not computed on read. Whatever already observes lanes (the lane
   supervisor in `crates/boop`, HeadWatch, `lane create`/exit paths, `boop tui` registration)
   writes liveness facts into SQLite when they change: pane alive/dead, pid alive/dead,
   tmux session, last-seen ts. One table (or columns on the existing lane/route rows) with an
   index; a schema migration in boop-store.
2. The graph read (`load_agent_session_graph*`, `boop db agent-summary`, and the path instant
   calls) reads ONLY SQLite: no process spawn, no tmux, no sysinfo, no mailbox-file parse.
   Mailbox content the graph needs comes from `agent_mail` rows already in the store.
3. Staleness is data: the read returns last-seen ts; the caller decides "stale".
4. Keep `SessionGraphReader` (data_version gate) from 9bf91bb6; with (2) its skip is correct.
5. If some caller truly needs a live probe, expose it as a separate explicit function, never
   inside the graph read.

## Regression tests (as far up as possible)
- CLI level (crates/boop integration test): run the built `boop` binary with
  `PATH=<tmpdir with shims>:...` where `tmux`, `bash`, `sh`, `ps`, `lsof`, `pgrep` are shims
  that append their argv to a log file and exit 0. Run `boop db agent-summary` (and any other
  read verb that projects the graph, e.g. `boop db sessions`, `boop db status` if it reads the
  graph) against a fixture store. Assert the shim log is EMPTY.
- Store level: a `Multiplexer` and `ProcReader` test double that panics on any call; the graph
  read must succeed with them.
- Writer level: supervisor/liveness writer test proving a pane death flips the stored row.
- Name the CLI test so its purpose is obvious: `graph_read_spawns_nothing`.

## Proof
`cargo test -p boop-store -p boop` green, `cargo clippy` clean for touched crates.

## Commits
One concern per commit. Final commit subject exactly:
`perf(boop): session-graph reads are SQLite-only`
