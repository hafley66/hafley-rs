# Brief: boop parent visibility

Read `plans/2026-08-24-boop-parent-visibility.PLAN.md` in this worktree and implement Deliverables 1, 2, 3 in that order.

Ownership: you own `crates/boop`, `crates/boop-harness`, `crates/boop-store`. Do not touch `crates/hafley-observe`, `crates/soopy`, `crates/boop-mux`, `crates/boop-proc`.

Laws:
- Commit after each deliverable. After EVERY commit run exactly: `boop tell-parent --kind yield --body "<sha> <deliverable> <test cmd> <pass/fail counts>"`. If that command errors, put its full stderr in the REPORT and continue.
- Validation: `cargo build --release -p boop && cargo test -p boop -p boop-harness -p boop-store`. Paste real output into the REPORT.
- Style: no em dashes, no words `provenance substrate load-bearing regime` in prose or identifiers, no negative parallelism, tables over prose.
- Before writing any new polling/wait loop, check what `boop wait` and `beep lane wait` already use and reuse it.
- End with `boop tell-parent --kind completion --body "REPORT at TASKS/boop-parent-visibility.REPORT.md"`.
