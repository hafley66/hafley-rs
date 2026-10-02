# ryi TS fix: workspace package imports resolve (plan row D2, case dogfood/ts/D02.sh)

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
`D2: 12/557 unresolved pairs`. Shown pairs (fast=false sqlite=false, "export target did not reach a corpus source file"):
- packages/json-rx/examples/4_cross_process_frame/1_generate.ts -> @hafley66/alloy-rs/emitter
- packages/md/src/0b_SequenceDiagram.tsx -> @hafley66/grapht-render-cytoscape
- packages/md/src/2_MarkdownTable.tsx, plugins/1_FsTreeFence.tsx -> @hafley66/signal-grid/theme.css, tree.css
- plugins/1_MarblesFence.tsx -> @hafley66/signal-marbles/marbles.css
- vitest-telemetry/src/report-app/main.tsx -> @hafley66/report-shell/marbler.css, style.css
## Task
Find all 12 (corpus: hafley-rxjs at d0802620, read-only via git show; do not create worktrees in it). For each, decide from the
plan row whether it must resolve (package exists in the corpus, export map or css asset) or is out of corpus. Make ryi emit the
resolution fact for the ones that exist (css assets included if the package export map points at a file). For packages absent from
the corpus, emit an explicit unresolved/external fact. If D02.sh counts external packages as failures against the plan row, report it; do not edit D02.sh.
