---
created: 2026-09-18
updated: 2026-09-18
type: feature
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
related: ['@extract-graph-verb']
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
