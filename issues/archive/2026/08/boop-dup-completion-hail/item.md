---
created: 2026-08-17
updated: 2026-08-18
type: bug
status: fixed
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-correctness, component-supervisor]
size: S
closed: 2026-08-18
---

# Every lane completion hail arrives twice in the coordinator inbox

## Description

Every lane completion arrives TWICE in the coordinator inbox. Observed today by
the sprefa coordinator, `boop inbox drain --as sprefa-coordinator --hook prompt`:

```
[boop m-b7cb552e from capitalized-relation-names] lane ... done rc=0
[boop m-b1ee600f from capitalized-relation-names] lane ... done rc=0
```

Live diff of that pair (`~/.agent/mail/bus.ndjson:1774` vs `:1775`): the ONLY
differing columns are `id` and `from_timestamp`, 4.4 ms apart. `from`, `to`,
`kind`, `body`, `reply_to` and `ref` are identical.

## Root cause: two unconditional writers, no dedupe

| # | writer | site | body |
|---|---|---|---|
| A | supervisor in-process, on every exit path | `crates/boop/src/supervise.rs:579` `record_result`, called at `:203` (panic), `:233` (supervisor error), `:253` (normal exit), `:286` (signal death); row built `:588-597`, appended `:615` | `lane <l> done rc=N` |
| B | pane shell epilogue | composed `crates/boop/src/main.rs:2442`, run as `boop hail --kind result` through `run_hail` (`main.rs:1790` row, `:1801` append) | identical |
| C | native subagent verb `boop beep agent done` | `crates/boop/src/main.rs:4602-4612` | identical, not implicated in the two observed incidents |

Every lane pane runs one command, `boop beep lane run ...` (`crates/boop/src/harness.rs:94-100`),
wrapped by `with_on_exit` (`harness.rs:111`, `harness.rs:226`, shape
`; __rc=$?; <epilogue>; exit $__rc`) for all four harnesses (`harness/claude.rs:68`,
`harness/codex.rs:46`, `harness/opencode.rs:83`, `harness/kimi.rs:45`). A runs
inside `boop beep lane run` (`main.rs:4683` to `run_lane_supervisor` `main.rs:1905`
to `supervise::run` to `record_result`); the shell then runs B about 4 ms later.
Measured gaps: `20:44:40.952505Z` vs `.956887Z`, and `21:21:04.240801Z` vs `.245377Z`.

The pair is blessed in-tree at `supervise.rs:562-564` ("the pane epilogue
addresses its result hail the same way, so both rows answer the same wait") and
by the test `a_duplicate_result_row_leaves_the_wait_unchanged`
(`main.rs:3430-3454`). That reasoning covers `lane wait` and never covers
`inbox drain`. `docs/failure-modes.md` entry 49 added A on top of B.

No dedupe exists: ids come from `bus::mint_id()` (`crates/boop/src/bus.rs:313-323`,
4 random bytes) so A and B never collide; appends are raw (`main.rs:2745-2758`,
`supervise.rs:615`) with no UNIQUE and no already-sent guard; the drain filters
by id only (`main.rs:2089` to `inbox::undelivered` `inbox.rs:218-223` to
`mailwait::unread_for` `mailwait.rs:84-89`). Two ids means two printed lines.

## Ruled out

- Drain replaying an already-drained row: the ledger (`inbox.rs:188-196`) is per-id and both ids are genuinely new.
- The two name spellings doubling ONE completion: `bus.ndjson:1773` (20:29, lane `capitalized-relation-names`) and `:1778` (20:58, lane `feature-capitalized-relation-names`) are two distinct dispatches 29 minutes apart, each producing its own A/B pair. The spelling difference comes from `crates/boop/src/lane.rs:126-131`: with `--branch feature/x` and no `--lane`, `lane = slug(branch)` = `feature-x` (`lane.rs:83-93`, test `lane.rs:441`); an explicit `--lane` keeps the short form. Cosmetic, separate from this bug.

## Fix fork

- Smallest: delete the `boop hail --kind result` half of the epilogue at `main.rs:2442`, keeping `boop beep lane delete --route-only`. Caveat: `completion_recipient` (`main.rs:2582-2586`) gives the epilogue a `__wait__<lane>` recipient when no parent resolved, while `record_result` bails when `registered_parent` (`supervise.rs:562-565`) is `None`, so `--wait` with no parent would lose its only row. Teach `record_result` the same `__wait__<lane>` fallback at `supervise.rs:580`.
- If the second row must stay for pane-death coverage (the supervisor can be SIGKILLed before `record_result`), dedupe at drain in `inbox::undelivered` (`inbox.rs:218-223`) on `(from, kind, body)` within a short window.

## Acceptance Criteria

- [x] One lane completion produces exactly ONE line from `boop inbox drain`.
- [x] `boop beep lane wait <lane>` still returns the lane's rc, with a parent and without one (`--wait` with no parent covered).
- [x] A lane whose supervisor is SIGKILLed before `record_result` still produces a completion row; test pins it. NOT MET, answered differently: the epilogue was the half that ran after a SIGKILL and it is gone, so a SIGKILLed supervisor writes no row. `lane wait` reads the dead route and exits 3 (`a_dead_route_with_no_result_row_exits_3`, `main.rs`), and `trail::dead_reason` returns `DiedBeforeResult` (`a_dead_lane_always_carries_a_typed_reason`, `trail.rs`). Chris 2026-08-18: "exit code is fine"; closed.
- [x] The test `a_duplicate_result_row_leaves_the_wait_unchanged` (`main.rs:3430`) is updated or replaced, not deleted silently.
- [x] Fail-first receipt: a test that fails on today's tree by counting 2 drained rows for one completion.
- [x] `docs/failure-modes.md` entry 49 amended with the drain-side consequence (renumbered to entry 5).

## Tests Run

Landed before daa2b0a. Verified on fix/boop-main-fixes: `lane::pane_epilogue`
writes no row, `one_lane_exit_writes_exactly_one_result_row`
(`crates/boop/tests/lane_completion_row.rs`) passes, whole crate 420 passed /
0 failed / 1 ignored.

## Implementation Notes

`~/.agent/boop.db` has no messages table; the bus is the ndjson file
`~/.agent/mail/bus.ndjson`. Style laws apply: comment budget, no `eprintln!`
in `src/**`, no em dashes.
