# Queue lane rules (every queue lane)

You are one agent with one branch and one worktree ($PWD). Keep both for the
whole queue. Never create another worktree or branch.

## Loop, one issue at a time
1. `git merge main` (main moves; take it before each issue).
2. Read `issues/<name>/item.md`. Reproduce with the release or debug `ryii`
   (`ryii` = in-process CLI; `ryi` = daemon client, same argv).
3. Fix the code. A test proves the fix: add or extend one in the matching
   `crates/sprefa-extract/tests/*.rs` (every file is a `#[path]` module in
   `tests/all.rs`; add the line for a new file).
4. Run only the targets you touched:
   `cargo nextest run --features cli -j 2 --test all -E 'test(/^t_<file_stem>::/)'`
   in `crates/sprefa-extract`, or `cargo nextest run -p <crate> -j 2 -E ...`.
   Never the whole suite. Never CodeQL or `scripts/ryi-vs-codeql.sh`.
5. Set the issue `status: fixed` with a one-line receipt (commit, test name).
6. `cargo fmt` the files you edited. Never format `tests/fixtures/**` or
   `**/gen/**` (goldens read fixture byte offsets; TypeSpec owns gen/).
7. Commit, one commit per issue. Body lines `area: before -> after`.
8. Next issue.

An issue that needs a design decision from the user, or is an epic (more
than ~1 day): set `status: needs-decision` with the question in one line,
commit, move on.

## Environment
- `CARGO_TARGET_DIR=$HOME/.cache/lanes/$BOOP_LANE/target` (yours, persistent).
- `-j 2` for every cargo command. The laptop overheats.
- No test may leave a `ryii --daemon` process running.
- Goldens: update only with the added/changed rows listed in the commit body
  and why each is correct against the fixture source.
