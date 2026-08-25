# Brief: boop parent visibility

Read BOTH `plans/2026-08-24-boop-opencode-supervision-failures.PLAN.md` (sections 6 to 11: delivery transitions, opencode transcript projection, HEAD-without-yield diagnostic, tell-parent receipt, clap-parsed help examples) and `plans/2026-08-24-boop-parent-visibility.PLAN.md` (`boop push`, `boop debug <lane>`, verb audit). Order: (1) supervision-failures 7.2 + 7.3 + 7.5 delivery receipts, (2) 7.4 opencode projection, (3) 7.6 HEAD-without-yield diagnostic, (4) 7.7 help examples parsed by clap, (5) `boop push`, (6) `boop debug <lane>`, (7) verb audit table. One commit per numbered item.

Ownership: you own `crates/boop`, `crates/boop-harness`, `crates/boop-store`. Do not touch `crates/hafley-observe`, `crates/soopy`, `crates/boop-mux`, `crates/boop-proc`.

Laws:
- Commit after each deliverable. After EVERY commit run exactly: `boop tell-parent --kind yield --body "<sha> <deliverable> <test cmd> <pass/fail counts>"`. If that command errors, put its full stderr in the REPORT and continue.
- Validation: `cargo build --release -p boop && cargo test -p boop -p boop-harness -p boop-store`. Paste real output into the REPORT.
- Style: no em dashes, no words `provenance substrate load-bearing regime` in prose or identifiers, no negative parallelism, tables over prose.
- Before writing any new polling/wait loop, check what `boop wait` and `beep lane wait` already use and reuse it.
- End with `boop tell-parent --kind completion --body "REPORT at TASKS/boop-parent-visibility.REPORT.md"`.
