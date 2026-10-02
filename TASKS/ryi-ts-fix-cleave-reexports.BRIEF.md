# ryi TS fix: cleave demo leaves re-exports in the source file

Repo hafley-rs, crate crates/sprefa-extract (its own workspace root; tests are one target `all`). Base: branch integrate/ryi-ts (6 earlier ryi TS lanes merged).
These tests pass on main b3673b84 and fail on integrate/ryi-ts. Restore them without undoing the TS facts the merged lanes added.

Rules:
- CODE COMPLETE ONLY. Do not run cargo build, cargo test, cargo check, npm, node, or dogfood scripts. The coordinator runs every gate, one at a time. Machine load.
- Read the failing test, the main version of the code (git show main:<path>), and the merged lane diff (git log main..HEAD -- <path>) to find the cause.
- Do not edit a test assertion or golden to match new output unless the new output is a deliberate fact change listed in that lane's plan row; if so, say which row in REPORT.md.
- ryi emits facts only. No SQL analyses, no new CLI commands or flags.
- Other lanes edit the same crate in parallel. Keep diffs inside your area. No drive-by renames or formatting.
- No hand-written files labelled generated. No `boop beep scream`. No push. No Python.
- Commit per cause; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- On done: REPORT.md section per test: cause (file:line, commit that introduced it), change, and the cargo test filter the coordinator should run.

## Failing test
t_156_cleave_play::the_demo_plays_four_cleaves_and_ends_green (tests/156_cleave_play.rs:60). Expected source after four cleaves:
"export const SEP = \"-\";\n". Got the same plus `tidy` still defined and four re-export lines (describe, slug, loadConfig, banner).
Suspects: refactor lane commits for plan rows D5 (source module keeps public exports) and D6 (named re-export keeps remaining exports), branch feature/ryi-ts-refactor.
## Task
D5/D6 add re-exports only where the plan row requires it (an external consumer imports the moved name from the source module). The demo has no such
consumer; restore main's output for it while keeping dogfood/ts/D05.sh and D06.sh expectations. If the two contradict, write the contradiction in REPORT.md and stop.
