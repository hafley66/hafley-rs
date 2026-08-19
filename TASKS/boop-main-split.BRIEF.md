# boop-main-split

`crates/boop/src/main.rs` (7237 lines, 52 inline tests) becomes a clap tree
plus `main()`; everything else moves into `crates/boop/src/cli/*.rs`. Pure
move. Zero behavior change. Card: `issues/boop-main-split/item.md`, epic
`boop-process`, design `docs/design/boop-process.md` section 4 row 1.

## Base
- worktree branch `refactor/boop-main-split`; first action
  `git merge --ff-only b57f250` (origin/main). Failure = STOP.

## Own
- `crates/boop/src/main.rs`, new `crates/boop/src/cli/**`, new
  `crates/boop/tests/cli_*.rs`, `crates/boop/src/lib.rs` only if a `pub use`
  is needed for the moved tests.
- Forbidden: every other file under `crates/boop/src/` (channel, harness,
  store, worktree, supervise, ...), `Cargo.toml`, docs. A move that needs a
  change there = STOP and report the line.

## Target layout (by namespace, section markers in main.rs name the seams)
| file | takes main.rs sections |
|---|---|
| `cli/mod.rs` | module list, shared output helpers ("The verb output helpers" :1486) |
| `cli/job.rs` | `beep lane*`, lane (:2573), dispatch (:1670), sweep (:2450), wait (:2269), resolve (:1875), measure (:1629), pstree (:6286) |
| `cli/mail.rs` | hail (:1947), inbox, message, hook drain, tell-parent/children |
| `cli/me.rs` | whoami, me *, adopt / prune (:2892) |
| `cli/db.rs` | db tree (:6551), query, usage, price, fact, session, turn |
| `cli/debug.rs` | debug tree, host chat, config |
| `main.rs` | `Cli`, `SubCmd` and the sub-enums, `main()`, dispatch match arms calling `cli::*` |
If a section straddles two namespaces, put it where its clap enum lives and
say so in the report. Do not rename any verb, flag, help string, or enum
variant; that is the later `boop-job-namespace` card.

## Tests
- The 52 `#[test]` fns in `mod tests` (:3148) move to `crates/boop/tests/cli_<namespace>.rs`
  matching the code they test; tests that need private items stay as
  `#[cfg(test)] mod tests` at the bottom of the new `cli/*.rs` file. Count
  before = count after; paste both `cargo test -p boop 2>&1 | grep 'test result'`
  blocks.

## Gates (run all, paste verbatim in the REPORT)
1. Before any edit: `for v in "" beep "beep lane" "beep lane create" db debug host me config whoami; do boop $v --help; done > /tmp/help-before.txt` using the WORKTREE binary (`cargo run -q -p boop -- ...`), one file per verb under `TASKS/help-before/`. After: same into `TASKS/help-after/`; `diff -r` must be empty. Commit neither directory; paste the diff command and its empty output.
2. `cargo test -p boop` same pass count as base (base: 461 passed across targets), `cargo clippy -p boop -- -D warnings` rc=0.
3. `wc -l crates/boop/src/main.rs` after; target under 1500. Paste.
4. `cargo semver-checks` is on CI; a bin crate has no public API, but if `lib.rs` changed, run `cargo semver-checks -p boop` and paste.
5. No `eprintln!` added; `grep -rn 'eprintln!' crates/boop/src/cli` pasted.

## Style
Comment budget: move comments with their code, add none. No em dashes. No
words provenance/substrate/load-bearing/regime as identifiers or prose.

## Report
`TASKS/boop-main-split.REPORT.md`: file table (new file, line count, which
sections), gate outputs, commit shas, PR url. One commit per namespace file
move, then one for tests, then one for the report. Push, open the PR, do not
merge.
