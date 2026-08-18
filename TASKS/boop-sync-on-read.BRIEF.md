# Brief: every boop read syncs first, and the incremental sync is sub-second

Chris, 2026-08-18: "boop should be auto doing what we are asking at speed of rust, esp if we are incrementally knowing how much data we need to scan." No server, no daemon, no launchd. The database is fresh because every read verb syncs the new bytes first, and that pass is cheap.

## Base
Base sha = cbf9fd7 (origin/main at spawn; if origin/main moves past it, do NOT rebase, work on your spawn sha). Worktree `.boop-worktrees/fix/boop-sync-on-read`, branch `fix/boop-sync-on-read`. FIRST action: `git status` clean and `git log -1` = cbf9fd7, else STOP AND REPORT. NEVER `git stash`. Never spawn subagents. Laws: no `eprintln!` in `src/**` (`tracing` only), no em dashes, comment budget, banned identifiers provenance/substrate/load-bearing/regime, surrogate INTEGER keys.

## Facts measured today
- The ingest is already incremental: byte cursor per (session, transcript path) in `sync_cursor` (`crates/boop/src/ident.rs:798-825`).
- One incremental pass with nothing new: ~1.0-1.4s wall (`time boop db sync create`), `boop db "<sql>"` alone 0.03s. So the cost is NOT reading bytes; it is listing every session of every harness adapter each call (`sync_all` `main.rs:1285`; same shape as `run_follow` `main.rs:1351` which rebuilds the whole session list per tick).
- Only these verbs sync before answering (`command_needs_startup_sync`, `main.rs:1068`): Events, Chat, Agent, Concatmap, Run, Me favorite. `db`, `beep lane list`, `debug`, `sessions`, `tail`, `me` (plain) answer from a database as stale as the last launchd tick (600s, `com.hafley.agentperf.sync`).

## Deliverables
1. Every read verb syncs first: add `Db { .. }` (all read subcommands, including the plain `boop db "<sql>"` and `sync-cursor list`), `Beep` lane list/status, `Debug`, `Sessions`, `Tail`, `Me` (all forms), `Harnesses` to `command_needs_startup_sync`. Writes that also read (`agent register`, `adopt`) too. `db sync create` itself and `inbox drain` (hook path, must stay fast: measure it; if the sub-second pass holds, include it) decide by measurement, state the numbers in the PR.
2. Make the incremental pass sub-second with nothing new, and proportional to changed bytes otherwise:
   - per harness adapter, stat the sessions ROOT dir(s) mtime first (`Harness::sessions()` implementations under `crates/boop/src/harness/*.rs`); unchanged root mtime AND no known session file mtime moved (keep a small in-db table `sync_root_stamp(harness_id, root_path_id, mtime_ms)`, INTEGER keys, dictionary for the path) -> skip that adapter entirely.
   - only then list sessions, compare per-session `modified_ms` against the stored stamp, read bytes from the cursor for the moved ones.
   - COUNT test: with zero changes, statements executed per pass is a small constant you name (target under 20) and no transcript file is opened; with one changed session, exactly that session's file is read from its cursor. Use the existing statement counting the tests already use (grep `count_cursor_sql` `ident.rs:2308` and the trace tables).
   - time pinned in a test: zero-change pass under 100ms on the fixture store; report the real number on `~/.agent/boop.db` in the PR (`time boop db "select 1"` before and after).
3. Delete the launchd dependence: nothing in the repo may say the database is fresh because of `com.hafley.agentperf.sync`; grep docs and `--help` text, fix wording. Do not touch `~/Library/LaunchAgents` (Chris's machine); say in the PR that the 600s job can be removed.
4. `run_follow` (`main.rs:1334`, `--forever`): reuse the same cheap pass; no separate session-list rebuild.

## Files owned
`crates/boop/src/main.rs`, `crates/boop/src/ident.rs`, `crates/boop/src/harness/*.rs` (only to expose root dirs / mtimes), `crates/boop/src/query.rs`, `crates/boop/tests/**`, `crates/boop/docs/**`. Nothing else in `crates/`.

## Tests
One at a time while iterating; once at the end `cargo test -p boop --no-fail-fast` and `cargo clippy -p boop`. Report the per-target `test result:` lines and the before/after timings.

## PR
`gh pr create --base main`. Body: 1-3 plain sentences (what a user gets: every boop answer is current, and the sync pass costs X ms with nothing new), `## Reading order`, `## Tests` (name, input, expectation, printed before; "full suite unchanged otherwise"). No words gate/leg/receipt/door/probe/refusal, no em dashes, no suite counts. Do NOT merge. Report PR number, head sha, test result lines, timings, exact error text on any failure.
