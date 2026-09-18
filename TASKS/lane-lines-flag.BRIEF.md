# Lane LN: `--lines`, byte spans rendered as file:line for every verb

Issue: `issues/extract-lines-flag/item.md` (`issuectl show extract-lines-flag`). Epic `extract-parity-move-rename`.

You work in `$PWD`, your own worktree. Never `cd` anywhere outside it. The crate is `crates/sprefa-extract`, its own cargo workspace.

## Setup

- `export CARGO_TARGET_DIR=$PWD/crates/sprefa-extract/target-laned` before any cargo command.
- Run only ONE cargo process at a time. Lane SQ is concurrent with you and holds the same rule.
- Gate: `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast`. Green at your base sha except the 2 pre-existing `tests/golden_parity.rs` failures (`ported_facets_match_v5`, `rust_doc_parity`), which are a stale oracle root prefix and NOT yours.
- Read `crates/sprefa-extract/AGENTS.md` first. Read nothing else until a row below names it.

## The defect

Every span extract emits is a byte offset into the file. Nothing renders a line number.

```
$ extract --resolve --family type src/worktrees.ts ...
sessionsForWorktree   returns   Session   worktrees.ts:43256
```

`src/worktrees.ts` in `~/projects/instant` is 58252 bytes. Byte 43256 is **line 1042**, which holds `export function sessionsForWorktree`. A reader cannot click it and `vim +43256` lands on line 43256 of a 1400-line file.

## The law that stays

`src/lang/ts.rs:2832`:

> `line_at` / `line_index` / `line_col`: a node is a byte Span, never a line.

Lines are a sink-side decoration. **Your diff under `src/lang/` must be 0 lines.** That is a gate, verified with `git diff --stat -- crates/sprefa-extract/src/lang | wc -l`.

## Files you own

- `crates/sprefa-extract/schema/1_facts.tsp` (the new record only)
- `crates/sprefa-extract/src/wire.rs` (`file_fact_with_content_id` and one new sibling fn)
- `crates/sprefa-extract/src/bin/extract/0_sqlite.rs` (`Output::line` and the DDL)
- `crates/sprefa-extract/src/bin/extract.rs` (the `--lines` clap flag ONLY, nothing else in this file)
- `crates/sprefa-extract/tests/` (one new test file, name it `140_lines_flag.rs`)

Not yours, and lane SQ is live on two of them: `src/project.rs`, `src/bin/extract/help.rs`, everything under `src/lang/`, `tests/RATCHET.tsv`, every existing test file.

Help text for `--lines` is lane SQ's file. Write the text you want into your REPORT.md under a `## help.rs text for the coordinator` heading and do NOT edit `help.rs`.

## The rows

| # | what | receipt | do |
| --- | --- | --- | --- |
| 1 | new record `line_start` | `schema/1_facts.tsp`; generated writers land in `schema/generated/7_writers_auto.rs` and give `src/bin/extract/0_sqlite.rs:12-20` a table for free | one row per file: `path=<string>`, `digest=<hex>`, `offsets=<u32[]>` holding every newline byte offset in order. JSON array column in sqlite, same treatment the other array fields get. |
| 2 | emit it | `src/wire.rs:580-593` `file_fact_with_content_id` already walks the content and counts newlines at `:585` (`content.iter().filter(\|byte\| **byte == b'\n').count()`) | add `line_start_fact_with_content_id(path, content, content_id) -> FlatFact` beside it, collecting the offsets in the same single pass. Do not add a second walk of the content. Emit it wherever `--file-fact` emits the `file` row, gated on `--lines`. |
| 3 | stdout decoration | `src/bin/extract/0_sqlite.rs:183-191` `Output::line` is the single funnel: `if let Some(db) = &mut self.database { ... }` then the stdout write | decorate in the **stdout arm only**. For every JSON object carrying `start`/`end` (including `span__start` shapes and the `from`/`to` edge pairs), add sibling `line`/`col` fields derived by binary search over the current file's offsets. The database arm is untouched by this row. |
| 4 | sqlite side | same file, the DDL the export writes | add a view `span_lines` joining any table with `_content_id` + a span column to `line_start`. One view, documented in the row the export prints on success. |
| 5 | the flag | `src/bin/extract.rs`, clap | `--lines`, no short form, default off. It conflicts with nothing. Adding it must not change any existing default. |

`col` is 1-based characters from the line start, counting UTF-8 scalar values, not bytes. `line` is 1-based.

## Verification, exact

```sh
cd crates/sprefa-extract
export CARGO_TARGET_DIR=$PWD/target-laned

# 1. the law
git diff --stat -- src/lang | wc -l          # must print 0

# 2. the gate
cargo test --features cli --no-fail-fast

# 3. the measured case, run from ~/projects/instant with your built binary
$PWD/target-laned/debug/extract --lines --file-fact ~/projects/instant/src/worktrees.ts \
  | jq -r 'select(.record=="line_start") | .offsets | length'
# 1400-ish, and the offset array's 1041st entry must be <= 43256 < 1042nd

# 4. default unchanged
$PWD/target-laned/debug/extract ~/projects/instant/src/main.ts | wc -l    # 10853
```

## Tests

Integration through the real binary, per the standing law. No unit fakes for IO. `tests/140_lines_flag.rs`:

| case | input | expected | why this case exists |
| --- | --- | --- | --- |
| byte 43256 is line 1042 | a fixture with known newline positions | `line=1042` | the motivating defect, pinned |
| line 1 byte 0 | any fixture | `line=1, col=1` | off-by-one at the boundary |
| last byte of a file with no trailing newline | fixture ending mid-line | line count matches `file.lines` | `wire.rs:586` `unterminated` handling must agree between the two records |
| multibyte line | a fixture line with non-ASCII before the span | `col` counts scalars, not bytes | the only place byte and char diverge |
| `--lines` off | same fixture | output byte-identical to today | proves the default did not move |

## Deliverable

`TASKS/lane-lines-flag.REPORT.md` with: the four verification outputs verbatim, the `## help.rs text for the coordinator` section, and one line per row saying done or why not. Commit with `Refs-Issue: @extract-lines-flag`.

## Style laws (apply to code comments, commit messages and the report)

- No em dashes. No sycophancy. No negative parallelism (`not X, Y` / `this isn't X. it's Y`).
- No deictic filler: never `here is`, `here's`, `below is`, `the following`. Point with file:line.
- Banned in prose AND identifiers: `provenance` (use source/origin), `substrate` (base layer), `load-bearing` (critical), `regime` (mode), `ground` as a verb (verified/checked-against), `ruling` (decision), `honest(ly)` (state the fact), `distill` (shrink/condense).
- Output is lists and tables. Prose is a one-line caption, never the medium.
- Short sentences. Active voice. Real paths, real numbers.
