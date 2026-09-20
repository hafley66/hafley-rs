# rename-path-union: RESULT

`ryi rename` plans the union over every module a `#[path]`-named file is; the exit 6 for a double reach is gone.

## Exit codes

| run | before | after |
| --- | --- | --- |
| `rename src/bin/home.rs#Thing Renamed`, two `#[path]` routes | 6, stderr `path attr twice reaches src/bin/home.rs at runtime` x2, no plan | 0, plan printed |
| one `#[path]` route | 0 | 0, same rows |
| TS computed member (`Dynamic`) | 6 | 6 |

## Tests

`tests/147_rename_path_union.rs` (147 was free; 141-146 taken):

- `a_symbol_two_path_attrs_reach_gets_a_plan_instead_of_a_stop`
- `the_plan_is_the_union_of_both_routes`
- `an_occurrence_both_routes_reach_is_planned_once`
- `a_file_with_one_route_plans_exactly_as_before`
- `a_dynamic_stop_from_another_cause_still_refuses`

Phase 1 receipt, against unmodified logic: `expected a plan: exit Some(6)`, `left: Some(6) right: Some(0)`; 2 passed, 3 failed. The one-route and refusal cases passed there by design; the one-route rows were recorded from that baseline binary.

Updated in place: `tests/146_rename_stop_lines.rs`. Four cases pinned the two-route stop (`file:line`, one-based line, reached file, exit 6). They keep their fixtures and now pin the union contract:

| was | now |
| --- | --- |
| `the_stop_prints_file_and_line_not_a_byte_offset` | `a_two_route_file_prints_no_stop_line` |
| `an_attr_on_the_first_line_prints_line_one` | `a_two_route_file_prints_no_byte_offset` |
| `the_stop_names_the_file_the_route_reaches` | `a_two_route_file_names_no_reached_file` |
| `the_two_route_stop_still_exits_six` | `the_two_route_file_exits_zero` |

`other_arms_keep_the_bare_file_and_line_form` is untouched. `5_rename_rust.rs` and `3_move_rust.rs` pin no double-reach exit 6.

## Sites, `crates/sprefa-extract/src/lang/rust_rename.rs`

| site | line | change |
| --- | --- | --- |
| `symbol_refs` gate on `path_stops` | was `:41-56` | deleted |
| `symbol_refs` anchor modules | `:73-78` | one module per anchor home; `nameable(&anchor_modules)` |
| `symbol_refs` harvest loop | `:95-98` | loops `homes_of(rel)`, `harvest` takes `home` |
| `Corpus.homes` | `:228` | `BTreeMap<String, Vec<ModuleId>>` |
| `Corpus.path_stops` | was `:243` | deleted |
| `homes_of` (was `home`) | `:282` | `&[ModuleId]`, ORPHAN is a 1-element slice |
| `nameable` | `:320-325` | seeds every anchor, loops `homes_of` |
| `scope_of` | `:349` | flattens every `(rel, home)` pair |
| `reexports` | `:361-370` | loops `homes_of` |
| `harvest` | `:405-415` | `home: &ModuleId` and `anchor_modules: &[ModuleId]` parameters |
| `variant_leaf`, `owner_reach` | `:704`, `:723` | `anchors.contains(&module)` |
| `path_module_table` `many` arm | `:1962-1984` | one `Vec<ModuleId>` per file, routes deduped in order; stops deleted |
| `path_decls` | `:1991` | collects modules only |

`resolve` keeps its signature; every caller loops.

## Dedupe

Key: `(file, span.start)` for `refs` in `settle`, `(file, span)` for `seats`. Both sort by `(file, span.start)` first, so plan order is the sort order and never the route order. No new dedupe pass. `homes` routes dedupe by module id in appearance order.

## Single route

A one-module file runs every loop once and pushes the same rows in the same order. `a_file_with_one_route_plans_exactly_as_before` pins rows recorded from the pre-change binary:

```
  src/elsewhere/impl.rs  1 uses
  src/lib.rs  3 uses
```

and the exact post-commit text of both files. Passed before and after.

## Gate

```
cargo test --features cli --no-fail-fast 2>&1 | grep -E "^test result:"
189 binaries, 1002 passed, 0 failed
```

Baseline 188 binaries, 997 passed. Delta: +1 binary, +5 passed (147). `cargo metadata --locked --format-version 1` prints `LOCK_OK`.

## Order deviation

Phase 2 removed the `path_stops` gate and the stops half of `path_module_table` because its own `a_symbol_two_path_attrs_reach_gets_a_plan_instead_of_a_stop` cannot pass while the gate stands. Phase 3 carried the `146` updates.

## Limit

A file's homes are its `#[path]` routes only. A file that lib.rs also declares by layout (`mod types;`) keeps no layout module once a `#[path]` names it, the same as the single-route code. Next action: a layout home beside the routes, in a separate change, before renaming `ExtractOutput` in the crate's own tree.
