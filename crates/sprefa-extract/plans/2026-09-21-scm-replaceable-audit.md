# How much of `src/lang/` is a syntax walk a `.scm` could produce

Audit date 2026-09-21, worktree `main-codex-attribution` at `fc89b1d5`.

## What was measured

`src/lang/` holds 60 Rust files, 46,820 lines. 36,993 of those lines sit inside
a `fn`; the remaining 9,827 are module docs, `use` lines, type definitions,
trait `impl` headers and consts. The function inventory was taken with the
crate's own query tool:

```
ryi query --lang rust --query '(function_item name: (identifier) @n) @f' FILE
```

which yields 1,605 functions with line spans. Every function was placed in
exactly one bucket. Line counts below are function lines, not file lines.

### Buckets

| bucket | test applied |
|---|---|
| `walk` | emitted rows are a function of the matched node alone: its kind, its captured descendants, their text and spans. A `.scm` pattern plus captures expresses it. |
| `walk-plus` | the same, followed by a bounded Rust step: interning, dedupe, span arithmetic, name joining, a static table lookup keyed by the captured name. |
| `resolve` | consults something outside the file: a corpus def index, a module table, a filesystem path, a scip document. |
| `bridge` | drives an external checker, compiler or macro expander (`rust_checker_ra.rs`, `go_checker.rs`, `ts_checker.rs`, `rust_checker.rs`, `rust_mbe.rs`). |
| `edit` | computes replacement text or a staging plan: rename respelling, rehome manifests, cleave import rewriting, owned-region markers. |
| `other` | threads an environment across the traversal (dataflow scopes, receiver binding scopes), mints recursive structural type ids, or is plumbing: constructors, `Display`, accessors, `Source` trait drivers, and the `.scm` engine itself (`1_ast_rule.rs`, `2_source_query.rs`, `3_source_facts.rs`, `5_scm_lower.rs`, `7_scm_rows.rs`, `8_scm_store.rs`). |

Two rulings drive most of the split. First, a function that matches on
`syn::Item`, `ts::Expression` or a tree-sitter `node.kind()` and pushes rows is
`walk` however long it is: `item_entity` at 90 lines and `kt_decl_edges` at 109
are both `walk`. Second, a function that carries a mutable scope through the
recursion is not a walk, because a query has no place to put the scope: every
`flow_*`, `df_flow_*` and `py_flow_*` function, and every receiver collector in
`*_receivers.rs` and `go_walk_receivers`, lands in `other`.

## Language by bucket (function lines)

| language | walk | walk-plus | resolve | bridge | edit | other | total |
|---|---|---|---|---|---|---|---|
| `rust` | 2517 | 224 | 2503 | 932 | 1592 | 2399 | 10167 |
| `ts` | 2106 | 233 | 1501 | 136 | 827 | 2324 | 7127 |
| `go` | 1120 | 218 | 1348 | 314 | 0 | 2394 | 5394 |
| `python` | 1258 | 119 | 1021 | 0 | 0 | 1876 | 4274 |
| `kotlin` | 1133 | 137 | 543 | 0 | 224 | 1462 | 3499 |
| `shared` | 172 | 391 | 90 | 0 | 113 | 2488 | 3254 |
| `prolog` | 868 | 32 | 279 | 0 | 431 | 461 | 2071 |
| `data` | 473 | 44 | 0 | 0 | 0 | 102 | 619 |
| `markdown` | 383 | 0 | 26 | 0 | 0 | 46 | 455 |
| `commonlisp` | 28 | 0 | 0 | 0 | 0 | 39 | 67 |
| `gdscript` | 28 | 0 | 0 | 0 | 0 | 38 | 66 |
| **total** | **10086** | **1398** | **7311** | **1382** | **3187** | **13629** | **36993** |

`shared` is the numbered engine files plus `astgrep.rs`, `extract_lang.rs`,
`fact.rs` and `mod.rs`. Its 391 `walk-plus` lines are almost all
`6_scm_family.rs`, which is the Kotlin CallF post-step the precedent left
behind.

## Family by bucket (function lines)

| family | walk | walk-plus | resolve | bridge | edit | other | total |
|---|---|---|---|---|---|---|---|
| `cst` | 3959 | 464 | 2570 | 0 | 0 | 2207 | 9200 |
| `type` | 978 | 123 | 169 | 1143 | 0 | 3378 | 5791 |
| `call` | 1661 | 272 | 990 | 239 | 0 | 1150 | 4312 |
| `import/specifier` | 748 | 104 | 2548 | 0 | 0 | 214 | 3614 |
| `df` | 0 | 0 | 0 | 0 | 0 | 3600 | 3600 |
| `rename` | 1388 | 0 | 819 | 0 | 703 | 365 | 3275 |
| `infra` | 172 | 391 | 90 | 0 | 113 | 2488 | 3254 |
| `rehome` | 324 | 0 | 99 | 0 | 2371 | 67 | 2861 |
| `data` | 856 | 44 | 26 | 0 | 0 | 148 | 1074 |
| `cfg` | 0 | 0 | 0 | 0 | 0 | 12 | 12 |
| **total** | **10086** | **1398** | **7311** | **1382** | **3187** | **13629** | **36993** |

The shape of the answer is in this table. `cst` and `data` are mostly walk.
`call` is half walk, half target resolution. `df` is zero walk: all 3,600 lines
thread a scope. `type` is one fifth walk, because `tsi_type_id` and its
relatives mint recursive structural ids rather than read a node. `import/
specifier` is one fifth walk: finding the import statement is a walk, turning
its text into a file on disk is not. `rehome` is almost entirely `edit`.

## The ten largest `walk` / `walk-plus` functions

`6_scm_family.rs` is excluded; it is the output of the precedent, not a
candidate for it.

| # | lines | file | fn | family | bucket | node kinds a query would need |
|---|---|---|---|---|---|---|
| 1 | 116 | `src/lang/prolog/_0_source.rs` | `walk_goals` | cst | walk | `compound_term`, `atom`, `unquoted_atom`, `quoted_atom`, `binary_operation`, `unary_operation`, `parenthesized`, `curly_block`, `cut`, `operator_atom`, fields `left` `right` `operand` `argument` |
| 2 | 116 | `src/lang/rust.rs` | `call_defs_in_items` | call | walk | `syn::Item::{Fn,Impl,Mod,Trait,Const,Static,Enum}`; in tree-sitter-rust: `function_item`, `impl_item`, `mod_item`, `trait_item`, `const_item`, `static_item`, `enum_item`, `attribute_item` |
| 3 | 109 | `src/lang/kotlin.rs` | `kt_decl_edges` | cst | walk | `class_declaration`, `type_identifier`, `interface`, `delegation_specifier`, `type_parameters`, `type_parameter`, `class_body`, `enum_class_body`, `enum_entry`, `primary_constructor`, `class_parameter`, `property_declaration`, `variable_declaration`, `simple_identifier` |
| 4 | 106 | `src/lang/rust_rename.rs` | `visit_item_use` | rename | walk | `use_declaration`, `scoped_use_list`, `use_list`, `use_as_clause`, `use_wildcard`, `identifier`, `crate`, `self`, `super`, `visibility_modifier` |
| 5 | 101 | `src/lang/markdown/_0_source.rs` | `project_inline_links` | data | walk | `inline_link`, `full_reference_link`, `collapsed_reference_link`, `shortcut_link`, `image`, `image_description`, `link_text`, `link_label`, `link_destination`, `link_title`, `uri_autolink`, `email_autolink` |
| 6 | 100 | `src/lang/ts.rs` | `scan_module_specifiers` | import/specifier | walk | `import_statement`, `export_statement`, `import_clause`, `named_imports`, `import_specifier`, `namespace_import`, `string`, `default` (oxc side: `ImportDeclaration`, `ExportNamedDeclaration`, `ExportAllDeclaration`, `ImportSpecifier`, `ImportDefaultSpecifier`, `ImportNamespaceSpecifier`) |
| 7 | 90 | `src/lang/python/_0_source.rs` | `py_walk_imports` | import/specifier | walk | `import_statement`, `import_from_statement`, `future_import_statement`, `aliased_import`, `dotted_name`, `alias`, `name`, field `module_name` |
| 8 | 90 | `src/lang/rust.rs` | `item_entity` | cst | walk | `struct_item`, `enum_item`, `union_item`, `type_item`, `trait_item`, `impl_item`, `mod_item`, `function_item`, `type_identifier`, `identifier` |
| 9 | 88 | `src/lang/rust_type_edges.rs` | `item_edge_candidates` | type | walk | `struct_item`, `enum_item`, `union_item`, `type_item`, `trait_item`, `impl_item`, `mod_item`, `field_declaration_list`, `field_declaration`, `enum_variant_list`, `type_parameters` |
| 10 | 87 | `src/lang/data/_0_source.rs` | `entries` | data | walk | `document`, `block_mapping`, `block_mapping_pair`, `flow_mapping`, `flow_pair`, `object`, `pair`, `table`, `table_array_element`, `inline_table`, fields `key` `value` |

Ranks 11 and 12, both 87 and 80 lines, are `go.rs::go_edge_candidates`
(`struct_type`, `interface_type`, `field_declaration_list`,
`field_declaration`, `method_elem`, `type_elem`, `type_parameter_declaration`)
and `python/_2_modules.rs::walk_imports`, which repeats rank 7 on the module
side.

## Predicates the lowering lacks that the top ten need

`5_scm_lower.rs` dispatches `#inside?`, `#has?`, `#follows?`, `#precedes?`,
`#match?`, `#pattern?`, `#nth-child?` and `#range?`, each also accepting a
`not-` prefix. Nothing else reaches an `AstRule`. What the ten above would ask
for and not get:

1. **Field selectors.** The lowering states that `function:`, supertypes and
   quantifiers "have no operator in the rule model, so the walk drops them and
   keeps the node constraint underneath". `walk_goals` distinguishes `left`
   from `right` on a `binary_operation`; `entries` distinguishes `key` from
   `value` on a pair; `py_walk_imports` needs `module_name:`. Dropping the
   field makes all three ambiguous, not merely less precise.
2. **`#eq?`.** It is absent from `5_scm_lower.rs` entirely. The native `cst {}`
   path in `2_source_query.rs` does accept `#sprefa-eq?` (capture to capture,
   or capture to string), so the two query surfaces in this crate carry
   disjoint predicate sets. `kt_decl_edges` compares an enum entry name against
   its owner; `visit_item_use` compares a leaf against the renamed symbol.
3. **A capture bound to an ancestor.** `#inside?` filters a match by its
   context but cannot report the context. `call_defs_in_items` needs the
   enclosing `mod_item` and its `#[cfg(test)]` attribute alongside each def;
   `walk_goals` needs the enclosing clause head.
4. **More than one reported node per rule.** The lowering refuses two
   predicates in one definition that constrain different captures
   (`FocusConflict`), because "an ast-grep rule reports exactly one node per
   match". Every function above emits a row with two or more spans in it. This
   is why `queries/kotlin/scip.scm` runs through the native tree-sitter query
   in `6_scm_family.rs` rather than through `lower_scm`.
5. **Quantifiers.** `kt_decl_edges` wants every `delegation_specifier` child,
   `entries` wants every pair in a mapping, `scan_module_specifiers` wants every
   specifier in a clause. Without `*` or `+` each needs one rule per arity, or a
   Rust loop over the captured parent.
6. **The wildcard `(_)`.** It is a `Syntax` error in the lowering. `entries`
   accepts any scalar under a `value` field; `project_inline_links` accepts any
   inline content inside `link_text`.
7. **A sibling ordinal as a capture.** `#nth-child?` filters by position but
   returns nothing. `kt_decl_edges` numbers its supertype edges,
   `item_edge_candidates` numbers its fields, and `entries` numbers array
   elements. The ordinal has to come back from the Rust side.
8. **Text-shaping predicates.** No `#downcase?`, `#trim?`, `#unquote?` or
   `#join?`. `entries` unquotes TOML keys and unescapes JSON strings;
   `py_walk_imports` joins a `dotted_name` into one module string.
9. **Supertype syntax** (`expression/identifier`). Dropped by the lowering.
   `walk_goals` and `project_inline_links` both match a supertype set and would
   have to spell every member.

## The total

The walk and walk-plus buckets together hold **11,484 lines**: 10,086 that a
`.scm` pattern with captures expresses on its own and 1,398 more that need a
bounded Rust step after the query. Against the 36,993 lines that sit inside a
function in `src/lang/` that is **31.0 percent**; against the full 46,820 lines
of the directory it is **24.5 percent**. The rest divides into 13,629 lines of
scope-threading and id-minting logic that has nowhere to keep its state in a
query (3,600 of them dataflow alone), 7,311 lines of cross-file and module-path
resolution, 3,187 lines of replacement-text computation for rename, rehome and
cleave, and 1,382 lines driving rust-analyzer, tsc and the Go checker. The
Kotlin CallF precedent removed 349 lines and replaced them with a 103-line
`.scm` plus a 198-line projection in `6_scm_family.rs`, so the realistic return
on the 11,484 is a reduction rather than a deletion: roughly a third of those
lines come back as query text and another third as projection code, which puts
the reachable net saving near 4,000 lines, concentrated in the `cst` family of
`rust`, `ts`, `prolog` and `markdown`.
