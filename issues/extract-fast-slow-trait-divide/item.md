---
created: 2026-09-16
updated: 2026-09-20
type: improvement
status: open
priority: high
size: L
epic: scip-ingestion-conformance
---

## Description

`extract fast` (syntax/diet) and `extract slow` (compiler/SCIP) are not two impls of one contract. They are disjoint fact planes, so neither spans the feature set, and `fast`'s accuracy cannot be determined at all.

Measured on a real Rust corpus (107 files, `~/projects/ascii-renderer`):

| plane | rows | what it carries |
| --- | ---: | --- |
| `extract fast` | 1,983,339 | cst 666,003, df 230,253, call 4,718, type 2,845. Spans everywhere, loops/nesting/allocations. |
| `extract slow` | 18,848 | symbol plane only: `scip_local` 8,763, `scip_fn_edge` 5,475, `scip_def` 1,328, `scip_name` 1,327, `scip_ref` 1,668, `scip_edge` 78, `scip_callee_type` 208. **No cst, no df.** |

Three consequences, each verified against the field corpus:

1. **Definition names are absent in fast mode.** `node.name` is NULL for every CST definition kind: `function_item` 2,426 rows / 0 named, `struct_item` 373/0, `const_item` 350/0, `impl_item` 201/0, `mod_item` 197/0, `enum_item` 56/0, `static_item` 35/0, `type_item` 26/0, `trait_item` 2/0. In fast mode only dataflow nodes are named (`var_read` 58,637, `let_bind` 22,383, `param` 9,960). A consumer holding a call span cannot learn its enclosing function from the tool; it has to re-read source bytes at the span.
2. **The symbol plane carries no spans.** `scip_def` and `scip_ref` hold `file` + `symbol`; `scip_fn_edge` holds caller/callee symbols. Nothing to join a fast call-site span against.
3. **So fast's accuracy is unmeasurable, and every naive attempt is an artifact.** Joining the planes by name swings on nothing but how symbol shapes are classified:

| join | overlap | implied rate |
| --- | ---: | --- |
| all caller to callee name pairs | 922 (fast 8,056 / slow 4,308) | 11% |
| callees restricted to `().` function symbols | 801 | 55% recall, 56% precision |
| methods (`X#method.`) included | 802 | 30% recall, 56% precision |

The disagreement is definitional, not error: SCIP counts enum-variant construction (`Dir::Up`) and field access (`Rect#h.`) as edges, and attributes calls to the enclosing symbol, while fast attributes a call inside a `static`/`const` initializer to the static's name (`SETTINGS` to `new`). None of those percentages is an accuracy figure.

What fast mode can be trusted for today, verified: syntax facts need no resolution, so `df_loop` (3,211 loops, each with the collection it iterates), `df_nest` (18,100 calls inside loops, nesting depth 1 to 5: 12,368 / 4,796 / 817 / 110 / 9) and `df_allocates` (515 functions) are parser-authoritative. Call resolution self-labels its uncertainty: 18,755 resolved edges against 19,281 unresolved carrying reasons (`inferred` 11,482, `no_corpus_def` 5,081, `ambiguous` 2,010, `external` 708) and origins (`module_plane` 7,970, `same_file` 5,774, `corpus_unique` 2,884, `self_type` 1,667, `receiver` 460). The honest reading is "half resolved, and the tool names the half it guessed".

## Requested shape

One fact vocabulary (cst, type, call, df, plus the symbol/defs/refs/edges plane) declared behind a trait, with `fast` and `slow` as two impls of it that each span the entire feature set. Then a query runs unchanged against either provider, and the diff of the two impls on one query is the accuracy metric, measured instead of argued.

Minimum increment that makes the planes joinable, independent of the full refactor:

- populate `name` on CST definition nodes from the grammar's `name:` field;
- emit span columns on `scip_def`, `scip_fn_edge` and `scip_ref`.

## Acceptance Criteria

- [ ] one trait declares the fact kinds; both providers implement it and are selected behind one flag.
- [ ] CST definition nodes carry `name` in fast mode (function_item, struct_item, enum_item, trait_item, impl_item, const_item, static_item, type_item, mod_item).
- [ ] the symbol plane carries spans, so a call-site span joins to its compiler symbol.
- [ ] the same query returns comparable rows from both providers on a field corpus.
- [ ] fast-vs-compiler precision and recall are reportable as a number, with the compared unit named in the output.

## Tests Run

Corpus: `/Users/chrishafley/projects/ascii-renderer` (107 Rust files, `src/*.rs` plus `src/modes/*.rs`).

```sh
extract fast --sqlite /tmp/ascii-fast.db $(ls src/*.rs src/modes/*.rs)   # 1,983,339 rows, 4.18 s
extract slow --sqlite /tmp/ascii-slow.db .                               # 18,848 rows, 0.34 s
extract --family cst src/modes/_46_volute.rs | grep function_item        # name: null on every row
```

Every number above came from `sqlite3` against those two databases:

```sql
-- name gap
SELECT kind, COUNT(*), SUM(CASE WHEN name IS NOT NULL THEN 1 ELSE 0 END)
FROM node WHERE kind IN ('function_item','struct_item','impl_item','const_item','static_item',
                         'enum_item','trait_item','type_item','mod_item') GROUP BY kind;
-- resolution plane
SELECT resolution_origin, COUNT(*) FROM resolved_edge GROUP BY 1;
SELECT reason, COUNT(*) FROM unresolved GROUP BY 1;
-- plane inventory
SELECT family, COUNT(*) FROM node GROUP BY family;   -- slow db: every scip_* table
```

`slow` reused a warm `.dl/.state/index.scip`, so its 0.34 s excludes index construction; cold cost was not measured.

## Implementation Notes

- `crates/sprefa-extract` is excluded from the root workspace and is its own workspace root, so a change here is proven by `cd crates/sprefa-extract && cargo test --features cli`.
- `tests/golden_parity.rs` fails 2 cases in this checkout (`ported_facets_match_v5` ts `lambdas`, `rust_doc_parity`) because 11 captured oracles still carry a `v6/sprefa-extract/...` root prefix; unrelated to this issue, do not re-debug.
- The consumer that hit this: a loop/blowout audit that maps every call inside every loop to its enclosing function. Fast mode supplied the loops and the call spans; the missing definition names forced a source-byte re-read at each span, and the missing spans on the symbol plane made the compiler cross-check impossible.
