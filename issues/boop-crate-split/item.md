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
- [x] no crate runs SQL against another crate's tables by string; `boop-store` exposes typed fns; `boop-proc` does not depend on clap.
- [x] `test_support` becomes `boop-store`'s `testing` feature; every integration test moves with its crate; `tests/temp_home_rail.rs` still covers all of them.
- [x] `cargo-semver-checks` on CI covers each new lib crate.
- [x] `docs/design/boop-process.md` section 3 table updated to the real file list after the move.

## Tests Run

- [x] `cargo test --workspace`: 607 passed, 0 failed, 2 ignored, the same as base `f3d5123`.
- [x] `boop db usage --show-sql`: `diff` against the base const empty.
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

The SQL seam is closed, on Chris's word rather than with the TODO markers the
brief allowed. `summary.rs` moved into `boop-store`, and four typed functions
took the remaining reaches:

| caller | now calls |
|---|---|
| `boop-proc` `concatmap::context_tokens` | `boop_store::Store::context_tokens` |
| `boop` `cli/db.rs` `run_usage` | `boop_store::Store::usage_totals`, printing `boop_store::usage::USAGE_TOTALS_SQL` for `--show-sql` |
| `boop-harness` `harness/codex.rs` test | `boop_store::testing::usage_totals_at` |
| `boop-harness` `harness/kimi.rs` test | `boop_store::testing::usage_totals_at` |

`USAGE_TOTALS_SQL` is one const in `boop-store`, so the SQL `--show-sql` prints
and the SQL `usage_totals` runs cannot drift apart; `boop db usage --show-sql`
is byte-identical to base. `boop-proc` links no clap. No `TODO(crate-seam)`
marker remains. The only SQL text left outside `boop-store` is `#[cfg(test)]`
fixture seeding in `concatmap.rs` and the caller-owned window example printed
in `boop concatmap --help`.

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

### 2026-08-20T14:11:16Z · @claude-lane

Crate seam closed on Chris's word, no TODO markers merged. Four typed boop-store fns took the reaches: Store::context_tokens (concatmap), Store::usage_totals + pub const usage::USAGE_TOTALS_SQL (cli/db.rs run_usage, --show-sql prints the same const the call runs), testing::usage_totals_at (harness codex + kimi ingest tests). run_passthrough_at's printing half split out as emit_named_rows so run_usage prints what a passthrough prints.

Re-measured: cargo test --workspace 607 passed / 0 failed / 2 ignored; cargo clippy --workspace -- -D warnings rc=0 (--all-targets still the one pre-existing host_chat.rs:44); help diff empty across 84 screens; boop db usage --show-sql diffs empty against the base const; grep TODO(crate-seam) over crates/ returns nothing. issuectl ready: 5 of 5.

### 2026-08-20T14:55:04Z · @claude-lane

CI on PR #41: semver PASS, test down to 3 known-red.

Run 32380792479 (head f4a7325) fails exactly 3, a strict subset of what base f3d5123 fails (run 32371247357, 4 failures): codex_spawn_returns_handle_and_stop_tears_down (needs the codex CLI, not installable on the runner) and two worktree tests that are real Linux-only defects (the setup-step deadline never fires, elapsed 999.00216605s; the process group survives the kill). Filed as @worktree-deadline-linux rather than fixed here.

Fixed on this branch, three runner-only gaps that base CI never reached because it dies in the boop lib target after 1016s:
- prune_dry_run_removes_nothing and registry_kinds' prune test depended on an ambient tmux server; each now owns one. Found in one pass with 'env -u TMUX TMUX_TMPDIR=<empty> cargo test --workspace --no-fail-fast': 606/1 before, 607/0 after.
- boop_start_warm and the boop-start worktree tests assert 'just' is on PATH; the test job installs it now.
- semver named four crates absent at baseline-rev; the package list is computed from what git finds at the base sha, so a crate added later joins by itself.


