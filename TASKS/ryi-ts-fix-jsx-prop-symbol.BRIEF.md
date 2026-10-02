# ryi TS fix: slow JSX prop symbol names the props type member (plan JSX row J01, case dogfood/ts/J01.sh)

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
`assert.ok(refs[0].symbol.includes('#FooProps.bar@'))` false in J01.sh. The slow tier (tsgo) binding for attribute bar in <Foo bar={x}/>
does not carry the FooProps.bar member symbol.
## Task
Read J01.sh and its fixture, read the slow JSX binding code added by commit 4718ffbb ("J01: emit tsgo bindings for JSX components and props members").
Make the emitted attribute binding symbol name the props type member in the format J01.sh asserts. Fix the symbol in the emitted fact; do not loosen J01.sh.
