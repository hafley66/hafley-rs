# Brief: make the Boop session-graph poll incremental

Work in `$PWD` (your worktree). Use `CARGO_TARGET_DIR=$PWD/target`.

## Problem (measured)
`instant` polls `boop_session_graph` every 3000 ms (instant `src/boopPanel.tsx:133`,
`src-tauri/src/0_boop.rs:1568 read_session_graph`). A 5 s `sample` of instant put
`read_session_graph` on 1003 of 4213 samples (about 24% of one thread) while idle.
Each poll re-does everything:
1. `open_store_ro()` on `~/.agent/boop.db`.
2. `bus::read_routes(dir)` and `bus::read_boxes(dir)` + `bus::parse_box` of EVERY mailbox file
   (`crates/boop-store/src/bus.rs:265,419,426`), unconditionally.
3. `proc::SysinfoSnapshot::capture()` (`crates/boop-store/src/proc.rs:104`) = `System::new_all()`
   then `refresh_processes(All)`. `new_all` also refreshes CPUs, disks, networks and
   components; `Components::new_with_refreshed_list()` alone measured ~86 ms per call on this Mac.
4. tmux liveness probes.
5. `load_agent_session_graph_with_runtime` (`_0_session_graph.rs:376`) with `include_history: true`.

## Goal
Cost of an unchanged poll ~0: nothing re-read unless its source changed.

## Required changes (boop-store)
- `SysinfoSnapshot::capture`: `System::new_with_specifics(RefreshKind::nothing().with_processes(<only the fields the graph reads>))`. No components, disks, networks, CPUs.
- Mailbox reads: incremental. Track per-file (len, mtime, byte offset) and parse only appended bytes; skip unchanged files. Same idea for routes (mtime gate).
- Store reads: progressive cursors. The graph query takes a watermark (max rowid / ts per table it reads: agent_session, agent_turn, agent_edge, route/shell tables) and returns only rows past it plus a new watermark; the caller merges. Check whether `boop db sync-cursor` / existing cursor tables already provide this and reuse them.
- Provide a stateful `SessionGraphReader` (or equivalent) that owns the cursors and the last graph, with `fn poll(&mut self) -> Result<Option<GraphDelta or Graph>>` returning None when nothing changed.

## Out of scope
instant repo changes (a follow-up wires the reader into `read_session_graph`). Do not touch unrelated crates.

## Proof
- Unit tests with fixture stores: unchanged poll returns None and performs no mailbox parse (count parses via a test hook); appended mailbox bytes parse only the tail; a new session row appears after one poll.
- A bench or timed test: second poll on a populated fixture is < 5% of the first.
- `cargo test -p boop-store` green; `cargo clippy -p boop-store` clean.

## Commits
One concern per commit, conventional subjects. Final commit subject:
`perf(boop-store): incremental session-graph reader`
