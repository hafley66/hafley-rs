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
