# beep fork join and diff bring a fork back

`boop beep fork <comment>` spawned a lane and recorded the link; nothing brought it home. This
change adds the return trip:

```
boop beep fork join  <comment> [--lane <lane>] [--no-merge] [--no-reply] [--dry-run] [--mail-dir <d>]
boop beep fork diff  <comment> [--lane <lane>] [--stat] [--mail-dir <d>]
```

`join` merges `git -C <repo> merge --no-ff <branch>` (refusing a dirty index or a branch with no
commits past the route's `base_sha`), writes the lane's last assistant turn under
`<mail_dir>/forks/comment-<id>.reply.md`, and delivers it to the fork's parent (the route's
`parent`, else the one registered coordinator, else an error). `diff` prints
`git -C <repo> diff <base>..<branch>`.

## clap spelling decision

`boop beep fork <id>` still spawns; no separate `spawn` verb was needed. The `Fork` variant keeps
its existing fields and gains `#[command(subcommand)] cmd: Option<ForkCmd>`, mirroring the `Beep`
variant's exact shape at main.rs:97:

```rust
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
Fork {
    comment: Option<i64>,          // was i64; required by the bare spawn spelling only
    #[command(subcommand)]
    cmd: Option<ForkCmd>,          // None => today's spawn path
    ... existing fields ...
}
```

`comment` had to become `Option<i64>`: clap cannot keep a required positional next to an optional
subcommand. The `Beep` variant already carries the same comment on its `route`/`body` positionals
(main.rs:99-104), so this follows the in-repo precedent. Both spellings parse (`beep fork 7` and
`beep fork join 7`), covered by `beep_fork_bare_spelling_still_parses`.

## Changed files

| File | Change |
|---|---|
| `crates/boop/src/main.rs` | `Fork` variant gains `cmd: Option<ForkCmd>` + the two `#[command]` attrs; `comment` becomes `Option<i64>`; new `ForkCmd` enum (`Join`, `Diff`); Fork primer names join and diff. |
| `crates/boop/src/cli/job.rs` | `run_fork_join`, `run_fork_diff`, `pick_fork`, `fork_parent`, `repo_root`, `ensure_clean`, `ensure_ahead`, `merge_branch`, `git_diff`; `Fork` dispatch arms; nine tests. |
| `crates/boop-store/src/ident.rs` | `Store::last_assistant_turn`. |
| `crates/boop-harness/src/worktree.rs` | `run_git` made `pub`. |

`repo_root` delegates to `lane::repo_root` (boop-proc/lane.rs:158), the same rule `run_fork` uses
for `--cwd`. The reply send reuses `crate::cli::mail::run_send` with `wait: false` and
`as_name: Some(&fork.lane)`, the `boop beep <parent> <body> --no-wait` path.

## Tests

Ten tests in `crates/boop/src/cli/job.rs` `mod tests`: pick one/many/named, join clean, join dirty,
join nothing-past-base, dry run, diff contains the changed path, and the bare-spelling parse test.

## Validation

```
CARGO_TARGET_DIR=$PWD/target cargo test -p boop -p boop-store
```

Unit tests pass: the ten fork tests, `boop-store` (all pass), and the `boop` lib unit tests pass.
Five pre-existing tmux/claude-door integration tests fail in this worktree
(`tell::*` parent-edge, `lane_carcass::*` reclaim, `deliver_door::*` paste rung) with "tmux
new-session reported success but the session is not live" and "no answer from claude-534" — they
require a live claude coordinator door and are unrelated to this change (they fail on the base
commit too).

```
CARGO_TARGET_DIR=$PWD/target cargo clippy -p boop -p boop-store -- -D warnings
```

Fails on two pre-existing diagnostics, both present on the clean base commit and untouched here:

- `debug.rs:186` `run_host` never used (dead code when `dl6` is off).
- `job.rs:1305` `harness.unwrap()` after `harness.is_some()` (`run_agent`, pre-existing).

The fork/join/diff code introduced no new clippy diagnostics (verified by grepping the clippy output
for the new identifiers; none appear).

```
CARGO_TARGET_DIR=$PWD/target cargo run -p boop -- beep fork --help
```

```
Fork a lane off a stored terminal comment: the quoted turns and the note become the brief, the
lane runs on `--preset` from the caller's repo, and the link is kept in `agent_turn_comment_fork`.
The `join` and `diff` verbs bring the fork back

Usage: boop beep fork [OPTIONS] [COMMENT]
       boop beep fork <COMMAND>

Commands:
  join  Merge the fork's branch into the caller's repo and deliver the lane's last assistant turn
        to the fork's parent
  diff  Print `git diff <base>..<branch>` for the fork
  help  Print this message or the help of the given subcommand(s)

Arguments:
  [COMMENT]  `comment_id` in `agent_turn_comment`. Required by the bare spawn spelling `boop beep
             fork <id>`; `join` and `diff` take their own
```

# ryi TS graph lane, 2026-10-02

Coordinator instruction suspended builds and all runtime gates. The initial
release build was terminated locally and remotely before completion. Cases below
are written but unrun; release build, dogfood and crate tests remain unverified.
Corpus: `~/projects/rxjs-corpus-feature-ryi-ts-graph`, detached at d0802620.

| Defect | Before (plan evidence) | After (implementation, unverified) | Layer |
|---|---|---|---|
| D9 | No callers for same-file arrow/function-expression consts | Lexical callable spans resolve g, k and nested inner; D09.sh checks those rows and corpus sectionsOf | Extraction/resolution facts |
| D10 | DOM globals and unrelated declarations bind through corpus_unique | TypeScript call-name fallback has no cross-file corpus-unique leg; plain calls require a lexical callable target or import/checker binding; D10.sh asserts globals, local forward const and parameter shadow | Resolution facts |
| D11 | invalidate binds SignalCreator to Query and enlarges the cycle | Local forward callable is extracted/resolved; phantom edge is absent; D11.sh asserts facts and existing stratify cycle output | Resolution facts |

D10 compatibility risk: previous name-only cross-file TypeScript resolutions
without imports are deliberately declined. Serialized gates must check legacy
fixtures that expected such resolutions. No frozen snapshots were regenerated.

| Defect | Before (plan evidence) | After (implementation, unverified) | Layer |
|---|---|---|---|
| D12 | 0_log → 0_0_log and 10_slice → 2_10_slice | Existing stratify proposals replace numeric/insertion prefixes before adding the depth prefix; D12.sh checks corpus and synthetic proposals | Existing command path proposal |
| D13 | Existing callers command emits 11 rows for 6 sites | Existing callers query chooses one row per full site/target span using SQLite row_number; closure/enclosing mirror facts remain available; D13.sh compares fast/slow rows to six fact sites | Existing callers output shape; facts retained |
| D16 | FILE#render and DiagramRenderCache.render silently return zero | FILE#name filters existing callers rows by canonical file path; Class.method returns an explicit usage error directing FILE#method (allowed by plan expected column); D16.sh asserts both | Existing callers anchor handling |

D16 does not derive class ownership. The callers relation carries callee path,
name and span. Class.method is explicitly rejected instead of reporting an empty
success. FILE#name retains all same-named declarations within the selected file.

| Defect | Before (plan evidence) | After (implementation, unverified) | Layer |
|---|---|---|---|
| D19 | Offset-only closure names remove/re-add unchanged call edges after earlier edits | Anonymous TS callers use named-owner + BLAKE3 closure bytes + duplicate-body ordinal; fast and SCIP slow share formatting; D19.sh asserts corpus delta and synthetic raw identity/diff invariance | Resolution fact identity, consumed by existing diff |

D19 identity scope: unrelated prefix or different-body sibling insertions preserve
identity. Edits inside the closure change its digest. Adding an identical closure
before another identical closure in the same named owner can shift ordinals.
Two existing TS closure-mirror expectations were updated for the declared identity
change; frozen extraction snapshots were not regenerated.

Static review: `git diff --check` and `bash -n` on D09, D10, D11, D12, D13,
D16 and D19 completed. No case bodies were executed. Build, crate test and
runtime correctness remain unverified. D13 selection prefers verified rows
and preserves the existing unresolved grade.

D10 lexical identifier coverage includes both calls and `new` expressions;
explicit local class constructors join their method declaration spans.
