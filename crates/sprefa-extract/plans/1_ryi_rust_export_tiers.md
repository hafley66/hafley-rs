# Rust extractor symbol tiers from ryi

Source: `src/lang/rust/lib.rs` on `feature/scm-rust-front-end`, 2026-09-23.

```sh
target/debug/ryi --resolve --family call,type --sqlite /private/tmp/ryi-rust-export-tiers-20260923.db src/lang/rust/lib.rs
```

The export contains 241 `resolved_edge` rows and 4 `resolved_type_edge` rows. The tier calculation joins call-site offsets to the smallest enclosing function or method, and definition offsets to ryi's call and type nodes. It yields 88 declarations, 203 matched non-self dependencies, 87 strongly connected components, and one cycle. Tier 0 has no dependencies within this file. Each higher tier is one plus the highest dependency tier. External crate dependencies are outside this graph.

| Tier | Symbols that determine the split |
| ---: | --- |
| 8 | `Source::extract` |
| 7 | `project_df` |
| 6 | `df_items`, `splice_macro_expansions` |
| 5 | `project_call`, `flow_fn_body` |
| 4 | `Resolve<TypeF>::resolve`, `module_specifiers`, `flow_block` + `flow_expr` (one SCC) |
| 3 | `project_types`, `resolve_type_dst`, `Resolve<CallF>::resolve`, `collect_module_leaves`, `bind_pat` |
| 2 | `item_entity`, `const_values`, `module_scoped_type`, call-name matching, `call_drops`, `use_tree_leaves`, `bind_pat_rec` |
| 1 | leaf combiners and walks: `push_entity`, `fn_sigs`, `module_target`, `own_file_blob`, `push_call_edge`, `visit_item`, `visit_expr`, `df_push`, `df_loop_row` |
| 0 | spans, types, constants, query accessors, and other local leaves |

```d2
direction: down
extract: "T8  Source::extract" {style.fill: "#075985"}
df: "T7  project_df" {style.fill: "#166534"}
calls: "T5  project_call" {style.fill: "#166534"}
types: "T3  project_types" {style.fill: "#166534"}
resolve: "T3–4  call/type resolution" {style.fill: "#92400e"}
base: "T0–2  spans, module paths, rows, queries" {style.fill: "#374151"}
extract -> df
extract -> calls
extract -> types
df -> base
calls -> base
types -> base
resolve -> base
```

## Export surface and direct in-file dependencies

| Export | Tier | Direct in-file dependencies | External consumer |
| --- | ---: | --- | --- |
| `syn_span` | 0 | none | Rust docs, type edges, modules, receivers, rename, rehome, SCIP macros |
| `module_segments` | 0 | none | Rust modules |
| `ModuleTarget` | 0 | none | Rust modules |
| `module_target` | 1 | `module_segments`, `ModuleTarget`, `crate_root_of` | Rust modules |
| `crate_root_of` | 0 | none | Rust modules |
| `own_blob_probes` | 0 | none | resolve scaling test |
| `call_drops` | 2 | `own_file_blob`, `module_qualifier` | project resolver |
| `def_span` | 0 | none | Rust modules, receivers |
| `mod_path_attr` | 0 | none | Rust modules |
| `RustSource` | 0 as a declaration | `extract` at tier 8, two `Resolve` implementations at tiers 3 and 4 | language roster, project resolver, rename, rehome, cleave, tests |
| `build_line_starts`, `variant_def_range` | re-exports from `hafley_scm` | none in this file | Rust modules, rename, rehome, TS rename, Kotlin rename, Prolog, SCIP macros |

The four resolved type edges cover only `ModuleTarget`, `CallCollector`/`CollectedSite`, and `RustSource` self/field references. Signature rows contain 234 parameter and 84 return type mentions, but ryi's current same-file type resolver does not turn most into edges. The tier graph is therefore a call/constructor dependency order, not a complete Rust type-use order. Visibility and external consumers above are source cross-checks, not claims from ryi's edge resolver.
