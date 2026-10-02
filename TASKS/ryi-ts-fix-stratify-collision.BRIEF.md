# ryi TS fix: stratify renumbering keeps file stems and emits one move per file (plan row D12, case dogfood/ts/D12.sh)

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
`ryii stratify packages/signals/src --from packages/signals/src/index.ts` on the corpus emits 40 stratify_move records. Defects:
- packages/signals/src/4_jsxAuto.test.ts -> packages/signals/src/4_jsxAuto.ts (test file renamed onto the source file; stem "jsxAuto.test" lost ".test")
- same from_path twice with different to_path: 0_log.ts x2, 2_Signal.ts x2, 3_Endpoint.ts x2, 8_sync.ts -> 1_sync.ts and -> 8_sync.ts, 9_history.ts -> 4_history.ts and -> 9_history.ts
## Task
Every stratify_move keeps the full stem after the numeric prefix (including .test, .browser.test, .memory etc), and each from_path appears in at most one
stratify_move. No two to_path values collide with each other or with an unmoved file. D12.sh's jq assertions define the stem rule.
