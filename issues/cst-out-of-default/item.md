---
created: 2026-09-18
updated: 2026-09-19
type: improvement
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
---

# cst out of the default family set, per-language rosters instead

## Description

## Description

`extract src/main.ts` emits **10853 JSONL lines** for a 587-line file.

| family | lines | share |
| --- | --- | --- |
| `cst` nodes + edges | 7571 | **70%** |
| `df` | 2729 | 25% |
| `call` | 475 | 4% |
| rest | ~78 | 1% |

70% of the default output is a serialized parse tree. The thing most callers want is 4%.

Nobody chose this. `--family` defaults to all four kinds, and a default that is the union of every option is an unmade decision.

### Who consumes cst rows

| consumer | status |
| --- | --- |
| dl7 programs in `~/projects/sprefa` | **zero** hits across `std/*.dl7` and `prelude/*.dl7` |
| this crate's tests | 10 files, each passes `--family cst` explicitly, so they survive the change |
| `--family cfg` | derives from cst internally (`--schema:202-203`), does not need the rows on stdout |
| cst-only languages | python, java, c, cpp, cs, rb, php, sh, lua, scala, swift, ex, hs, md, gd, lisp, html, css |

That last row is the catch. For 18+ extensions cst is the only plane extract emits. A flat drop makes `extract foo.py` print nothing.

### The default becomes per-language

| language | default families | cst |
| --- | --- | --- |
| ts, rust, go, kotlin, prolog | `call, type, df` | opt-in via `--family cst` |
| everything else | `cst` | it is the answer |

`extract src/main.ts` drops from 10853 lines to 3282. `extract foo.py` unchanged.

## Acceptance Criteria
- [ ] default family set is derived from the language's own plane roster, not a fixed list
- [ ] `extract src/main.ts` emits 3282 lines, `--family cst` still gives 7571
- [ ] `extract <cst-only-file>` output is byte-identical to today
- [ ] `--help` LANGUAGE COVERAGE table states the per-language default
- [ ] `cargo test --features cli` green

## Tests Run

## Implementation Notes

The roster already knows each language's planes (`docs/0_architecture-matrix-20260917.md`). This reads that rather than adding a table.

## Decisions

### 2026-09-19T22:02:40Z · @claude-opus-5

Superseded by @default-families-no-conditional, user-set 2026-09-19. The per-language table in this issue is rejected along with the per-file conditional the lane built at 32f82d98. One default for every language, call,type,df, cst always opt-in. Languages with no front-end get a generic CST-derived call plane driven by a call-kind table, since every tree-sitter grammar names its call nodes. A file that yields zero facts discloses the next commands rather than printing a parse tree or nothing. Branch improvement/cst-out-of-default is NOT merged; salvage the wire_golden.jsonl regeneration and the explicit mask in tests/4_capability_parity.rs.
