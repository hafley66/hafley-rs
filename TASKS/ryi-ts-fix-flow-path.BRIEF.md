# ryi TS fix: graph --flow-path from a df param returns its paths (plan row D25, case dogfood/ts/D25.sh)

Repo hafley-rs, crate crates/sprefa-extract (its own workspace root). Base: branch integrate/ryi-ts (all 6 earlier ryi TS lanes merged).
Read: plans/2026-10-01-ryi-ts-utility.md (row named below), crates/sprefa-extract/dogfood/ts/README.md and the case script named below, skill sprefa-extract-add-language.

Rules:
- CODE COMPLETE ONLY. Do not run cargo build, cargo test, cargo check, npm, node, or the dogfood scripts. The coordinator runs every gate, one at a time, after you finish. Machine load.
- ryi emits facts only. No SQL analyses, no graph walkers beyond the existing commands, no new CLI commands or flags.
- Do not edit the case script to make it pass. If the case expectation contradicts the plan row, stop and write that in REPORT.md.
- Other lanes edit the same crate in parallel. Keep diffs inside your area. No drive-by renames or formatting.
- No hand-written files labelled generated. No `boop beep scream`. No push. No Python.
- Commit per change; subject starts with the plan row id; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- On done: REPORT.md section with the failure, cause (file:line), change, and the exact command the coordinator should run to verify.

## Failure on integrate/ryi-ts
In corpus packages/md: the sqlite facts hold node family=df kind=param name=children span 3621..3640 in packages/md/src/lib/1_tableModel.ts, and at least one
df edge from that span. `ryii graph --flow-path packages/md/src/lib/1_tableModel.ts@3621:3640 packages/md` writes empty stdout (same for the blake3 digest spelling).
D25.sh fails at `[ -s path.jsonl ]`.
## Task
Find why the existing --flow-path command emits nothing for a seed that has df edges. Make it emit the existing path records for that seed using
the df edges the facts already hold. No new flags; the PATH@START:END and digest spellings must keep producing identical output.
