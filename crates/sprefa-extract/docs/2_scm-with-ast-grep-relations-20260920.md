# Writing `.scm` queries that use ast-grep's relational operators

Date: 2026-09-20. Receipts are `file:line` inside `crates/sprefa-extract/` at commit `22fdedaa`. Every number comes from a command run on that commit.

## What this surface is

`.scm` gains ast-grep's relational operators without anyone implementing a relational operator. `lower_scm` at `src/lang/5_scm_lower.rs:84` parses a query file and rewrites it into an `AstRule` tree. `ast_grep_core` evaluates that tree. The module header at `src/lang/5_scm_lower.rs:1` states the division: containment, sibling order and regex all stay with ast-grep.

This guide assumes that you write `highlights.scm` or `locals.scm` for Helix or Zed. It assumes nothing about ast-grep or about this crate.

## Terms a `.scm` author does not have yet

| term | meaning | where |
| --- | --- | --- |
| `AstRule` | The rule tree that ast-grep evaluates. It has eleven variants: `Pattern`, `Kind`, `Regex`, `Matches`, `All`, `Any`, `Not`, `Inside`, `Has`, `Follows`, `Precedes`. | `src/lang/1_ast_rule.rs:21` |
| `Kind("x")` | The node has the grammar node type `x`. `(function_item)` lowers to `Kind("function_item")`. | `src/lang/1_ast_rule.rs:21` |
| `Matches("name")` | A reference to another rule by its id. The node satisfies `Matches("scope")` when it satisfies the rule stored under `scope`. ast-grep YAML spells it `matches: scope`. | `src/lang/1_ast_rule.rs:25` |
| `utils` | The list of named rules that `Matches` can refer to. Each entry is a `NamedAstRule { id, rule }`. | `src/lang/5_scm_lower.rs:51` |
| `Inside`, `Has`, `Follows`, `Precedes` | The four relational operators. Each holds one inner rule and one optional `stop_by`. `Inside` tests an ancestor, `Has` tests a descendant, `Follows` and `Precedes` test siblings. | `src/lang/1_ast_rule.rs:21` |
| `StopBy` | Where the relational walk stops. `End("end")` walks to the root or to the leaves. `Rule(r)` walks until a node satisfies `r`. An absent `stop_by` tests the immediate neighbor only. | `src/lang/1_ast_rule.rs:53` |
| `ScmProgram` | The output of `lower_scm`: one `rule` that reports matches, plus `utils`. | `src/lang/5_scm_lower.rs:51` |

## Why upstream `.scm` has no relations

- Upstream `.scm` has no relational operator. The issue `tree-sitter#880`, "Specify descendant or ancestor in query", is open since 2021-01-13.
- The tree-sitter C library evaluates no predicate. `ts_query__parse_predicate` in `query.c` accepts any identifier that ends in `?` or `!` and stores it. The host program decides what each predicate means.
- A predicate argument is flat. The Rust runtime type `tree_sitter::QueryPredicateArg` has two variants, `Capture(u32)` and `String(Box<str>)`. The query grammar, tree-sitter-tsquery 0.8.0, lists the children of `parameters` as `capture`, `identifier` and `string` in `node-types.json`. Neither form can hold a nested rule.

Those three facts shape the surface. A host is free to define `#inside?`. The argument of `#inside?` cannot be a pattern, so it is a name.

## Split a file into named rules and a reporting rule

`lower_scm` sorts the top-level patterns of a file into two groups at `src/lang/5_scm_lower.rs:100`.

| top-level pattern | becomes |
| --- | --- |
| carries a direct `@label` | one `utils` entry whose id is the label |
| carries no direct label | part of the rule that reports matches |

Only a capture that is a direct child of the top-level pattern is a label. A deeper capture names a node inside the pattern (`src/lang/5_scm_lower.rs:202`).

The reporting rule depends on the count of unlabelled patterns (`src/lang/5_scm_lower.rs:126`).

| unlabelled patterns | reporting rule | lowered output |
| --- | --- | --- |
| one | that pattern | `(function_item)` gives `Kind("function_item")` |
| several | `Any` over them | `(function_item)` and `(impl_item)` give `Any([Kind("function_item"), Kind("impl_item")])` |
| none | `Any` over every label | labels `@scope` and `@wall` give `Any([Matches("scope"), Matches("wall")])` |

A labelled pattern reports nothing on its own when an unlabelled pattern exists. It is a definition that other patterns refer to.

A bracketed list lowers to `Any`. `[(function_item)] @scope` gives the `utils` entry `scope` with the rule `Any([Kind("function_item")])`.

## Choose the node a pattern reports

An ast-grep rule reports exactly one node per match. A `.scm` pattern can hold many captures. The predicates decide which capture is the reported node (`src/lang/5_scm_lower.rs:226`).

- Every predicate names the capture it constrains as its first argument.
- The node that carries that capture is the reported node.
- All predicates in one pattern must name the same capture. Two different captures give `FocusConflict`.
- A pattern with no predicate reports the whole pattern.

When the reported node sits below the top of the pattern, the lowering adds the enclosing pattern as an `Inside` constraint with `stop_by: Some(End("end"))` (`src/lang/5_scm_lower.rs:254`). The reported node then matches only inside the structure that you wrote around it.

## Pick a predicate

Each row shows one complete pattern and the rule that `lower_scm` returns for it. The rows with `scope` assume the file also holds `[(function_item)] @scope`. The dispatch is at `src/lang/5_scm_lower.rs:352`.

| predicate | one line of `.scm` | lowered reporting rule |
| --- | --- | --- |
| `#inside?` | `((identifier) @m (#inside? @m scope))` | `All([Kind("identifier"), Inside { rule: Matches("scope"), stop_by: Some(End("end")) }])` |
| `#has?` | `((identifier) @m (#has? @m scope))` | `All([Kind("identifier"), Has { rule: Matches("scope"), stop_by: Some(End("end")) }])` |
| `#follows?` | `((identifier) @m (#follows? @m scope))` | `All([Kind("identifier"), Follows { rule: Matches("scope"), stop_by: Some(End("end")) }])` |
| `#precedes?` | `((identifier) @m (#precedes? @m scope))` | `All([Kind("identifier"), Precedes { rule: Matches("scope"), stop_by: Some(End("end")) }])` |
| `#match?` | `((identifier) @m (#match? @m "^is_"))` | `All([Kind("identifier"), Regex("^is_")])` |
| `#pattern?` | `((identifier) @m (#pattern? @m "$A.len()"))` | `All([Kind("identifier"), Pattern("$A.len()")])` |
| `#not-` prefix | `((identifier) @m (#not-inside? @m scope))` | `All([Kind("identifier"), Not(Inside { rule: Matches("scope"), stop_by: Some(End("end")) })])` |

Argument rules:

- The first argument is always a capture.
- The second argument of a relation is an identifier that names a top-level label.
- The second argument of `#match?` and `#pattern?` is a string. `#match?` takes a regex. `#pattern?` takes an ast-grep pattern, where `$A` is a metavariable.
- The `not-` prefix applies to all six predicates. It wraps the lowered rule in `Not` (`src/lang/5_scm_lower.rs:343`).
- Every predicate takes two or three arguments (`src/lang/5_scm_lower.rs:334`). Any other count is `PredicateArity`.

## Refer to another pattern by name

Write the inner rule as a labelled top-level pattern. Pass its label to the predicate as a bare identifier.

```scheme
[(function_item) (impl_item)] @scope

((call_expression) @m
 (#inside? @m scope))
```

The reporting rule lowers to:

```text
All([Kind("call_expression"), Inside { rule: Matches("scope"), stop_by: Some(End("end")) }])
```

The `utils` list holds `scope` with the rule `Any([Kind("function_item"), Kind("impl_item")])`. ast-grep resolves `Matches("scope")` against that list when it evaluates the rule.

The reference is a name because a predicate argument cannot be a nested rule. The runtime type allows a capture or a string. The query grammar allows a capture, an identifier or a string. The comment at `src/lang/5_scm_lower.rs:310` records that constraint, and `src/lang/5_scm_lower.rs:371` reads the identifier as a label reference.

An identifier that names no top-level label is `UnboundReference`.

## Set where the walk stops

A relation takes an optional third argument. Its node type decides its meaning (`src/lang/5_scm_lower.rs:372`).

| third argument | example | lowered `stop_by` | walk |
| --- | --- | --- | --- |
| absent | `(#inside? @m scope)` | `Some(End("end"))` | every ancestor up to the root |
| string `"end"` | `(#inside? @m scope "end")` | `Some(End("end"))` | every ancestor up to the root |
| string `"neighbor"` | `(#inside? @m scope "neighbor")` | `None` | the direct parent only |
| identifier | `(#inside? @m scope wall)` | `Some(Rule(Matches("wall")))` | upward until a node satisfies `wall` |
| any other string | `(#inside? @m scope "sideways")` | error `UnknownStopBy("sideways")` | none |

The table describes `Inside`. `Has` walks down through descendants. `Follows` and `Precedes` walk across siblings.

The default differs from ast-grep YAML. YAML with no `stopBy` tests the neighbor only. This surface with no third argument walks to the end.

The string and identifier typing exists so that a label spelled `neighbor` stays a label. With `[(block)] @neighbor` in the file, `(#inside? @m scope neighbor)` lowers to `stop_by: Some(Rule(Matches("neighbor")))`. The quoted form `"neighbor"` is the walk mode.

## Know what the surface drops

The `.scm` grammar carries constructs that `AstRule` has no operator for (`src/lang/5_scm_lower.rs:33`). The lowering parses them and discards them. It keeps the node constraint underneath.

| construct | example | what the lowering keeps |
| --- | --- | --- |
| field selector | `function: (identifier)` | `Has { rule: Kind("identifier"), stop_by: None }` |
| quantifier `*`, `+`, `?` | `(identifier)+` | `Kind("identifier")` |
| supertype | `expression/identifier` | the node constraint underneath |

`(call_expression function: (identifier)+)` lowers to `All([Kind("call_expression"), Has { rule: Kind("identifier"), stop_by: None }])`. The field name and the quantifier leave no trace. A child pattern always lowers to `Has` with no `stop_by`, which tests direct children.

Two constructs are refused with `Syntax`: the wildcard `(_)` and `(MISSING x)`.

A negated field lowers its field name as a kind. `(call_expression !arguments)` lowers to `All([Kind("call_expression"), Not(Has { rule: Kind("arguments"), stop_by: None })])`. That rule is correct only where the grammar spells the field and the node type the same way. The Rust grammar does so for `arguments`. Check `node-types.json` of your grammar before you rely on a negated field.

## Read every refusal

Two error types exist. `ScmLowerError` fires at lower time, inside `lower_scm`, before any source file is read. `AstRuleError` fires at run time, inside `query_ast_rule`, when the rule meets a target grammar. Both print their `Debug` form, so the message is the variant text.

The seven `ScmLowerError` variants are at `src/lang/5_scm_lower.rs:57`.

| variant | input that produces it | literal message |
| --- | --- | --- |
| `Syntax` | `(identifier` | `` Syntax { row: 0, message: "unparsed `.scm` text: (identifier" } `` |
| `Syntax` | `(_)` | `` Syntax { row: 0, message: "a wildcard node `_` has no AstRule" } `` |
| `Syntax` | `(MISSING identifier)` | `Syntax { row: 0, message: "missing_node has no AstRule" }` |
| `UnknownPredicate` | `((identifier) @m (#frob? @m "x"))` | `UnknownPredicate("frob?")` |
| `UnknownPredicate` | `((identifier) @m (#not-frob? @m "x"))` | `UnknownPredicate("not-frob?")` |
| `PredicateArity` | `((identifier) @m (#inside? @m))` | `PredicateArity { operator: "inside?", got: 1 }` |
| `UnboundReference` | `((identifier) @m (#inside? @m nowhere))` with no `@nowhere` label | `UnboundReference("nowhere")` |
| `UnboundReference` | `((identifier) (#inside? @m scope))`, where the pattern never binds `@m` | `UnboundReference("m")` |
| `DuplicateLabel` | `[(a)] @scope` and `[(b)] @scope` in one file | `DuplicateLabel("scope")` |
| `UnknownStopBy` | `((identifier) @m (#inside? @m scope "sideways"))` | `UnknownStopBy("sideways")` |
| `FocusConflict` | `((call_expression (identifier) @a) @b (#match? @a "x") (#match? @b "y"))` | `FocusConflict { first: "a", second: "b" }` |

`row` is zero-based. `UnboundReference` also covers an argument of the wrong node type, such as a string where a capture belongs.

`lower_scm` never checks a node kind against a grammar, because it does not know the target language. That check is `AstRuleError::UnknownKind { kind, language }` at `src/lang/1_ast_rule.rs:182`. `query_ast_rule` runs it at `src/lang/1_ast_rule.rs:321`, before ast-grep sees the rule.

| variant | input that produces it | literal message |
| --- | --- | --- |
| `AstRuleError::UnknownKind` | `(function_itm)` run over `src/project.rs` | `UnknownKind { kind: "function_itm", language: "Rust" }` |

The check exists because ast-grep matches nothing for an unknown kind and reports no error (`src/lang/1_ast_rule.rs:180`).

## Run the same rule from `.scm` and from YAML

`examples/scm_vs_yaml.rs` holds one `.scm` query and one hand-written ast-grep YAML twin. It lowers the first, decodes the second, compares the two rule trees, and runs both over one file. It exits nonzero when anything differs.

The `.scm` side:

```scheme
[(closure_expression)] @closure
[(function_item) (impl_item)] @scope
[(field_expression)] @receiver

((call_expression
   function: (field_expression field: (field_identifier) @name)) @m
 (#inside? @m scope)
 (#not-inside? @m closure)
 (#has? @m receiver "neighbor")
 (#match? @m "contains|starts_with|ends_with|find")
 (#not-match? @m "^is_"))
```

The YAML side:

```yaml
id: twin
utils:
  closure:
    any: [{kind: closure_expression}]
  scope:
    any: [{kind: function_item}, {kind: impl_item}]
  receiver:
    any: [{kind: field_expression}]
rule:
  all:
    - all:
        - kind: call_expression
        - has:
            all:
              - kind: field_expression
              - has: {kind: field_identifier}
    - inside: {matches: scope, stopBy: end}
    - not: {inside: {matches: closure, stopBy: end}}
    - has: {matches: receiver}
    - regex: contains|starts_with|ends_with|find
    - not: {regex: "^is_"}
```

Line by line:

| `.scm` | YAML |
| --- | --- |
| `[(closure_expression)] @closure` | `utils.closure: any: [{kind: closure_expression}]` |
| `(call_expression function: (field_expression field: (field_identifier) @name)) @m` | the first `all` member: `kind` plus nested `has` |
| `(#inside? @m scope)` | `inside: {matches: scope, stopBy: end}` |
| `(#not-inside? @m closure)` | `not: {inside: {matches: closure, stopBy: end}}` |
| `(#has? @m receiver "neighbor")` | `has: {matches: receiver}` |
| `(#match? @m "contains\|starts_with\|ends_with\|find")` | `regex: contains\|starts_with\|ends_with\|find` |
| `(#not-match? @m "^is_")` | `not: {regex: "^is_"}` |

The field selectors `function:` and `field:` and the inner capture `@name` have no YAML counterpart. The lowering drops them.

Run it:

```bash
cd crates/sprefa-extract
cargo run --example scm_vs_yaml --features cli -- src/project.rs
```

Output, exit status 0:

```text
target src/project.rs, 108887 bytes

rule from .scm
  All([All([Kind("call_expression"), Has { rule: All([Kind("field_expression"), Has { rule: Kind("field_identifier"), stop_by: None }]), stop_by: None }]), Inside { rule: Matches("scope"), stop_by: Some(End("end")) }, Not(Inside { rule: Matches("closure"), stop_by: Some(End("end")) }), Has { rule: Matches("receiver"), stop_by: None }, Regex("contains|starts_with|ends_with|find"), Not(Regex("^is_"))])

rule from yaml
  All([All([Kind("call_expression"), Has { rule: All([Kind("field_expression"), Has { rule: Kind("field_identifier"), stop_by: None }]), stop_by: None }]), Inside { rule: Matches("scope"), stop_by: Some(End("end")) }, Not(Inside { rule: Matches("closure"), stop_by: Some(End("end")) }), Has { rule: Matches("receiver"), stop_by: None }, Regex("contains|starts_with|ends_with|find"), Not(Regex("^is_"))])

rule trees equal: true
utils equal:      true
matches .scm:     16
matches yaml:     16
match sets equal: true

    25753..25782   named.contains(&row.relation)
    29255..29952   witnesses.extend(legs.into_iter().map(|leg| {
    29272..29951   legs.into_iter().map(|leg| {
    30268..30315   semantic.iter().find(|(lang, _)| *lang == "ts")
    30498..30545   semantic.iter().find(|(lang, _)| *lang == "go")
    30728..30777   semantic.iter().find(|(lang, _)| *lang == "rust")
    33185..33260   inputs
    33185..33463   inputs
    33185..33482   inputs
    35603..35828   inputs
    35603..36031   inputs
    35603..36050   inputs
  ... 4 more

1-1
```

The two notations produce one rule tree and the same 16 matches over a 108887-byte file.

## Check containment with a complement

`#inside?` and `#not-inside?` with the same label split a match set into two parts. A call that sits several levels below a closure must land in the `#inside?` part. When the two counts sum to the unfiltered count, containment holds at every depth.

Four queries over `src/project.rs`. Each file starts with `[(closure_expression)] @closure`.

| reporting pattern | matches |
| --- | --- |
| `((call_expression) @m (#match? @m ""))` | 1121 |
| `((call_expression) @m (#inside? @m closure))` | 282 |
| `((call_expression) @m (#not-inside? @m closure))` | 839 |
| sum of the two filtered counts | 1121 |
| `((call_expression) @m (#inside? @m closure "neighbor"))` | 70 |

282 plus 839 equals 1121, the unfiltered count. The `"neighbor"` mode finds 70 calls whose direct parent is a closure. The default mode finds 282 calls with a closure anywhere above them.

The first row uses an empty regex, which matches every node text. Each file holds one unlabelled pattern, so the label `closure` reports nothing on its own.

The numbers come from a throwaway example that calls `lower_scm` and then `query_ast_rule`, the same two calls as `examples/scm_vs_yaml.rs`. The example is deleted.

## Languages the query can target

`lower_scm` is independent of the target language. The language enters when the rule runs.

- `query_ast_rule` picks the grammar from the file path through `RyiLang::from_path` (`src/lang/extract_lang.rs:36`).
- The native tree-sitter query path uses `query_language` at `src/lang/2_source_query.rs:161`. It routes a language name through `RyiLang::parse_name` at `src/lang/extract_lang.rs:54`.

`RyiLang` at `src/lang/extract_lang.rs:23` holds 28 languages, counted from source:

| group | count | members |
| --- | --- | --- |
| grammars this crate links itself | 5 | `prolog`, `markdown`, `markdown_inline`, `gdscript`, `commonlisp` |
| `SupportLang::all_langs()` in `ast-grep-language` 0.38.7 | 23 | Bash, C, Cpp, CSharp, Css, Elixir, Go, Haskell, Html, Java, JavaScript, Json, Kotlin, Lua, Php, Python, Ruby, Rust, Scala, Swift, Tsx, TypeScript, Yaml |

`parse_name` also accepts the aliases `md`, `md_inline`, `gd`, `lisp` and `cl`. A name that `parse_name` rejects gives `unknown lang '<name>'`. A kind that the chosen grammar does not spell gives `UnknownKind`.
