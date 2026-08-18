# Brief: boop.chat runner, `boop run <program.dl6>` (phase 1b)

Plan: `/Users/chrishafley/projects/sprefa/plans/2026-08-18-boop-resident-coroutine.md`. Read it whole first, especially "Runner contract".

## Base
Your branch starts at 062be72 (also kept as `wip/boop-chat-runner-opus`) = origin/main 6578544 + ONE unverified WIP commit from an earlier lane that died mid-way (`runner.rs` scaffold, `concatmap.rs` seam edits, one Cargo dep). FIRST action: `git log -2` shows 062be72 on top of 6578544, else STOP AND REPORT. Read `git show --stat HEAD` and the diff; keep what is right, fix or delete what is not; nothing in it has been run or built. Squash or amend as you like; the PR is judged on its final diff. NEVER `git stash`. Never spawn subagents. Standing laws: no `eprintln!` in `src/**` (`tracing` only), no em dashes, comment budget, banned identifiers provenance/substrate/load-bearing/regime, surrogate INTEGER keys for any table.

## What the runner is
One rx operator, in Rust: `session_turns$ -> engine -> resident_ask deltas -> concatMap(resident chat send) -> resident rows back into the engine`. The engine holds the program (`resident-coroutine.dl6`, sprefa plan section 3) behind a UDS socket. The runner:

1. Compiles and boots the program on a socket. Shell out to the sprefa engine harness (no crate link): `emit_rust_harness <program.rs> <schedule.json> --socket <path>` (`~/projects/sprefa/v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs:9-11`); compile step per `~/projects/sprefa/v6/sprefa-engine-rs/tests/15_source_mutation_hosts.rs:14-18`. Locate the sprefa checkout through `SPREFA_ROOT` env (default `~/projects/sprefa`); the compiled `.program.rs`, schedule and socket live under `~/.agent/run/<name>/`.
2. Pushes the source session's turns from `~/.agent/boop.db` into rel `turn` via `POST /arrive` (`ArrivalDto{rel, sign, values}`, `serve.rs:32,225`), initial backfill then poll every `--poll` (default 5s) for new `agent_turn` rows (`turn_rows`, `concatmap.rs:536` shows the query shape).
3. Follows `GET /rel/resident_ask/deltas?since=<tick>` (long-poll; the sprefa lane `feature/rel-deltas-route` is adding it in parallel; until it lands, poll `GET /rel/resident_ask` and diff against rows already answered; write the code against the deltas shape `{"tick", "add": [[..]], "del": [[..]]}` behind one small trait so the swap is one impl).
4. For each new `resident_ask` row, in `user_run` order within a batch: send `prompt` to ONE resident chat channel (`Rewriter::Chat`, `crates/boop/src/concatmap.rs:156-215`, keep `open_channel`, `pending_goal`, compact resume), block for the reply, then `POST /arrive` `resident(session, user_run, reply_turn, reply)`. Serial: the next ask waits for this reply. Skip an ask whose `resident` row already exists (`GET /rel/resident`) so a restart never re-asks.
5. HTTP over UDS: pick a library after a 3-candidate look (`hyper` + `hyperlocal`, `ureq` has no UDS, `reqwest` unix socket feature); write the candidate table into the PR body; smallest dependency that does UDS wins.

## CLI
`boop run <program.dl6> --session <source session id> --resident-model <model> [--goal <text|@file>] [--poll 5s] [--name <run name>]`. Add beside `Concatmap` in `crates/boop/src/main.rs:272`. `boop concatmap` keeps working this PR but its help gains one line pointing at `boop run`; delete `concatmap.rs` cursor file, done markers, `coalesce_jobs`, `out/` writing ONLY if `boop concatmap` still passes its tests without them; if not, leave deletion for the follow-up and say so.

## Files owned
`crates/boop/src/runner.rs` (new), `crates/boop/src/main.rs` (verb + dispatch only), `crates/boop/src/concatmap.rs` (seam extraction), `crates/boop/Cargo.toml` (one http dep), `crates/boop/tests/runner_e2e.rs` (new), `crates/boop/docs/runner.md` (new, short: the operator, the CLI, the contract table). Nothing else in `crates/`. Never touch `~/projects/sprefa`.

## Tests, one at a time while iterating; whole battery once at the end
- unit: batch ordering by `user_run`; skip-already-answered; deltas trait fake.
- `tests/runner_e2e.rs`: a fake engine (tiny hyper server on a temp socket serving `/rel/resident_ask/deltas`, `/rel/resident`, `/arrive`) and a fake `Harness` channel that echoes; assert N asks -> N `resident` arrivals in order, restart -> 0 re-asks. COUNT assertions, not end-state only.
- Once at the end: `cargo test -p boop --no-fail-fast` and `cargo clippy -p boop`; report the per-target `test result:` lines.

## PR
`gh pr create --base main`. Body: 1-3 plain sentences on what a user gets with the `boop run` line, `## Reading order` (files, why), `## Tests` (name, input, expectation, what it printed before; one line "full suite unchanged otherwise"), plus the UDS-http candidate table. No words gate/leg/receipt/door/probe/refusal, no em dashes, no suite counts. Do NOT merge. Report: PR number, head sha, test result lines, exact error text on any failure.
