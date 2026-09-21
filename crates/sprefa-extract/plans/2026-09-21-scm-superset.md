# Why the `.scm` lowering is not yet a superset, and the design that is

Investigation date 2026-09-21, worktree `main-codex-attribution` at `82ff4686`.
Follows `plans/2026-09-21-scm-replaceable-audit.md`, which counted 11,484 lines
of `src/lang/` that a `.scm` query with captures could express and found nine
things the lowering cannot say.

The demand this answers: one query surface that is a proper superset of native
tree-sitter `.scm` and of ast-grep's rule algebra at once.

## 1. Why the lowering yields one node per match

It is `lower_scm`, not the `Matcher` API. `ast_grep_core` 0.38.7
(`Cargo.lock:98-102`, checksum `a807991d95797b16ed5bf7431be8d1890dccb8c1b2e560c1f0d983a4f125405d`)
carries every `$META` binding with its own live `Node`.
`MetaVarEnv` is `single_matched: HashMap<MetaVariableID, Node>` plus
`multi_matched: HashMap<MetaVariableID, Vec<Node>>` plus `transformed_var`
(`ast-grep-core-0.38.7/src/meta_var.rs:17-21`); `NodeMatch::get_env()`
(`src/matcher/node_match.rs:28`) hands that whole map back, and
`get_matched_variables()` (`meta_var.rs:67-85`) enumerates all three families.

### The probe

Throwaway `examples/scmprobe_throwaway.rs`, run and removed, never committed.
Source under test:

```rust
fn outer(name: &str) -> bool {
    let needle = "ab";
    name.contains(needle)
}
```

Output:

```text
A match 58..79 binds ["ARG", "METHOD", "RECV"]
A   env[$ARG] = "needle" at 72..78
A   env[$METHOD] = "contains" at 63..71
A   env[$RECV] = "name" at 58..62
B match span Span { start: 58, len: 21 } captures [
     AstRuleCapture { name: "ARG", text: "needle", span: Span { start: 72, len: 6 } },
     AstRuleCapture { name: "METHOD", text: "contains", span: Span { start: 63, len: 8 } },
     AstRuleCapture { name: "RECV", text: "name", span: Span { start: 58, len: 4 } }]
C lower_scm(two constrained captures) = Err(FocusConflict { first: "recv", second: "method" })
D lower_scm(one predicate) rule = All([Kind("field_identifier"),
     Inside { rule: All([Kind("call_expression"), Has { rule: All([Kind("field_expression"),
     Has { rule: Kind("identifier"), stop_by: None },
     Has { rule: Kind("field_identifier"), stop_by: None }]), stop_by: None }]),
     stop_by: Some(End("end")) }, Regex("contains")])
D match span Span { start: 63, len: 8 } captures []
E lower_scm(no predicate) rule = All([Kind("call_expression"), Has { rule: All([Kind("field_expression"),
     Has { rule: Kind("identifier"), stop_by: None },
     Has { rule: Kind("field_identifier"), stop_by: None }]), stop_by: None }])
E match span Span { start: 58, len: 21 } captures []
```

Row A is `ast_grep_core` alone: one match, three bindings, three distinct
ranges. Row B is the same env arriving in this crate's own `AstRuleMatch`:
`make_match` (`src/lang/1_ast_rule.rs:465-521`) already walks
`matched.get_env().get_matched_variables()` at `:478-498` and turns every
binding into an `AstRuleCapture` with its own `Span`. The N-spans-per-match
plumbing is finished and shipping.

Rows C, D and E are the lowering on the same query. Two losses, both local to
`5_scm_lower.rs`:

1. **The focus collapse.** `lower_definition` (`src/lang/5_scm_lower.rs:242-279`)
   keeps a single `focus: Option<String>` across the definition's predicates and
   returns `FocusConflict` at `:252-260` the moment a second predicate names a
   different capture. The module header states the reason at `:31-32`: "that
   capture's host node is the rule's root, because an ast-grep rule reports
   exactly one node per match."
2. **Captures are never emitted.** `lower_node` (`:552-581`) and
   `lower_named_node` (`:584-617`) translate a `named_node` into
   `Kind` plus one `Has` per nested pattern. A `capture` child is not in
   `DEFINITION_KINDS`, so `@recv` and `@method` leave no trace in the rule.
   Rows D and E show the consequence: `captures []`. Even where the lowering
   succeeds, the `AstRuleCapture` vector is empty, so the rule reports one span
   and nothing else.

The lane knew this. `TASKS/lane-scm-lower-astgrep.RESULT.md:81-84` records the
measurement and the decision: "Row 3 shows that a metavariable binding would
work. It is NOT emitted: the capture text is already the match span under this
rooting, capture names such as `local.scope` carry characters no metavariable
spelling accepts, and a `Pattern` arm risks a per-grammar parse failure for data
already present."

### The follow-up probe that decides the design

If `@capture` lowered to a metavariable, would the bindings be the right ones?
Second throwaway, same method, on `fn f(a: i32, b: i32) -> i32 { a - b }`:

```text
1 kind only:              1 rows, span 30..35, caps []
2 kind+has(no meta):      1 rows, span 30..35, caps []
3 kind+has($L):           1 rows, span 30..35, caps [("L", "a")]
4 kind+has($L)+has($R):   1 rows, span 30..35, caps [("L", "a"), ("R", "a")]
```

against the native surface on the same file and the same question:

```text
ryi query --lang rust --query '(binary_expression left: (identifier) @l right: (identifier) @r)'
{"end_line":1,"l":"a","line":1,"r":"b"}
```

Row 4 is the whole problem. A nested `Pattern("$X")` does merge into the outer
env, so ast-grep can carry two bindings out of one match. Both bindings land on
the same node, because `Has` has no field and no ordinal: each `Has` searches
descendants and stops at the first `identifier`. Native tree-sitter answers
`l = a`, `r = b`. `AstRule` has no operator that can tell them apart.

So: `get_env()` carries every binding with its node, and this crate already
harvests it. The lowering discards captures and refuses two of them, and even if
it stopped refusing, `AstRule` cannot address which node each capture names.

## 2. Why `#eq?`, `#not-eq?`, `#any-of?` and `#set!` are absent from the lowering

Three separate reasons, and the native side is better off than the audit
recorded.

**The lowering never had them in its table.** `TASKS/lane-scm-lower-astgrep.BRIEF.md:73-87`
lists the entire predicate mapping the lane was told to build: `#inside?`,
`#has?`, `#follows?`, `#precedes?`, `#match?`. `#eq?` is not a row. The lane
built the table it was given, and `lower_predicate`
(`src/lang/5_scm_lower.rs:329-452`) dispatches exactly that set plus the three
later additions (`14dc33ac`, `#nth-child?`, `#range?`, `#pattern?`), with
`UnknownPredicate(operator)` at `:430` for everything else. The published guide
repeats the set at `docs/2_scm-with-ast-grep-relations-20260920.md:84-97` and
never mentions `#eq?` at all, so a reader arriving from Helix or Zed meets
`UnknownPredicate("eq?")` with no warning.

**There is no ast-grep operator to lower `#eq?` onto.** `AstRule`
(`src/lang/1_ast_rule.rs:21-61`) has `Pattern`, `Kind`, `Regex`, `Matches`,
`All`, `Any`, `Not`, `Inside`, `Has`, `Follows`, `Precedes`, `NthChild`,
`Range`. Capture-to-capture text equality needs two captures, which section 1
shows the lowering does not have. `#eq? @a "literal"` could become
`Regex("^literal$")`, and `#any-of?` a `Regex` alternation, but neither was in
the brief and `constraints` (`:76-78`) binds a metavariable to a rule rather
than to text.

**On the native side three of the four already work, two of them unvalidated.**
`rewrite_predicates` (`src/lang/2_source_query.rs:320-355`) renames only
`#not-match?`, `#match?` and `#eq?` into `#sprefa-*` so they arrive at
`general_predicates()` and get checked by `validate_predicates` (`:167-182`).
`#not-eq?`, `#any-of?`, `#any-eq?` and `#not-any-of?` are left alone, which
means the tree-sitter Rust binding evaluates them itself: it compiles them into
`TextPredicateCapture` (`tree-sitter-0.25.10/binding_rust/lib.rs:2614-2736`) and
`QueryMatches::next` applies them at `:3450` and `:3494`. Measured through the
binary on the fixture above:

| query | rows |
|---|---|
| `(#eq? @m "contains")` on a two-capture pattern | 1, carrying both `m` and `recv` |
| `(#not-eq? @v "nope")` | 1 |
| `(#not-eq? @v "needle")` | 0 |
| `(#any-of? @v "needle" "x")` | 1 |
| `(#any-of? @v "zzz")` | 0 |
| `(#set! role def)` | 1, accepted with no error and no effect |
| `(#inside? @v fn)` | `invalid query: predicate #inside? is not allowed` |

So `#not-eq?` and `#any-of?` filter correctly today by way of the binding, past
`validate_predicates` rather than through it. `#set!` is the silent one:
tree-sitter routes it to `property_settings()`, nothing in the crate reads that,
and the row comes back unchanged. That is a trap for any `.scm` vendored from
Helix or Zed, where `#set!` carries the taxonomy.

## 3. Why the two surfaces diverged

Two lanes ran on 2026-09-20 with explicit mutual no-touch fences, and each one
forbade adding predicates to the other's surface.

`TASKS/lane-scm-lower-astgrep.BRIEF.md` owned `5_scm_lower.rs` and said at
`:142-143`: "Do NOT touch `src/lang/2_source_query.rs`. Its `matches_predicates`
pattern-leak is a separate defect on a separate surface and is not this lane's."
Its out-of-scope list repeats it at `:259`: "No change to `2_source_query.rs`,
`query_language`, or `matches_predicates`." The lane was also bounded to a lab
at `:28-29`: "This is a lab. It proves the lowering on rust and ts, nothing
more. It does not vendor Helix files, does not add a CLI verb, does not touch
`ryi query`."

`TASKS/lane-query-predicate-leak.BRIEF.md` owned `2_source_query.rs` and said
the mirror image at `:124-125`: "Do NOT touch `src/lang/5_scm_lower.rs`,
`Cargo.toml`, `Cargo.lock`, or any other `src/lang/*` file." Its out-of-scope
list at `:202-206` reads: "No change to `predicate_matches`,
`rewrite_predicates`, or `validate_predicates`. ... No new predicate operators."

Neither lane was wrong about its own job. Together they guaranteed that the
predicate sets could not converge: one lane could add operators but only to the
ast-grep side, the other could touch the native side but was told to add no
operators at all. The third lane, `TASKS/lane-fast-tier-on-scm.BRIEF.md:57`,
then closed the last door: "FORBIDDEN: `5_scm_lower.rs`, `1_ast_rule.rs` (if L1
lacks a predicate you need, `Boop-Status: blocked` with the predicate named)."

That lane needed what L1 lacks, and rather than stop it built a second
executor. The result is the shape in the tree today. `file_captures`
(`src/lang/7_scm_rows.rs:355-367`) runs each `.scm` twice:

```rust
let selected = lowered_spans(&name, &source, query_text)?;
let captured = native_captures(&name, lang, query_text, &source, &selected)?;
```

with the reason stated above `lowered_spans` at `:369-370`: "L1 supplies the
candidate spans. Native execution retains the capture grouping the AstRule
representation does not store." `lowered_spans` (`:371-401`) throws away the
lowered `rule` entirely and keeps only `program.utils` and
`program.constraints`, running `AstRule::Any` over the six `SPAN_LABELS`
(`:18-25`) to get a span set. `native_captures` (`:405-462`) then opens its own
`tree_sitter::Parser` at `:413-421`, reparsing a file the family path has
already parsed, builds its own `Query` and `QueryCursor` at `:426-432`, and
keeps a match only when its span capture is in the L1 set at `:451`.
`6_scm_family.rs` does the same two-step at `:29-35` and `:123-143` against the
already-parsed Kotlin tree.

Neither path goes through `2_source_query.rs`. The crate therefore holds three
independent `.scm` executors: `collect_spanned_matches` (`2_source_query.rs:208-257`),
`native_captures` (`7_scm_rows.rs:405-462`) and `project_kotlin_call`
(`6_scm_family.rs:22-119`). Three spellings of `did_exceed_match_limit()`
follow from that: an `assert!` at `6_scm_family.rs:87`, a recoverable
`ScmError::MatchLimit` at `7_scm_rows.rs:456`, and no check at all in
`collect_spanned_matches`, which is the one `ryi query` uses.

The three bundled `.scm` files carry zero predicates
(`queries/rust/scip.scm` 82 lines, `queries/typescript/scip.scm` 66,
`queries/kotlin/scip.scm` 103; `grep -n '(#' queries/*/scip.scm` is empty) and
the largest single pattern binds three captures. The entire lowering, the
`tree-sitter-tsquery` dependency and the ast-grep round trip currently serve
queries that use none of it.

## 4. What each engine has that the other lacks

Measured on this checkout through `ryi query` and through `query_ast_rule`,
not read from documentation.

### ast-grep has, native `.scm` lacks

| capability | `AstRule` spelling | why `.scm` cannot |
|---|---|---|
| ancestor at any depth | `Inside { rule, stop_by }` | nesting is depth-fixed; `tree-sitter#880` is open since 2021-01-13 with no accepted syntax |
| descendant at any depth | `Has { rule, stop_by }` | same |
| `stopBy` as a walk bound | `StopBy::End`, absent, or `StopBy::Rule` | no walk to bound |
| sibling at distance | `Follows`, `Precedes` | the `.` anchor is adjacency only |
| pattern with meta-variables | `Pattern("$A.len()")` | `.scm` is structural, not textual |
| ordinal with an `of` filter | `NthChild { position, of_rule, reverse }` | no positional operator |
| exact point range | `Range { start, end }` | no positional operator |
| negation of a whole subpattern | `Not(Box<AstRule>)` | only `!field` and `#not-eq?` |
| rewrite | `AstRuleRequest.fix`, `AstRuleMutationProposal` | queries do not produce text |

### Native `.scm` has, ast-grep lacks

Every row below was run on this checkout and returned the stated result.

| capability | `.scm` spelling | measured |
|---|---|---|
| N captures per match, each with its own span | `@a` `@b` in one pattern | `{"l":"a","r":"b"}`; `AstRule` gives one span and `captures []` |
| field names | `left: (identifier) @l` | distinguishes `a` from `b`; `AstRule` binds both to `a` (section 1, row 4) |
| anchors | `(block . (let_declaration) @first)` | `{"first":"let needle = \"ab\";"}` |
| quantifiers | `(block (_)+ @s)` | matches; `5_scm_lower.rs` drops `*`, `+`, `?` (`RESULT.md:138`) |
| alternations | `[(a) (b)]` | lowers as `Any`, works on both sides |
| wildcard | `(call_expression arguments: (arguments (_) @a))` | `{"a":"needle"}`; the lowering returns `Syntax` (`5_scm_lower.rs:590`) |
| supertypes | `(_expression/identifier) @x` | two rows; the lowering keeps only the `name` field |
| `#eq?` capture to capture or to string | `(#eq? @a @b)` | works, via `sprefa-eq?` at `2_source_query.rs:305-315` |
| `#not-eq?`, `#any-of?` | as written | work, via the binding's own text predicates |
| `#set!` | `(#set! role def)` | parsed, routed to `property_settings()`, read by nothing |

The asymmetry is clean. ast-grep's additions are all predicates over a single
node: given a node, answer a yes or no. Native tree-sitter's additions are all
about which nodes a match names and how many. A predicate can be added to a
query engine. A capture model cannot.

## 5. The design of the superset

### The target

One entry, one executor. A `.scm` file with N captures returns N spans per
match. Every native predicate works. Every ast-grep relation works as a host
predicate. `ryi query` and `ryi fast` both run through it.

### The three candidate shapes

**A. Lower to ast-grep with env passthrough.** Emit a metavariable per capture
so `get_env()` carries them all. Section 1 shows the env merge works and
section 1 also shows it binds the wrong nodes: two `Has` rules both stop at the
first matching descendant, because `AstRule` has no field operator, no anchor
and no ordinal within a parent. Making it correct means adding `Field`, `Anchor`
and a repetition operator to `AstRule` and implementing their matchers in this
crate, which is the one thing every brief in this area has forbidden
(`lane-scm-lower-astgrep.BRIEF.md:262-264`: "No reimplementation of any ast-grep
matching operator. `ast_grep_core` evaluates."). Rejected.

**B. Run native tree-sitter and apply ast-grep relations as post-filters over
the capture nodes.** Keep `QueryCursor::matches()` as the match source, so
captures, fields, anchors, quantifiers, alternations, wildcards and supertypes
are free and already proven. Register `#inside?`, `#has?`, `#precedes?`,
`#follows?`, `#nth-child?`, `#range?` and `#kind?` as host predicates through
`general_predicates()`, each naming a capture and a label, where a label is a
top-level pattern carrying a capture directly on its root: the exact convention
`lower_scm` defines at `5_scm_lower.rs:108-120` and the guide publishes at
`docs/2_scm-with-ast-grep-relations-20260920.md:101-123`. `#pattern?` keeps its
ast-grep meaning by compiling the snippet with `ast_grep_core::Pattern` and
testing the one node.

**C. The hybrid already in the tree.** Run both engines and intersect on spans.
This is what `7_scm_rows.rs:355-367` does today. It costs two executors, two
parses of the same bytes in the `7_scm_rows` path, and it still discards the
grouping it paid for: `native_captures` returns `BTreeSet<Capture>` and
`kept.extend(captures)` at `:452` loses which captures came from one match, so
`rows` (`:466-539`) rebuilds the association by span containment arithmetic.

### The recommendation: B

**Fewer lines.** B deletes `lowered_spans` in both consumers
(`7_scm_rows.rs:371-401`, 31 lines; `6_scm_family.rs:123-143`, 21 lines), the
private `Parser` in `native_captures` (`7_scm_rows.rs:413-425`, 13 lines) and
the span-intersection gate at `:446-453`, then folds `native_captures` into the
shared entry. It adds one relation evaluator. The relations reduce to span-set
lookups on a `tree_sitter::Node`: `#inside?` walks `node.parent()` upward,
`#has?` walks descendants or direct children under `"neighbor"`,
`#precedes?` and `#follows?` compare sibling spans, `#nth-child?` reads the
child index in the parent, `#range?` compares two points. Estimate 220 added
against 80 removed, with the ast-grep round trip for `.scm` gone. A cannot be
costed below "a new matcher per dropped operator", and C is the status quo.

**`did_exceed_match_limit()` keeps its semantics and gains coverage.** Today
there are two checks with two different meanings and one surface with none. B
puts one check in the shared entry, after the cursor is dropped, returning the
recoverable `MatchLimit` error that `7_scm_rows.rs:456-460` already defines. The
`assert!` at `6_scm_family.rs:87` goes, since a panic is the wrong answer to a
silently dropped match. `issues/lab-scopegraph-queries/item.md:188-191` already
demands exactly this, plus "the engine uses `matches()` or `captures()`
exclusively and a test pins which". B pins `matches()`, which is what all three
current executors use.

**One thing B must fix on the way.** `query_tree_sitter` (`2_source_query.rs:112-130`)
projects a match into `BTreeMap<String, Value>`, so a repeated capture name
collapses to its last node: `(block (_)+ @s)` returned one `s` where the match
bound several. `query_tree_sitter_spans` (`:134-157`) already keeps the full
`Vec<TreeSitterSpannedCapture>`. The superset entry is the spans one; the map
projection stays only as the `ryi query` display shape, and the quantifier case
gets a test that names the collapse.

### What breaks

| thing | effect |
|---|---|
| `tests/145_query_predicate_scope.rs` (88 lines, 5 tests) | the rule it pins, a match is gated by its own pattern's predicates only, is the superset's rule. The 5 tests move to the new entry unchanged, or stay if the new entry is `query_tree_sitter_spans` widened in place |
| `tests/144_scm_lower.rs` (252 lines, 1 snapshot test) | untouched while `5_scm_lower.rs` keeps serving the YAML and `fix` surface. Its census snapshot under `tests/fixtures/scm/` changes only if the lowering itself gains predicates, which B does not require |
| `tests/150_fast_scm_kotlin.rs`, `157`, `158`, `160`, `161` | row counts and the checked-in ratchet floors must be identical. The three bundled `.scm` files carry zero predicates, so phase 4 is byte-identical by construction; any drift is a defect in the port, not a new behaviour |
| `tests/159_fast_scm_judge.rs` | the scip-typescript agreement is downstream of the row set and holds if the rows hold |
| `tests/162`-`166` (graph views, cleave) | untouched; they consume rows, not queries |
| `docs/2_scm-with-ast-grep-relations-20260920.md` | the predicate table at `:84-97` gains the native family and the `#set!` rule. The "accepted but not enforced" table at `:434-447` empties: fields, quantifiers and supertypes all hold. The wildcard refusal at `:447` and `:456` goes. "Which node is reported" at `:296-311` is replaced by "every capture is reported", and the `FocusConflict` row at `:463` goes with it. The guide's completeness claim at `:82`, "anything ast-grep can match, the `.scm` file can ask for", becomes true in both directions and should say so |
| `AstRule` and `1_ast_rule.rs` | unchanged. It keeps the YAML rule surface, `constraints`, `fix` and `AstRuleMutationProposal`, which native queries cannot produce |

## 6. The lane plan

Baseline to re-measure before phase 1, from `crates/sprefa-extract`:
`timeout 600 cargo test --features cli --no-fail-fast 2>&1 | grep -E '^test result:'`,
summed. The last recorded counts are 1005 passed across 190 binaries
(`TASKS/lane-fast-tier-on-scm.BRIEF.md:68`) and 1004 across 186
(`TASKS/lane-scm-lower-astgrep.RESULT.md:23`). Two `golden_parity` root-prefix
diffs are pre-existing.

| phase | owned files | content | counted check |
|---|---|---|---|
| 1 | `src/lang/2_source_query.rs`, `tests/167_query_predicate_surface.rs` | one entry. `did_exceed_match_limit()` moves into `collect_spanned_matches` as `MatchLimit`. `validate_predicates` names the whole native family explicitly instead of letting `#not-eq?` and `#any-of?` past unchecked. `#set!` is read through `property_settings()` and attached to the match, or refused by name | the 10 probe queries of section 4 return their stated row counts through `ryi query`; `cargo test --test 145_query_predicate_scope` 5 passed, 0 failed |
| 2 | same, `tests/168_scm_relations.rs` | label patterns plus `#inside?`, `#not-inside?`, `#has?`, `#not-has?` with the three `stopBy` spellings (`"end"` default, `"neighbor"`, a label) | the complement numbers from `TASKS/lane-scm-relations-doc.RESULT.md:48-56` reproduced through `ryi query` over `src/project.rs`: 282 inside, 839 not-inside, 1121 total, 70 with `"neighbor"` |
| 3 | same, `tests/169_scm_relations_order.rs` | `#precedes?`, `#follows?`, `#nth-child?` with `An+B`, `"reverse"` and an `of` label, `#range?`, `#kind?`, `#pattern?` bridging to `ast_grep_core::Pattern` | every row of the guide's worked examples at `docs/2_scm-*:126-432` reproduced through `ryi query` with its published match count |
| 4 | `src/lang/7_scm_rows.rs`, `src/lang/6_scm_family.rs` | both drop `lowered_spans`, the private `Parser` and the span-intersection gate, and call the one entry. `native_captures` becomes a projection over grouped matches rather than a `BTreeSet` | `tests/150`, `157`, `158`, `160`, `161`: identical row counts and identical ratchet floors; one parse per file confirmed by trace span count |
| 5 | `src/lang/5_scm_lower.rs` header, `docs/2_scm-with-ast-grep-relations-20260920.md` | the lowering keeps the YAML and `fix` surface. The guide is rewritten per the table in section 5 | `cargo run --example scm_vs_yaml --features cli -- src/project.rs` still exits 0 with equal match sets |
| 6 | one walk per commit | the conversions below | per fn: the rows before and after are byte-identical over its fixtures, and the deleted line count is named |

Every command under `timeout 10`, cargo under `timeout 600`, with
`CARGO_TARGET_DIR=$PWD/target`.

### The audit's top ten, in the order the superset unlocks them

Ranks are from `plans/2026-09-21-scm-replaceable-audit.md:88-99`. The phase
column is the earliest phase after which the query can be written.

| order | phase | rank | lines | fn | what it was waiting for |
|---|---|---|---|---|---|
| 1 | 1 | 10 | 87 | `src/lang/data/_0_source.rs::entries` | field names `key` and `value`, the wildcard under `value`, `#eq?` on a key |
| 2 | 1 | 5 | 101 | `src/lang/markdown/_0_source.rs::project_inline_links` | alternation, the supertype set for inline content, the wildcard inside `link_text` |
| 3 | 1 | 1 | 116 | `src/lang/prolog/_0_source.rs::walk_goals` | fields `left`, `right`, `operand`, `argument` on `binary_operation` and `unary_operation`, plus a supertype set |
| 4 | 1 | 7 | 90 | `src/lang/python/_0_source.rs::py_walk_imports` | field `module_name`, and a quantified capture over `dotted_name` children; the join stays a bounded Rust step |
| 5 | 1 | 8 | 90 | `src/lang/rust.rs::item_entity` | alternation over ten item kinds with the name capture reported beside the item span, which is two spans per match |
| 6 | 1 | 4 | 106 | `src/lang/rust_rename.rs::visit_item_use` | `#eq?` between a leaf capture and the renamed symbol, plus the item span reported alongside |
| 7 | 2 | 3 | 109 | `src/lang/kotlin.rs::kt_decl_edges` | `#eq?` between an enum entry name and its owner, and the owning `class_declaration` reported as a capture rather than filtered by `#inside?` |
| 8 | 2 | 2 | 116 | `src/lang/rust.rs::call_defs_in_items` | the enclosing `mod_item` and its `#[cfg(test)]` attribute reported beside each def, which is `#inside?` against a label plus that label's own span in the row |
| 9 | 3 | 9 | 88 | `src/lang/rust_type_edges.rs::item_edge_candidates` | a field ordinal, which is `#nth-child?` with an `of` label, and the ordinal returned rather than only filtered |
| 10 | 3 | 6 | 100 | `src/lang/ts.rs::scan_module_specifiers` | every specifier in one clause as a repeated capture, which needs phase 1's quantifier fix in the map projection and still keeps a bounded Rust loop over the group |

Ranks 11 and 12, `go.rs::go_edge_candidates` and
`python/_2_modules.rs::walk_imports`, follow ranks 9 and 4 respectively with no
new capability.

The audit put the reachable net saving near 4,000 lines against 11,484 walk and
walk-plus lines. Ranks 1 through 6 above, 590 lines, need nothing that
tree-sitter has not shipped for years; they are blocked today only because the
`.scm` surface in this crate routes through a rule model that reports one node.
