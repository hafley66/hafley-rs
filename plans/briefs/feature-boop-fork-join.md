# feature/boop-fork-join

Work in `$PWD` (your worktree). Never `cd` to another checkout.
Set `CARGO_TARGET_DIR=$PWD/target` for every cargo command.

## Goal

`boop beep fork <comment>` (crates/boop/src/cli/job.rs:1401 run_fork) spawns a
lane on branch `fork/comment-<id>` in its own worktree and records the link in
`agent_turn_comment_fork`. Nothing brings the fork back. Add the return trip:

```
boop beep fork join <comment> [--lane <lane>] [--no-merge] [--no-reply] [--dry-run] [--mail-dir <d>]
boop beep fork diff <comment> [--lane <lane>] [--stat] [--mail-dir <d>]
```

`join` does two things, each skippable:
1. merge: `git -C <repo> merge --no-ff <branch>` where repo = the caller's cwd
   repo (same rule run_fork uses for `--cwd`) and branch from the fork row. Refuse
   with a clear message when the repo has a dirty index (`git status --porcelain`
   non-empty) or when the branch has no commits past the route's `base_sha`.
2. reply: read the fork lane's last assistant turn (same query as instant's
   `fork_reply`: latest `agent_turn` row with role assistant and non-empty said
   for the route's `session_id`), write it under `<mail_dir>/forks/comment-<id>.reply.md`
   with a header `# Reply from <lane> to comment <id>` plus the merge result line,
   then deliver it to the fork's parent route through the existing send path
   (`boop beep <parent> <body>` internals, `--no-wait` semantics). Parent = the
   route's `parent` column, else the one registered coordinator, else error.

`diff` prints `git -C <repo> diff <base_sha>..<branch>` (`--stat` adds `--stat`).

## Signatures (Rust)

```rust
// crates/boop/src/main.rs, inside BeepCmd: replace the bare `Fork { .. }` variant
// with a subcommand enum so `boop beep fork <id>` keeps working:
Fork {
    #[command(subcommand)]
    cmd: Option<ForkCmd>,          // None => today's spawn path
    // keep every existing field of Fork as-is for the None case
    ...
},
pub enum ForkCmd {
    Join { comment: i64, #[arg(long)] lane: Option<String>, #[arg(long)] no_merge: bool,
           #[arg(long)] no_reply: bool, #[arg(long)] dry_run: bool, #[arg(long)] mail_dir: Option<PathBuf> },
    Diff { comment: i64, #[arg(long)] lane: Option<String>, #[arg(long)] stat: bool,
           #[arg(long)] mail_dir: Option<PathBuf> },
}
// If clap cannot mix a positional `comment` with an optional subcommand, make
// the spawn form `boop beep fork spawn <id>` AND keep `boop beep fork <id>`
// working via `#[command(args_conflicts_with_subcommands = true)]`; document
// which you chose in REPORT.md and test both spellings.

// crates/boop/src/cli/job.rs
pub(crate) fn run_fork_join(registry: &Registry, comment_id: i64, lane: Option<String>,
    no_merge: bool, no_reply: bool, dry_run: bool, mail_dir_arg: Option<&Path>) -> Result<()>
// fork = pick_fork(store, comment_id, lane)?            // one row or error listing lanes
// route = store route for fork.lane (base_sha, session_id, parent, cwd)
// repo = current repo root (git rev-parse --show-toplevel of std::env::current_dir)
// if !no_merge { ensure_clean(repo)?; ensure_ahead(repo, base_sha, branch)?; git merge --no-ff }
// if !no_reply { reply = last_assistant_turn(store, session_id)?; write file; send to parent }
// println one line per step; dry_run prints the git command and the recipient, runs nothing

pub(crate) fn run_fork_diff(comment_id: i64, lane: Option<String>, stat: bool, mail_dir_arg: Option<&Path>) -> Result<()>

fn pick_fork(store: &Store, comment_id: i64, lane: Option<&str>) -> Result<TurnCommentFork>
// forks = store.turn_comment_forks(comment_id)?; 0 => bail "no lane forked off comment N";
// 1 => it; many => require --lane, bail listing lane names
```

Reuse: `git_head` (job.rs:258) for the git spawn pattern, `run_git` in
boop-harness `worktree.rs:302` if it is reachable (make it `pub` if not),
`store.turn_comment_forks` (boop-store ident.rs:1704). Add to boop-store:

```rust
pub fn last_assistant_turn(&self, session: &str) -> Result<Option<(i64, String)>>
// SELECT t.turn, t.said FROM agent_turn t JOIN dict_session ds ON ds.id=t.session_id
//   JOIN dict_role r ON r.id=t.role_id WHERE ds.value=?1 AND r.value='assistant'
//   AND t.said IS NOT NULL AND t.said!='' ORDER BY t.turn DESC LIMIT 1
```

Update the `boop --help` primer text for Fork (main.rs:855) to name join and diff.

## Tests (cargo, in job.rs `mod tests` next to `a_fork_brief_carries_the_ask...`)

| case | input | expected | why |
| --- | --- | --- | --- |
| pick one | one fork row | that row | default path |
| pick many | two rows, no --lane | Err lists both lane names | ambiguity must be explicit |
| pick named | two rows, --lane second | second | override works |
| join clean | temp repo, branch with 1 commit past base | merge commit exists, reply file written | the whole trip |
| join dirty | temp repo with unstaged change | Err mentions dirty, no merge | never merge over work |
| join nothing | branch == base | Err mentions no commits, no merge | nothing to join |
| dry run | --dry-run | prints git cmd and recipient, repo HEAD unchanged | inspectable |
| diff | branch with 1 commit | output contains the changed path | diff verb |
| help spelling | `boop beep fork 7` still parses | Ok | back-compat |

## Validation (paste tails into REPORT.md)

```
CARGO_TARGET_DIR=$PWD/target cargo test -p boop -p boop-store
CARGO_TARGET_DIR=$PWD/target cargo clippy -p boop -p boop-store -- -D warnings
CARGO_TARGET_DIR=$PWD/target cargo run -p boop -- beep fork --help
```

## Ownership

You own: `crates/boop/src/main.rs` (Fork variant + ForkCmd + help text only),
`crates/boop/src/cli/job.rs`, `crates/boop-store/src/ident.rs`
(last_assistant_turn only), `crates/boop-harness/src/worktree.rs` (visibility
of run_git only), `REPORT.md`. Nothing else.

## Style laws

No em dashes. No words `provenance`, `substrate`, `load-bearing`, `regime` in
prose or identifiers. `///` doc comments in the file's voice. Follow each file's
existing style. Commit subject exactly: `boop: beep fork join and diff bring a fork back`
