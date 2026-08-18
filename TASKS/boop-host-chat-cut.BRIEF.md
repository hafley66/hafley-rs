# Brief: cut `boop run` down to `boop host chat`; boop stops booting engines

Plan: `/Users/chrishafley/projects/sprefa/plans/2026-08-18-boop-resident-coroutine.md`, section "boop's role, corrected". boop = database + events; the engine serves the dl6 program; the resident chat is one adapter row. #26 (`a676904`) put the whole loop in boop and had boop compile+boot the engine: wrong place.

## Base
Base sha = c434a04 (origin/main at spawn; if origin/main has moved past it since, that is fine, do NOT rebase, work on your spawn sha). Worktree `.boop-worktrees/fix/boop-host-chat-cut`, branch `fix/boop-host-chat-cut`. FIRST action: `git status` clean and `git log -1` = c434a04, else STOP AND REPORT. NEVER `git stash`. Never spawn subagents. Standing laws: no `eprintln!` in `src/**`, no em dashes, comment budget, banned identifiers provenance/substrate/load-bearing/regime.

## Deliverables
1. New verb `boop host chat`: reads ONE JSON request on stdin `{"resident": "<session name>", "model": "<model>", "goal": "<text, optional, first call only>", "prompt": "<text>"}`, opens (or reuses, keyed by `resident`) one `Rewriter::Chat` channel (`crates/boop/src/concatmap.rs:156-215`, keep `open_channel`, `pending_goal`, compact resume), sends `prompt`, blocks for the reply, prints ONE JSON row `{"reply_turn": <int>, "reply": "<text>"}` on stdout, exit 0; on failure prints `{"outcome":"failed","detail":"..."}` exit 0 (failures are rows, per the `boop host oneshot` shape on branch `feature/dl6-boop-concatmap-golden` `6b6315f`, read it for the JSON contract; do not merge that branch). Channel reuse across invocations: the channel lives in a small resident helper process keyed by `resident` under `~/.agent/run/<resident>/` OR the channel is re-opened with `resume` each call; pick after reading how `LaneChannel` resumes; write the reason in one comment.
2. Delete from `crates/boop/src/runner.rs`: engine compile (`swipl`), engine boot (`emit_rust_harness --socket`), the UDS client, delta following, turn pushing, `boop run` verb and its flags. Keep only what `boop host chat` needs. If `runner.rs` ends up under ~150 lines fold it into `concatmap.rs` or a `host.rs`; the `curl` dependency goes if nothing uses it. `docs/runner.md` becomes `docs/host-chat.md` (the verb, the JSON contract, one example).
3. `boop concatmap`: leave as is this PR.
4. Tests: `tests/host_chat.rs`: fake harness channel that echoes; two invocations against the same `resident` reuse the channel (COUNT: one open); a failure prints the failed row and exits 0. `cargo test -p boop --no-fail-fast` once at the end, `cargo clippy -p boop`.

## Files owned
`crates/boop/src/runner.rs` (or its replacement), `crates/boop/src/main.rs` (verb), `crates/boop/src/concatmap.rs` (seam only), `crates/boop/Cargo.toml`, `crates/boop/tests/host_chat.rs`, `crates/boop/tests/runner_e2e.rs` (delete), `crates/boop/docs/`. Nothing else.

## PR
`gh pr create --base main`. Body: 1-3 plain sentences (what `boop host chat` does, the stdin/stdout JSON), `## Reading order`, `## Tests` (name, input, expectation, printed before; "full suite unchanged otherwise"). No words gate/leg/receipt/door/probe/refusal, no em dashes, no suite counts. Do NOT merge. Report PR number, head sha, test result lines, exact error text on failure.
