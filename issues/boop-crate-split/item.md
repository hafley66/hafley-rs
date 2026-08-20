---
created: 2026-08-19
updated: 2026-08-20
type: task
status: testing
priority: high
epic: boop-process
size: L
blocked_by: ['@boop-main-split']
---

# Split boop into boop-store / boop-harness / boop-mail / boop-proc / boop-cli

## Description

## Description
Split `crates/boop` (33363 lines) into `boop-store`, `boop-harness`, `boop-mail`, `boop-proc`, `boop-cli` per `docs/design/boop-process.md` section 3. One PR per crate extraction in dependency order: store, harness, mail, proc, cli. Zero behavior change; `boop --help` per verb byte-identical before and after (pinned test); `cargo test` wall time unchanged or better.
## Acceptance Criteria
- [x] workspace has the five crates; `crates/boop` is gone or is the bin crate only.
- [ ] no crate runs SQL against another crate's tables by string; `boop-store` exposes typed fns; `boop-proc` does not depend on clap.
- [x] `test_support` becomes `boop-store`'s `testing` feature; every integration test moves with its crate; `tests/temp_home_rail.rs` still covers all of them.
- [x] `cargo-semver-checks` on CI covers each new lib crate.
- [x] `docs/design/boop-process.md` section 3 table updated to the real file list after the move.

## Tests Run

- [x] `cargo test --workspace`: 607 passed, 0 failed, 2 ignored, the same as base `f3d5123`.
- [x] `cargo clippy --workspace -- -D warnings`: rc=0.
- [x] 84 `--help` screens, base binary against branch binary: `diff` empty.
- [x] `cargo test -p boop --test temp_home_rail`: 2 passed, now walking every `boop*` crate.

## Implementation Notes

Landed as five crates on one branch, one commit per extraction:
`boop-store` (12330 src lines), `boop-acp` (2786), `boop-harness` (5232),
`boop-proc` (5421), `boop` the bin plus a facade lib (8336).

Three things differ from the card as written:

1. The card says `boop-mail`; `docs/design/boop-process.md` section 7 renames it
   `boop-acp`, and section 7 is what landed. `bus.rs`, `event.rs` and `trail.rs`
   went to `boop-store` (the runtime snapshot and session graph read them);
   `inbox.rs` and `mailwait.rs` went to `boop-proc`.
2. Dependency order is store -> acp -> harness -> proc -> cli, not
   store -> harness -> acp -> proc. `Harness::open_channel` returns a
   `Box<dyn LaneChannel>` and every adapter constructs its own channel, so the
   channel crate is below the adapters.
3. The fifth crate is still packaged `boop`, because the binary, the lib every
   caller links, and the install recipe are all named `boop`. It is the bin
   crate plus a facade lib that re-exports the four libraries at their old
   paths, which is what keeps the behavior change at zero.

The SQL-seam criterion is left unchecked: `summary.rs` moved into `boop-store`
so its three raw queries are in-crate, but two production sites still name
another crate's tables by string and are marked `// TODO(crate-seam):` rather
than redesigned, per the brief:
`crates/boop-proc/src/concatmap.rs` `context_tokens` (`agent_usage`,
`dict_session`) and `crates/boop/src/cli/db.rs` `USAGE_TOTALS_SQL`
(`agent_usage`, `model_price`, printed verbatim by `--show-sql`). Two more are
in `#[cfg(test)]` blocks of `crates/boop-harness/src/harness/{codex,kimi}.rs`.
`boop-proc` links no clap.

## Comments

### 2026-08-20T13:49:05Z · @claude-lane

PR https://github.com/hafley66/hafley-rs/pull/41 (do not merge yet).

Five crates on one branch, one commit per extraction: boop-store, boop-acp, boop-harness, boop-proc, boop (bin + facade lib). Order landed store -> acp -> harness -> proc -> cli, not store -> harness -> acp -> proc: Harness::open_channel returns a Box<dyn LaneChannel> and every adapter constructs its own channel, so the channel crate is below the adapters. The one 8-module SCC at f3d5123 (channel, harness, ident, identity, lane, registry, supervise, worktree) broke through nine code moves, each re-exported at its old path.

Gates:
- cargo test --workspace: base f3d5123 607 passed / 0 failed / 2 ignored; branch identical. The 4 tests the brief named as known-red all passed on base here.
- boop --help, 84 screens: diff of base binary against branch binary empty, 1385 lines each.
- cargo clippy --workspace -- -D warnings: rc=0. --all-targets is rc=101 on base AND branch on the same pre-existing crates/boop/tests/host_chat.rs:44 needless_borrow.
- cargo semver-checks: not installed on this machine; .github/workflows/ci.yml:34 package list widened to the four new crates.
- tests/temp_home_rail.rs now walks every boop* crate's src/ and tests/.

Receipts: TASKS/boop-crate-split.REPORT.md.
