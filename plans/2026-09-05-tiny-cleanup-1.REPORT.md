# Lane report: tiny cleanup batch 1 (hafley-rs)

Note: `REPORT.md` at the worktree root is a committed file from another lane
(soopy issue 018). Left untouched. This is this lane's report.

## Summary

Six items, six commits on `chore/tiny-cleanup-1`. Items 1 and 2 were already true
on this tree and are marked `done` with a file note; items 3-6 were implemented.
Every item committed; every issue `item.md` set to `status: done` (the schema's
closing status for `improvement`; `fixed` is bug-only and rejected by `issuectl`).

## Commits

| item | commit | subject |
|---|---|---|
| 1 temprepo-dedupe | b0d72a2 | boop: mark boop-temprepo-dedupe done |
| 2 rfc3339-parser-dedupe | 9db7d4a | boop: mark boop-rfc3339-parser-dedupe done |
| 3 one-shell-quote | 51381ff | boop-harness: one shell_quote |
| 4 dead-code-allows | de70eb1 | boop: dead_code allows off, per-item reasons |
| 5 registry-into-sqlite | 6dacddd | boop-store: write_route upserts agent_route; registry.json import-only |
| 6 kind-enums | 0c126e7 | boop-store: Message.kind and Route.kind are enums |
| issues statuses | c5419d7 | issues: mark done |

## Items

1. **boop-temprepo-dedupe** - done, no code. One `TempRepo` at
   crates/boop-store/src/testing.rs:12; claude/codex/opencode adapters import
   `boop_store::testing::TempRepo`.
2. **boop-rfc3339-parser-dedupe** - done, no code. Only hand-rolled RFC-3339
   parser is `iso_to_ms` (crates/boop-harness/src/transcript.rs:63, neutral
   module). `boop_store::session::parse_iso_ms` uses the `time` crate
   (`Rfc3339` well-known), not hand-rolled. Grep confirmed no `fn .*rfc3339`,
   `split('T')`, or `parse::<u64>` on timestamp strings; the `parse::<u64>`
   sites are env-var duration strings.
3. **boop-one-shell-quote** - done. `harness.rs::quote` renamed to `pub fn
   shell_quote`; deleted the claude.rs and opencode.rs copies; opencode
   `shell_quote_double` kept. Added table test `quotes_the_edge_cases`.
   boop-mux left alone: its `quote_arg` is a different quoting job (tmux argv,
   conditional double-quoting) and boop-mux has no boop-harness dependency
   (deps: anyhow, tmux_interface, tracing). job.rs:1148 untouched (another lane).
4. **boop-dead-code-allows** - done. Removed module-level `#![allow(dead_code)]`
   from boop-mux/src/lib.rs and harness/{codex,kimi,claude}.rs. Dead items
   deleted: `tmux_command` (boop-mux), `LivePeer`/`PeerKey` (claude.rs).
   Narrow per-item `#[allow(dead_code)]` with reasons kept on claude.rs
   `launch_command` (test-only) and boop-mux test `Sink`/`sink`/`received`
   scaffolding. boop-store/src/event.rs untouched. boop-mux and boop-harness
   clippy `-D warnings` clean.
5. **boop-registry-into-sqlite** - done. `bus::write_route` now opens the store
   and upserts `agent_route` via the existing `upsert_route` (the path
   `import_legacy` used). `import_legacy`/`import_registry_file` retained for
   old mail dirs. Stale module doc at bus.rs:3 updated. `sha256_hex` renamed
   `hash_hex` (it is a `DefaultHasher`, not SHA-256). `cas_update_json` kept:
   `lane-residency.json` and `parent-policy.json` (and boop's own
   `cli::write_route`) still route through it. deliver.rs (:1035, :1127, :1132,
   :1299), control.rs, me.rs callers compile unchanged.
6. **boop-kind-enums** - done. `Message.kind` (`MessageKind`) and `Route.kind`
   (`RouteKind`) at bus.rs are enums with an `Other(String)` fallback so unknown
   on-disk wire values still deserialize and round-trip. Pinned by
   `an_unknown_message_kind_round_trips_through_other` and
   `a_route_with_an_unknown_kind_still_loads`. Compile-fix fallout across
   boop-proc (supervise/inbox/deliver/mailwait), boop-store (runtime.rs,
   tests/mail_contention, tests/wal_three_writers), and boop/src/cli
   (debug/job/mail/mod).

### item 6 notes (deviations to record)

- `lane_state` and `ParentPick.source` left as strings: both live in crates/boop
  (cli/job.rs) and crates/boop-proc/src/lane.rs, which the lane brief says
  another lane owns today. Left for that lane.
- Literal comparison sites (`route.kind == "lane"`, `matches!(as_str(), ...)`)
  were NOT rewritten to explicit `match ... { Kind::X => .. }` blocks. They keep
  working through the enum's `as_str()` and `PartialEq<&str>`/`PartialEq<String>`
  impls, which compare against the wire string and never drop an unknown kind
  (`Other` round-trips). This bounds blast radius across the concurrent-lane
  files rather than rewriting ~30 match sites.

## Validation

`CARGO_TARGET_DIR=$HOME/.cache/cargo-target/tiny-1`

`cargo test --workspace` surfaces only the `boop` integration target in this
repo (the `soopy` member fails to link with a SIGTERM from a cross-project
sign-link linker config, short-circuiting the workspace build), so per-crate
counts are authoritative:

| crate | result |
|---|---|
| boop-store | 146 passed, 1 failed (pre-existing `ident::schema_rows_lists_views_and_join_keys`) |
| boop-proc | 152 passed, 0 failed |
| boop-harness | 165 passed, 1 failed (pre-existing kimi fixture), 1 ignored |
| boop | ~94 passed, 4-5 failed (env) |
| boop-mux | 10 passed |
| boop-acp | 55 passed, 6 ignored |

`cargo clippy -p boop-store -p boop-proc -p boop-harness -p boop-acp -p
boop-mux --all-targets -- -D warnings` -> clean, Finished.

## Pre-existing failures (confirmed at base commit 888df8a, not introduced here)

- `deliver_door::a_route_with_a_live_pane_takes_the_paste_rung` - env (needs a
  live tmux pane); flagged in the lane brief.
- `tell::a_caller_with_no_recorded_parent_falls_back_to_the_one_registered_coordinator`,
  `tell::a_caller_with_no_parent_edge_and_no_registered_coordinator_fails_by_name`,
  `tell::beep_parent_with_no_edge_fails_by_name_instead_of_addressing_the_word` -
  env (live registry state); reproduce at base.
- `harness::kimi::tests::discovers_main_and_a_sub_agent_from_the_fixture` -
  fixture/CWD-dependent; reproduces at base and on a clean tree.
- `ident::tests::schema_rows_lists_views_and_join_keys` - schema view test;
  reproduces at base.
- `boop` clippy `-D warnings` fails on two pre-existing lints: `run_host`
  never used, and `unwrap` on `harness` after `is_some` (cli/job.rs:1270).
  Both present at base; not touched (job.rs is another lane's file).
- `lane_carcass::*` - flaky under parallel `cargo test -p boop` (spawns real
  tmux lanes); pass in isolation.
