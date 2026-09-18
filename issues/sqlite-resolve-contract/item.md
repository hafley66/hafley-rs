---
created: 2026-09-18
updated: 2026-09-18
type: bug
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
---

# sqlite export: one filename column on unresolved, and a truthful --help

## Description

## Description

Two small defects in the sqlite export, found while testing an unrelated claim on 2026-09-18.

### The `unresolved` table spells its filename two ways

Measured on a 3-file export, 436 rows total:

| column | rows set | rows null |
| --- | --- | --- |
| `_input_path` | 2 | 434 |
| `path` | 434 | 2 |

Perfectly complementary, and **0** rows where both are set and disagree. The 2 rows with a blank `path` are phase-1 intra-file misses (`src/wire.rs:463-470`, `path=None`); the 434 are phase-2 resolve drops (`src/project.rs:2058-2072`, `path=Some`). The merge happens in `flatten_each` (`src/project.rs:288-296`).

Consequence: `SELECT ... FROM unresolved WHERE path = 'src/worktrees.ts'` silently drops 2 rows. Every query needs `COALESCE(path, _input_path)` and nothing says so.

Every row already knows its file. Populate one column on all 436.

### `--help` describes one branch and is silent about the other

`src/bin/extract/help.rs:122-124` says `--resolve` streams "phase-2 records only ... never the per-file phase-1 records". True of the stdout branch (`src/bin/extract.rs:934-936`, `resolve_project_jsonl`). The database branch carries phase 1 and phase 2 both.

Not a mode-ignored bug: `extract fast --sqlite` and `extract --resolve --family call,type --sqlite` produce byte-identical files (sha1 `b83dfccb48598d521bfb0b81c131c32e95f9c209`) because `extract.rs:325` rewrites `fast` into `--family diet_scip` and both spellings reach `resolve_project_with_raw` with the same arms. Dropping `--family` proves the flag is honored: `resolved_type_edge` is 0 versus 37.

## Acceptance Criteria
- [ ] `unresolved` rows all carry their file in one column; `SELECT count(*) FROM unresolved WHERE <that column> IS NULL` returns 0
- [ ] `help.rs:114-124` states that the database branch carries phase 1 and phase 2
- [ ] a test pins the row count parity between the stdout stream and the sqlite table
- [ ] `cargo test --features cli` green

## Tests Run

## Implementation Notes

Independent of every other extract issue. Two files, no design decisions left.
