# Lane SQ: one filename column on `unresolved`, and a truthful `--help`

Issue: `issues/sqlite-resolve-contract/item.md` (`issuectl show sqlite-resolve-contract`). Epic `extract-parity-move-rename`.

You work in `$PWD`, your own worktree. Never `cd` anywhere outside it. The crate is `crates/sprefa-extract`, its own cargo workspace.

## Setup

- `export CARGO_TARGET_DIR=$PWD/crates/sprefa-extract/target-laned` before any cargo command.
- Run only ONE cargo process at a time. Lane LN is concurrent with you and holds the same rule.
- Gate: `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast`. Green at your base sha except the 2 pre-existing `tests/golden_parity.rs` failures (`ported_facets_match_v5`, `rust_doc_parity`), a stale oracle root prefix, NOT yours.
- Read `crates/sprefa-extract/AGENTS.md` first. Read nothing else until a row below names it.

## Defect 1: `unresolved` spells its filename two ways

Measured 2026-09-18 on a 3-file export (`extract fast --sqlite f.db src/0_boopGraph.ts src/worktrees.ts src/main.ts`, run from `~/projects/instant`), 436 rows:

| column | rows set | rows null |
| --- | --- | --- |
| `_input_path` | 2 | 434 |
| `path` | 434 | 2 |

Perfectly complementary. `SELECT count(*) FROM unresolved WHERE path IS NOT NULL AND _input_path IS NOT NULL AND path <> _input_path` returns **0**. They never disagree.

The two shapes:

| shape | emitter | `path` |
| --- | --- | --- |
| phase 1, per-file intra-file miss | `src/wire.rs:463-470`, `FlatFact::Unresolved { path: None, .. }` | `None` |
| phase 2, `--resolve` drop | `src/project.rs:2066`, `.map(\|drop\| FlatFact::Unresolved { .. })` | `Some(input.path)` |

They merge in the database through `flatten_each` (`src/project.rs:288-296`).

The 2 phase-1 rows, verbatim from that export:

```
_input_path       path  reason            detail
src/worktrees.ts        spread-call-args  ...agentMenuItems(c.clonePath!, branch, c.clonePath!, false)
src/main.ts             spread-call-args  ...args
```

Consequence: `SELECT ... FROM unresolved WHERE path = 'src/worktrees.ts'` silently drops a row that is in the table and does know its file. Every consumer needs `COALESCE(path, _input_path)` and nothing tells them.

**Fix:** every row carries its file in ONE column. Populate `path` on the phase-1 rows from the file being flattened. Do not add a column, do not split the table, do not solve it in documentation.

`src/wire.rs` is lane LN's file for `file_fact_with_content_id` and one new sibling fn only. The `Unresolved` push at `:463-470` is yours. Coordinate by touching only those lines; if the stamping has to happen at the flatten site instead, do it in `src/project.rs` and leave `wire.rs` alone entirely. That is the preferred shape.

## Defect 2: `--help` describes one branch and is silent about the other

`src/bin/extract/help.rs:123-126`:

> The stream carries phase-2 records only (resolved_edge, resolved_type_edge,
> flow_edge), never the per-file phase-1 records `--schema` also lists (node,
> edge, sig, site, specifier, unresolved and the rest).

True of the stdout branch (`src/bin/extract.rs:934-936`, `resolve_project_jsonl`). The database branch carries phase 1 and phase 2 both.

This is NOT a mode-ignored bug and you must not "fix" it by changing behavior. Proof, measured 2026-09-18:

```
extract fast --sqlite f.db  A B C                       -> 45915 rows, sha1 b83dfccb48598d521bfb0b81c131c32e95f9c209
extract --resolve --family call,type --sqlite r.db A B C -> 45915 rows, sha1 b83dfccb48598d521bfb0b81c131c32e95f9c209
```

Byte-identical because `src/bin/extract.rs:325` rewrites `fast` into `--family diet_scip`, and both spellings reach `resolve_project_with_raw` with arms call+types (`src/project.rs:1277-1288` and `src/bin/extract.rs:911-932`). The flag IS honored, proven by dropping `--family`:

```
extract --resolve --sqlite a.db A B C   -> resolved_edge 177, resolved_type_edge  0
extract fast      --sqlite f.db A B C   -> resolved_edge 177, resolved_type_edge 37
```

**Fix:** amend `help.rs:114-126` so the database branch is described. Behavior does not move.

## Files you own

- `crates/sprefa-extract/src/project.rs`
- `crates/sprefa-extract/src/bin/extract/help.rs`
- `crates/sprefa-extract/src/wire.rs` lines 463-470 ONLY, and only if the flatten-site fix proves impossible
- `crates/sprefa-extract/tests/` (one new test file, name it `141_unresolved_contract.rs`)

Not yours, lane LN is live on them: `schema/1_facts.tsp`, `src/bin/extract/0_sqlite.rs`, `src/bin/extract.rs`, everything under `src/lang/`.

Lane LN will hand you help text for a `--lines` flag in `TASKS/lane-lines-flag.REPORT.md` under `## help.rs text for the coordinator`. If that file exists when you start, paste its section into `help.rs`. If it does not, skip it and say so in your report; the coordinator will land it.

## Verification, exact

```sh
cd crates/sprefa-extract
export CARGO_TARGET_DIR=$PWD/target-laned
cargo test --features cli --no-fail-fast

# rebuild the measured export, from ~/projects/instant
rm -f /tmp/sq.db
$PWD/target-laned/debug/extract fast --sqlite /tmp/sq.db \
  ~/projects/instant/src/0_boopGraph.ts ~/projects/instant/src/worktrees.ts ~/projects/instant/src/main.ts

sqlite3 /tmp/sq.db "SELECT count(*) FROM unresolved;"                    # 436
sqlite3 /tmp/sq.db "SELECT count(*) FROM unresolved WHERE path IS NULL;" # must be 0
sqlite3 /tmp/sq.db "SELECT path, reason FROM unresolved WHERE reason='spread-call-args';"
# both rows must now name src/worktrees.ts and src/main.ts in `path`

# behavior must not move
rm -f /tmp/a.db /tmp/f2.db
$PWD/target-laned/debug/extract --resolve --sqlite /tmp/a.db  <same 3 files>
$PWD/target-laned/debug/extract fast      --sqlite /tmp/f2.db <same 3 files>
sqlite3 /tmp/a.db  "SELECT count(*) FROM resolved_type_edge;"   # 0
sqlite3 /tmp/f2.db "SELECT count(*) FROM resolved_type_edge;"   # 37
```

## Tests

Integration through the real binary, per the standing law. No unit fakes for IO or sqlite. `tests/141_unresolved_contract.rs`:

| case | input | expected | why this case exists |
| --- | --- | --- | --- |
| no null filename | a 2+ file export with at least one phase-1 miss | `count(*) WHERE path IS NULL` = 0 | the defect, pinned |
| stream and table agree | same files, stdout `--resolve` vs the sqlite table | the phase-2 row counts match exactly | today they differ by 2 and nothing catches it |
| phase 1 rows keep their reason and span | a fixture with a spread call arg | `reason='spread-call-args'`, span preserved | proves the stamp added a path and changed nothing else |
| mode flag still honored | `--resolve --sqlite` vs `fast --sqlite` | `resolved_type_edge` 0 vs 37 | pins the behavior a future reader might "fix" after misreading help.rs |

## Deliverable

`TASKS/lane-sqlite-contract.REPORT.md` with the verification outputs verbatim and one line per defect. Commit with `Refs-Issue: @sqlite-resolve-contract`.

## Style laws (apply to code comments, commit messages and the report)

- No em dashes. No sycophancy. No negative parallelism (`not X, Y` / `this isn't X. it's Y`).
- No deictic filler: never `here is`, `here's`, `below is`, `the following`. Point with file:line.
- Banned in prose AND identifiers: `provenance` (use source/origin), `substrate` (base layer), `load-bearing` (critical), `regime` (mode), `ground` as a verb (verified/checked-against), `ruling` (decision), `honest(ly)` (state the fact), `distill` (shrink/condense).
- Output is lists and tables. Prose is a one-line caption, never the medium.
- Short sentences. Active voice. Real paths, real numbers.
