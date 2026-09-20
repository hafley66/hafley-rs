---
created: 2026-09-18
updated: 2026-09-19
type: feature
status: done
priority: normal
epic: extract-parity-move-rename
labels: [extract, phase-refinement-1, artifact-cli]
related: ['@extract-graph-verb']
closed: 2026-09-19
closed_by: claude-opus-5
---

# extract --lines: byte spans as file:line, every verb

## Description

## Description

Every span extract emits is a byte offset. `worktrees.ts:43256` is byte 43256 of a 58252-byte file, which is **line 1042**, `export function sessionsForWorktree`. Nothing a human can click, and `vim +43256` lands on line 43256 of a 1400-line file.

This blocks hand-use of every verb, not just `graph`. It is the single largest usability win per line of code in the crate.

`src/lang/ts.rs:2832` states the extractor law and it stays: "a node is a byte Span, never a line." Lines are a sink-side decoration, never an extractor concern.

## Two attach points, one per sink

| sink | attach point | mechanism |
| --- | --- | --- |
| facts | `src/wire.rs:580-593`, `file_fact_with_content_id` | content is in hand and it already counts newlines at `:585`. Emit a sibling `line_start` row: path plus the `[u32]` newline offsets. Needs a TypeSpec record in `schema/1_facts.tsp` so the generated writers (`src/bin/extract/0_sqlite.rs:12-20`) get a table. |
| stdout | `src/bin/extract/0_sqlite.rs:183-215`, `Output::line` / `Output::fact` | the single funnel both branches write through. Decoration goes in the stdout arm; the database arm gets a SQL view over `line_start`. |

`src/lang/*` is not touched by this issue.

## Acceptance Criteria
- [ ] `line_start` record in `schema/1_facts.tsp`, one row per file, newline byte offsets
- [ ] `--lines` adds `line` and `col` beside `start`/`end` on stdout for every verb
- [ ] sqlite export carries a view joining any span table to `line_start`
- [ ] `extract --lines src/worktrees.ts` reports line 1042 for byte 43256
- [ ] `src/lang/` diff is 0 lines
- [ ] `cargo test --features cli` green

## Tests Run

## Implementation Notes

Ships before `extract-graph-verb`. The graph verb's output is unreadable without it.

## Comments

### 2026-09-18T22:35:43Z · @claude-513

Receipt from the 2026-09-18 ryi rename. rename exited 6 citing 'src/bin/extract.rs byte 1905' and 'tests/0_sqlite.rs byte 56'. Resolving those with head -c | wc -l gave src/bin/extract.rs:40 and tests/0_sqlite.rs:4, two extra shell steps before the diagnostic could be read. See @rename-path-double-reach.

### 2026-09-18T23:48:33Z · @claude-513

Checked whether --lines already landed in a worktree. It did not. `git log --all -S'"--lines"' -- crates/sprefa-extract` returns zero commits, and no branch matches *line* except origin/fix/mux-multiline-brief. Four candidate worktrees inspected: extract-fixes ad865db5 is the diff-verb report, extract-check and extract-oracle-paths-test both sit on 3ee65276 which is a boop cleanup record, extract-post-move-fix 11d09f93 gates the cli identity test. The worktree the 20260918.1 session log labeled 'lane LN --lines' (commit 82978233, now 4bd8cf2f on main) carries schema/1_facts.tsp, the three generated schema files, 0_sqlite.rs and wire.rs: sqlite-side work with no CLI flag. That lane died on 'Prompt is too long' and produced no --lines work.

### 2026-09-19T20:28:27Z · @claude-opus-5

Landed on main at 252a7346 via lane feature-extract-lines-flag (preset glm53f-omp). Gate: cargo test --features cli --no-fail-fast, every target ok, 0 failures. src/lang/ diff empty three-dot against the merge base. Receipt: head -c 43256 instant/src/worktrees.ts | wc -l = 1041, and ryi --lines reports span start 43256 line 1042 col 8 on function_declaration sessionsForWorktree. Decoration is per span by owning path: caller_site against caller_path, callee against callee_path, resolved_type_edge owner/target likewise. Named stops: flow_edge and projectedge own spans by content digest (from_blob/to_blob) with no path in the row, and scip_signature_occurrence offsets index signature text rather than a file.


