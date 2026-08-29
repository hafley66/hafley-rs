# Brief: quiet yields for coordinator routes (lane `feature-boop-quiet-yields`)

## First action
```
git merge --ff-only f3a2bfe9faf4c660c09f64717a275f2d6c4ffc21
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

## Build (user decision 2026-08-29, supersedes any flag design)
No new column, no flag. Two rules in `mail_to_parent_kind` / the retire path:
1. A lane that has already written a `result` or `request` row to its parent
   mints NO further `idle` or `retired` yield rows. The parent already has the
   answer; a later `boop beep <lane> <body>` revives the session as today.
2. `kind=yield` rows (commit, idle before any result, rewind) are appended to
   the mailbox for the trail and are NEVER passed to `deliver_outbound` when
   the parent route has `kind=coordinator`. Lane-to-lane parents keep today's
   delivery. `result`, `request`, `note` deliver as today.

## Tests, fail-first
In `supervise.rs` tests beside `an_idle_park_mails_the_parent_one_yield_row`:
a lane with a prior result row parks idle -> zero new rows; a lane with no
result row parks idle -> one yield row appended, zero delivery rows when the
parent is a coordinator; a `result` row still delivers; a lane parent still
receives yields.

## Receipt
`cargo test -p boop -p boop-proc -p boop-store` SUM. Push,
`gh pr create --base main`, hail
`boop beep --no-wait --as feature-boop-quiet-yields sprefa-coordinator "boop quiet yields: PR #N, gate <p>/<f>"`.

## Laws
No em dashes. No `eprintln!` in src (tracing). Comments state constraints
only, no dates. Banned identifiers: provenance, substrate, load-bearing,
regime. Never `--no-verify`. No `cargo fmt` outside files you touch.
