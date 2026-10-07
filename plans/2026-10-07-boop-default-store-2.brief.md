# Brief: finish the boop default-store fix (base b0686203)

Base is b0686203 "fix(boop): callers holding a mail dir open its store, not the default".
Rule: a function that holds a `Store` or a mail dir never calls `Store::default_path()`.

## Tasks
1. These call sites still call `Store::default_path()`:
   - crates/boop/src/cli/db.rs:52, :69, :1398, :1404, :1744
   - crates/boop-harness/src/click.rs:82
   - crates/boop-store/src/trail.rs:427
   - crates/boop-store/src/lib.rs:89

   For each one: trace its callers. If any caller has a mail dir or store in scope, pass
   it down and use `bus::db_path(dir)` or the held store. If no caller has one, leave the call.
   bus.rs:821 and invoke.rs:258 are the intended no-mail-dir fallbacks; leave them.
2. Delete `invoke::mail_dir_from_argv` (crates/boop/src/invoke.rs). It re-parses argv by hand
   for parse failures. An invocation that clap rejected logs to the default store, as before b0686203.
3. b0686203 edited crates/boop/tests/0_sqlite_contention.rs, session_mood.rs and tell.rs.
   Diff each against 10d58d28. Every assertion that existed before must still assert the same
   claim. Restore any assertion that was removed or weakened.
4. Do not commit plans/.gate-*.log.

## Gate
`KACHE_DISABLED=1 cargo nextest run -p boop -p boop-proc -p boop-store -p boop-harness --no-fail-fast`
run to completion. Copy the final `Summary` line and every FAIL line into the report.

## Landing
Commit on this branch. Do not push.

## Report
One table: site, changed or left, the caller that decided it.
Then the commit hashes and the nextest Summary line.

Rust workspace rule: `[profile.dev.package."*"] opt-level = 3`, `[profile.dev] opt-level = 1`.
