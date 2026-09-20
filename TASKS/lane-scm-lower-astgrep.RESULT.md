# lane: `.scm` surface syntax lowering to ast-grep `AstRule`

Branch `feature/scm-lower-astgrep`, crate `crates/sprefa-extract`, issue
`issues/lab-scopegraph-queries/item.md`.

A `.scm` file is parsed by `tree-sitter-tsquery`, walked, and lowered into
`sprefa_extract::AstRule`. `ast_grep_core` evaluates. No matching operator was
implemented, no S-expression parser was hand-written.

| file | state |
| --- | --- |
| `crates/sprefa-extract/src/lang/5_scm_lower.rs` | new, 493 lines |
| `crates/sprefa-extract/src/lang/mod.rs` | +3 lines, module and re-export |
| `crates/sprefa-extract/Cargo.toml` | `tree-sitter-tsquery = "0.8"` plus its comment |
| `crates/sprefa-extract/Cargo.lock` | generated |
| `crates/sprefa-extract/tests/144_scm_lower.rs` | new, 313 lines, 18 tests |

## Receipts

| command | result |
| --- | --- |
| `cargo tree -d -p sprefa-extract \| grep -c "^tree-sitter v"` | `0` duplicates; `cargo tree` lists one core, `tree-sitter v0.25.10` |
| `cargo test -p sprefa-extract --features cli` | `1004 passed, 0 failed` across 186 binaries |
| `cargo metadata --locked --format-version 1` | `LOCK_OK` |
| `cargo clippy -p sprefa-extract --all-targets --features cli` | 289 warning/error lines repo-wide, `0` of them in either new file |

Baseline was `986 passed, 0 failed` across 185 binaries. 986 + this file's 18 =
1004, and the binary count rises by exactly one. The two pre-existing
`golden_parity` root-prefix diffs named in the `hafley-rs-repo` skill did not
fire in this checkout.

`--features cli` is required. Plain `cargo test -p sprefa-extract` fails to
compile `tests/141_unresolved_contract.rs`, which calls `tempfile::tempdir`
while `tempfile` is optional and gated behind `cli`. That is pre-existing and
untouched here.

## The lowering table as built

| query CST node | `AstRule` |
| --- | --- |
| `named_node`, no children | `Kind(name)` |
| `named_node` with nested patterns | `All[Kind(name), Has{child}, ...]` |
| `named_node` with `negated_field` | `All[Kind(name), Not(Has{Kind(field)})]` |
| `anonymous_node` `"return"` | `Kind("return")` |
| `list` `[a b]` | `Any[a, b]` |
| `grouping` `(a b)` | `All[a, b]` |
| `field_definition` `f: (x)` | the inner rule; the field name is dropped |
| `predicate` `#inside?` | `Inside { rule, stop_by: End("end") }` |
| `predicate` `#has?` | `Has { rule, stop_by: End("end") }` |
| `predicate` `#follows?` | `Follows { rule, stop_by: End("end") }` |
| `predicate` `#precedes?` | `Precedes { rule, stop_by: End("end") }` |
| `predicate` `#match?` | `Regex(string_content)` |
| `identifier` argument | `Matches(label)`, resolved against the top-level labels |
| top-level `definition` with a direct `capture` | one `NamedAstRule` in `utils` |

## The root question, measured

An ast-grep rule reports exactly ONE node per match, and a predicate names the
capture it constrains. The brief's target spans decide which node that is. Three
rule shapes were run against `RUST_SRC` through `query_ast_rule` before the walk
was written:

| shape | matches |
| --- | --- |
| `All[All[Kind(call_expression), Has{...}], Inside{Matches(local.scope)}]` | `58..79` `"name.contains(needle)"`, `120..138` `"items.contains(&3)"` |
| `All[Kind(field_identifier), Inside{...}, Inside{Matches(local.scope)}]` | `63..71` `"contains"`, `126..134` `"contains"` |
| `All[Kind(field_identifier), Pattern("$M")]` | `63..71`, `126..134`, each with capture `M` |

Row 2 is the brief's measured target, so the lowering roots a predicated
definition at the host of the capture the predicate names, and expresses the
pattern around it as `Inside`:

```text
All[
  <rule of the capture's host subtree>,
  Inside{ <rule of the whole pattern>, stopBy: end },   # skipped when host == pattern root
  <one relation per predicate>,
]
```

Row 3 shows that a metavariable binding would work. It is NOT emitted: the
capture text is already the match span under this rooting, capture names such as
`local.scope` carry characters no metavariable spelling accepts, and a
`Pattern` arm risks a per-grammar parse failure for data already present.

## Tests, every name with its assertion

`crates/sprefa-extract/tests/144_scm_lower.rs`, 18 tests, all through the library
and the real grammar.

| test | assertion |
| --- | --- |
| `the_query_grammar_abi_sits_inside_the_runtime_window` | `scm_language().abi_version()` is in `13..=15` |
| `a_bare_named_node_lowers_to_kind` | `(function_item)` -> `Kind("function_item")`, `utils` empty |
| `a_list_lowers_to_any` | `[(function_item) (block)]` -> `Any[Kind, Kind]` |
| `a_nested_field_pattern_lowers_to_kind_plus_has` | `(call_expression function: (field_expression))` -> `All[Kind, Has{Kind}]` |
| `a_grouping_lowers_to_all` | `((function_item) (block))` -> `All[Kind, Kind]` |
| `a_negated_field_lowers_to_not_has` | `(function_item !body)` -> `All[Kind, Not(Has{Kind("body")})]` |
| `a_named_reference_lowers_to_matches_plus_one_util` | the rust `scope.scm`: `utils` is exactly one `local.scope` entry holding `Any[3 Kinds]`, and `rule` is the full `All[...]` literal including `Matches("local.scope")` |
| `follows_and_precedes_lower_to_their_relations` | `#follows?` -> `Follows{Matches("s"), end}`, `#precedes?` -> `Precedes{Matches("s"), end}` |
| `a_has_predicate_lowers_to_has_and_a_match_predicate_to_regex` | `#has?` -> `Has{Matches("s"), end}`; `(#match? @m "^self\.")` -> `Regex("^self\.")` |
| `the_rust_scope_query_matches_both_method_names` | exactly `[(63, 71, "contains"), (126, 134, "contains")]` |
| `the_ts_scope_query_matches_both_method_names` | 2 matches, both text `"includes"` |
| `an_unmapped_predicate_is_an_error` | `(#nope? @a b)` -> `UnknownPredicate("nope?")` |
| `an_identifier_argument_naming_no_definition_is_an_error` | `(#inside? @m no_such)` -> `UnboundReference("no_such")` |
| `a_wrong_parameter_count_is_an_error` | `(#inside? @m)` -> `PredicateArity { operator: "inside?", got: 1 }` |
| `an_error_node_never_lowers_to_a_partial_rule` | `(function_item` -> `Syntax { row: 0, message: "unparsed `.scm` text: (function_item" }` |
| `a_duplicate_top_level_label_is_an_error` | two `@s` labels -> `DuplicateLabel("s")` |
| `two_predicates_on_two_captures_are_an_error` | `#inside? @x` beside `#inside? @y` -> `FocusConflict { first: "x", second: "y" }` |
| `a_file_of_only_labelled_patterns_matches_every_label` | no unlabelled pattern -> `rule` is `Any[Matches(label)]` over `utils` |

### The two span sets

| language | source | matches |
| --- | --- | --- |
| rust | `RUST_SRC`, `probe.rs` | `63..71` `"contains"`, `126..134` `"contains"` |
| ts | `TS_SRC`, `probe.ts` | 2 matches, both `"includes"` |

The ts `scope.scm` swaps `field_expression`/`field_identifier` for
`member_expression`/`property_identifier` and the scope list for
`function_declaration`/`arrow_function`/`statement_block`. The engine and the
walk are unchanged between the two: 0 lines differ.

## Deviation from the brief's signature

`ScmLowerError` carries TWO variants the brief's enum did not list. Both name a
uniqueness condition the brief itself demanded or the rooting created:

| variant | condition | why it is not one of the five |
| --- | --- | --- |
| `DuplicateLabel(String)` | one capture label used by two top-level definitions | the brief requires this be an error rather than last-wins, and none of the five spells it |
| `FocusConflict { first, second }` | two predicates in one definition constraining two different captures | an ast-grep rule reports one node, so one definition has one root; silently taking the first would drop the second predicate |

## Not implemented

| thing | why |
| --- | --- |
| quantifiers `*`, `+`, `?` | `AstRule` has no repetition operator; the walk drops them and keeps the node constraint |
| field selectors (`function:`) | no field operator in the rule model; the child becomes a plain `Has` |
| supertypes (`expression/identifier`) | no supertype operator; the `name` field alone is lowered |
| `negated_field` as a real field test | lowered as `Not(Has{Kind(field_name)})`, which holds only where a grammar spells a field and a node type the same way |
| the wildcard `(_)` and `(MISSING x)` | no `AstRule` for either; both are `Syntax` |
| string escape decoding | `string_content` text reaches `Regex` verbatim, so a `.scm` regex writes its own backslashes |
| metavariable bindings for nested captures | see the root question above |
| anchors (`.`) and sibling order beyond one `Follows`/`Precedes` shape each | no fixture in this lab orders siblings |
| every language but rust and ts | the lab's stated bound |
| `ryi scm`, a `SourceQuery` arm, Helix file vendoring, any edit to `2_source_query.rs` | out of scope by the brief |
