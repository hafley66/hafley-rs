# Brief: quiet yields for coordinator routes (lane `feature-boop-quiet-yields`)

## First action
```
git merge --ff-only e0b0a1dc308d8ab059596e945bda7e8e4212a555
cargo build -p boop 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as feature-boop-quiet-yields sprefa-coordinator "<one line>"`.

## Defect
A coordinator pane receives one `kind=yield` hail per lane commit
(`crates/boop-proc/src/supervise.rs:1427`, "commit <lane> a..b dirty=n"),
per idle turn (`:1350` `idle_body`), and per retire (`:985`). Measured
2026-08-29 14:00-14:35Z on route `claude-3611`: 13 yield rows, 3 result, 3
note. Each injected row costs the coordinator a full harness turn. Only
`request` and `result` rows carry a decision.

## Build
1. `agent_route` gains a column `quiet_yields INTEGER NOT NULL DEFAULT 0`
   (migration in `crates/boop-store`, additive).
2. `boop adopt --quiet-yields` and `boop beep lane create --quiet-yields`
   set it on the route being written; `boop route set <route> quiet_yields=1`
   if a route-edit verb already exists, else add the flag to `adopt` only.
3. `mail_to_parent_kind` (supervise.rs) with `kind == YIELD`: when the
   parent route has `quiet_yields=1`, append the row to the mailbox
   (the trail stays complete for `boop beep lane list` and self-diagnosis)
   and SKIP `deliver_outbound`. `result`, `request`, `note` deliver as today.
4. `boop beep lane list` output is unchanged.

## Tests, fail-first
In `supervise.rs` tests beside `an_idle_park_mails_the_parent_one_yield_row`:
quiet parent -> yield row appended, zero delivery rows; loud parent ->
unchanged; a `result` row to a quiet parent still delivers.

## Receipt
`cargo test -p boop -p boop-proc -p boop-store` SUM. Push,
`gh pr create --base main`, hail
`boop beep --no-wait --as feature-boop-quiet-yields sprefa-coordinator "boop quiet yields: PR #N, gate <p>/<f>"`.

## Laws
No em dashes. No `eprintln!` in src (tracing). Comments state constraints
only, no dates. Banned identifiers: provenance, substrate, load-bearing,
regime. Never `--no-verify`. No `cargo fmt` outside files you touch.
