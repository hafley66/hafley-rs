# Fix: boop tests touch the real store; sync observation budget (issues `boop-tests-touch-real-store`, `sync-observation-budget`)

## 1. Hermetic tests (HIGH)
On 2026-10-01 21:00 `cargo test -p boop-proc --lib` and/or `cargo test -p boop --bin boop` on a schema-40 build
opened the user's real ~/.agent/boop.db and migrated it. Find every test that resolves the store (or mail dir,
trail dir, config, ~/.cache/boop) through the default HOME-based path.
- HOW TO RUN TESTS SAFELY while hunting: always run cargo test with `HOME=<lane worktree>/scratch/home`
  (create it), `BOOP_DB` and `BOOP_MAIL_DIR` unset, and a read-only sentinel: create
  `<scratch>/home/.agent/boop.db` as an empty file with `chmod 000`; any test that touches it fails loudly.
  Never run any test or boop binary with the real HOME.
- Fix every offender to use a temp dir. Then add the guard: under cfg(test) (or a debug-only check reachable from
  tests), resolving the store/mail/trail path under the process's real home panics with a message naming the path.
  Prove it with one test that tries.
- Report the offending tests (file:line) and the path each touched.

## 2. Sync observation budget
Live store at schema 40 logs on every sync: `trace join observation budget exceeded: 96821 > 10000`
(crates/boop-store/src/0_trace_identity.rs:109, from 7c9b54bf). agent_session_observation has 96,829 rows.
Rehearse on a COPY only: `cp /Users/chrishafley/backups/boop/boop-v40-20261001-210341.db <scratch>/` (4.5 GB; delete
after). Determine whether schema 40 changed the join (e.g. observations that were deduped through dict ids now
counted per row) or the budget is simply too small for real data; fix the root cause, not by just raising the limit
unless the measurement shows the join is correct and bounded. Show the sync completing on the copy with
`HOME=<scratch>/home BOOP_DB=<copy>` and timings.

## Gates
`cargo test -p boop-store -p boop-proc --lib` and `cargo test -p boop --bin boop`, all under the sandboxed HOME, green.
Contract suite from /Users/chrishafley/projects/boop2-harmonize (detached at boop2 main):
`BOOP_BIN=<built boop> bash tests/run.sh` 0 not-ok. CLAUDE.md rules; CARGO_BUILD_JOBS=4; no Python.
Commit before reporting done; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
Do not push; do not install boop.
