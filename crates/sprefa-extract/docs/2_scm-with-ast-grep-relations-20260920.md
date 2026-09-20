# Writing `.scm` queries with ast-grep's relational operators

## Introduction

A `.scm` query is a pattern written against a parse tree. Tree-sitter turns source code into a tree of named nodes, and a `.scm` file describes a shape of nodes in S-expression form, tagging the parts you care about with `@captures`. For example:

```scheme
(call_expression function: (identifier) @name)
```

finds every call whose callee is a plain identifier and hands you that identifier as `@name`. Editors such as Helix and Zed ship one such file per language for highlighting, folding, and scope tracking, so most people who work on language tooling have written one.

What a query cannot say is "this node, but only when it sits inside that other node". The tree-sitter query language has no ancestor or descendant operator, and the upstream request for one has been open since 2021. This guide describes an extension that adds those operators, borrowed from ast-grep, as ordinary `.scm` predicates:

```scheme
[(function_item) (impl_item)] @scope

((call_expression) @call
 (#inside? @call scope))
```

That query reports every call that sits anywhere inside a function or an `impl` block. Nothing else about `.scm` changes: the same S-expressions, the same captures, the same `#match?` you already use.

## What ast-grep's rule language offers

ast-grep matches code with a YAML rule. A rule is a small algebra over tree nodes.

**Atomic rules** pick a node on its own:

| rule | meaning |
| --- | --- |
| `kind: call_expression` | the node has this grammar type |
| `pattern: $A.len()` | the node matches a code snippet, where `$A` stands for any subtree |
| `regex: ^is_` | the node's text matches a regular expression |

**Relational rules** place a node against its neighbours:

| rule | meaning |
| --- | --- |
| `inside: {...}` | some ancestor matches the inner rule |
| `has: {...}` | some descendant matches the inner rule |
| `follows: {...}` | some earlier sibling matches the inner rule |
| `precedes: {...}` | some later sibling matches the inner rule |

Each relational rule takes a `stopBy` that says how far the walk goes: `neighbor` checks only the adjacent node, `end` walks all the way, and a nested rule walks until a node matches it.

**Composite rules** combine the others:

| rule | meaning |
| --- | --- |
| `all: [...]` | every listed rule holds |
| `any: [...]` | at least one listed rule holds |
| `not: {...}` | the inner rule does not hold |
| `matches: name` | the rule defined under `utils` as `name` holds |

`utils` is a map of named rules defined once and referred to by name, which is how a long rule is built from readable parts.

Three more filters are covered at the end of this guide: `nthChild` selects a node by its position among siblings, `range` selects the node at an exact position, and `constraints` attach a rule to a `$A` placeholder inside a pattern.

## Why `.scm` grew the same operators

Every editor, and this crate, already speaks `.scm`. Rewriting each per-language query in YAML would mean two query languages for one grammar and two files per language to keep in step.

The tree-sitter query language leaves predicates to the host program: any `(#name? ...)` is parsed and stored, never interpreted, so a host can define its own. This crate defines the ast-grep vocabulary as predicates. A `.scm` file is translated into the identical rule tree that the equivalent YAML would produce, and ast-grep evaluates it. The result is one query dialect that reaches ast-grep's full matching algebra without anyone writing a new matcher.

The translation is mechanical and one-to-one:

| in `.scm` | in ast-grep YAML |
| --- | --- |
| `(function_item)` | `kind: function_item` |
| `[(a) (b)]` | `any: [{kind: a}, {kind: b}]` |
| a top-level pattern with a label, `[...] @scope` | an entry under `utils` named `scope` |
| `(#inside? @m scope)` | `inside: {matches: scope, stopBy: end}` |
| `(#has? @m x)`, `(#follows? @m x)`, `(#precedes? @m x)` | `has:`, `follows:`, `precedes:` |
| `(#match? @m "^is_")` | `regex: ^is_` |
| `(#pattern? @m "$A.len()")` | `pattern: $A.len()` |
| `(#not-inside? @m scope)` and the other `not-` forms | `not: {inside: ...}` |
| several predicates on one capture | `all: [...]` |
| `(#nth-child? @m "2")` | `nthChild: 2` |
| `(#range? @m "3:4" "3:23")` | `range: {start: ..., end: ...}` |
| `(#pattern? @m "$A.len()" A name)` | `pattern:` plus `constraints: {A: {matches: name}}` |

## Structure of a query file

A query file has two kinds of top-level pattern.

A **labelled pattern** carries a capture directly on the top-level node. It defines a name that other patterns can refer to, and reports nothing on its own:

```scheme
[(function_item) (impl_item)] @scope
[(closure_expression)] @closure
```

An **unlabelled pattern** is what the query reports:

```scheme
((call_expression) @call
 (#inside? @call scope)
 (#not-inside? @call closure))
```

The example reports calls inside a function or impl block but outside any closure.

Rules for the file as a whole:

- One unlabelled pattern reports its matches directly. Several unlabelled patterns report the union of their matches.
- A file with only labelled patterns reports the union of all of them.
- Each label may be defined once. A second definition of the same label is an error.
- Only a capture placed directly on the top-level node is a label. A capture deeper inside a pattern names a node within it and does not define anything.

## Which node a pattern reports

An ast-grep rule reports exactly one node per match, while a `.scm` pattern can hold many captures. The predicates decide which capture is reported:

- Every predicate's first argument is the capture it constrains.
- The node that carries that capture is the reported node.
- All predicates in one pattern must name the same capture. Two different captures in one pattern is an error.
- A pattern with no predicate reports the whole pattern.

When the reported capture sits below the top of the pattern, the surrounding structure still applies. In this query the reported node is the identifier, but only identifiers that appear as the callee of a call are reported:

```scheme
((call_expression function: (identifier) @callee)
 (#match? @callee "^is_"))
```

## The predicates

### Relations: `#inside?`, `#has?`, `#follows?`, `#precedes?`

Each takes the capture to constrain, the label of the pattern it must relate to, and an optional third argument that sets how far the walk goes.

```scheme
[(function_item)] @fn
[(block)] @block

((call_expression) @c (#inside? @c fn))            ; any ancestor is a function
((call_expression) @c (#inside? @c fn "end"))      ; same as above, spelled out
((call_expression) @c (#inside? @c fn "neighbor")) ; the direct parent is a function
((call_expression) @c (#inside? @c fn block))      ; walk upward, but stop at the first block
```

| third argument | walk |
| --- | --- |
| absent | to the root, or to the leaves for `#has?`, or across every sibling |
| `"end"` | same as absent |
| `"neighbor"` | the immediately adjacent node only |
| a label | until a node matching that label is reached |

The default here is `"end"`. This differs from writing ast-grep YAML by hand, where an absent `stopBy` means `neighbor`. The default was chosen because "anywhere inside" is the question people ask most.

The third argument is a string for a walk mode and a bare identifier for a label, so a label that happens to be spelled `neighbor` is still read as a label.

### Text: `#match?`

Takes the capture and a regular expression string. Matches when the node's source text matches the expression.

```scheme
((identifier) @name (#match? @name "^is_"))
```

### Shape: `#pattern?`

Takes the capture and an ast-grep pattern string. A pattern is a snippet of code in the target language where `$A`, `$B` and so on stand for any single subtree and `$$$` stands for any sequence.

```scheme
((call_expression) @c (#pattern? @c "$A.len()"))
```

### Negation: the `not-` prefix

Any predicate can be negated by prefixing its name with `not-`:

```scheme
((call_expression) @c (#not-inside? @c closure))
((identifier) @n (#not-match? @n "^_"))
```

### Combining predicates

Several predicates on the same capture all have to hold:

```scheme
[(function_item) (impl_item)] @scope
[(closure_expression)] @closure
[(field_expression)] @receiver

((call_expression
   function: (field_expression field: (field_identifier) @name)) @m
 (#inside? @m scope)
 (#not-inside? @m closure)
 (#has? @m receiver "neighbor")
 (#match? @m "contains|starts_with|ends_with|find")
 (#not-match? @m "^is_"))
```

This reports method calls named `contains`, `starts_with`, `ends_with` or `find`, called on a field, inside a function or impl block, outside any closure.

## What is accepted but ignored

Some `.scm` syntax has no counterpart in ast-grep's rule language. The translation accepts it and keeps only the node constraint underneath:

| construct | example | effect after translation |
| --- | --- | --- |
| field selector | `function: (identifier)` | the child must exist; the field name is not checked |
| quantifiers | `(identifier)+`, `(identifier)?` | treated as a single child |
| supertype | `expression/identifier` | the underlying node type is kept |

A nested pattern always becomes a direct-child check. `(call_expression (identifier))` means "a call with an identifier as a direct child".

A negated field such as `(call_expression !arguments)` is translated as "has no direct child of kind `arguments`". This is correct only when the grammar uses the same spelling for the field and the node type, which is true for `arguments` in Rust. Check your grammar's `node-types.json` before relying on it.

Two constructs are refused outright: the wildcard `(_)` and `(MISSING x)`.

## Errors

Errors come in two groups. The first group is found when the query file is read, before any source is examined:

| what you wrote | message |
| --- | --- |
| an unclosed pattern such as `(identifier` | `Syntax { row: 0, message: "unparsed .scm text: (identifier" }` |
| `(_)` | `Syntax { row: 0, message: "a wildcard node _ has no AstRule" }` |
| `(#frob? @m "x")` | `UnknownPredicate("frob?")` |
| `(#inside? @m)` with too few arguments | `PredicateArity { operator: "inside?", got: 1 }` |
| `(#inside? @m nowhere)` where no pattern is labelled `@nowhere` | `UnboundReference("nowhere")` |
| a predicate naming a capture the pattern never binds | `UnboundReference("m")` |
| two top-level patterns both labelled `@scope` | `DuplicateLabel("scope")` |
| `(#inside? @m scope "sideways")` | `UnknownStopBy("sideways")` |
| two predicates in one pattern naming `@a` and `@b` | `FocusConflict { first: "a", second: "b" }` |

`row` is zero-based.

The second group is found when the query runs against a file, because only then is the target language known:

| what you wrote | message |
| --- | --- |
| `(function_itm)`, a node type the grammar does not have | `UnknownKind { kind: "function_itm", language: "Rust" }` |

This check exists because ast-grep itself matches nothing for an unknown node type and reports no error. The mistyped kind would otherwise fail silently.

## Worked example: the same query in `.scm` and in YAML

The crate ships an example program that holds one `.scm` query and its hand-written YAML twin, translates the first, reads the second, and confirms that both produce the same rule and the same matches over one file.

The `.scm` side is the combined query from the previous section. The YAML side is:

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

Run it:

```bash
cd crates/sprefa-extract
cargo run --example scm_vs_yaml --features cli -- src/project.rs
```

The output ends with:

```text
rule trees equal: true
utils equal:      true
matches .scm:     16
matches yaml:     16
match sets equal: true
```

## Checking containment

`#inside?` and `#not-inside?` with the same label split a match set in two. If containment works at every depth, the two counts sum to the unfiltered count. Over one 108 KB Rust file:

| query | matches |
| --- | --- |
| every `call_expression` | 1121 |
| `(#inside? @m closure)` | 282 |
| `(#not-inside? @m closure)` | 839 |
| `(#inside? @m closure "neighbor")` | 70 |

282 plus 839 is 1121. The `"neighbor"` mode finds the 70 calls whose direct parent is a closure; the default mode finds the 282 calls with a closure anywhere above them.

## Supported languages

The query file itself is language-independent. The language is chosen from the target file's extension when the query runs. Twenty-eight grammars are available:

| group | languages |
| --- | --- |
| bundled by ast-grep | Bash, C, C++, C#, CSS, Elixir, Go, Haskell, HTML, Java, JavaScript, JSON, Kotlin, Lua, PHP, Python, Ruby, Rust, Scala, Swift, TSX, TypeScript, YAML |
| added by this crate | Prolog, Markdown, Markdown inline, GDScript, Common Lisp |

When a language is named directly instead of inferred from a path, the aliases `md`, `md_inline`, `gd`, `lisp` and `cl` are accepted.

## Position, range and placeholder constraints

Three more ast-grep filters are available. They are the ones reached for as soon as a plain pattern returns too much.

### Position among siblings: `#nth-child?`

Selects a node by its index among its siblings: the first argument of a call, the last statement of a block. Positions are 1-based and may use the CSS `An+B` form. An optional label counts only siblings matching that pattern, and the string `"reverse"` counts from the end.

```scheme
[(argument)] @arg

((argument) @a (#nth-child? @a "2"))                 ; the second sibling
((argument) @a (#nth-child? @a "2n+1"))              ; every odd sibling
((argument) @a (#nth-child? @a "1" arg "reverse"))   ; the last sibling that is an argument
```

In YAML this is `nthChild: 2`, or `nthChild: {position: 2, ofRule: {kind: argument}, reverse: true}`.

### Exact position in the file: `#range?`

Selects the node that starts and ends at exactly the given points, written as `"line:column"` with both numbers zero-based. This is how a rule targets one specific expression, for example the node under a cursor or the one a diff hunk names, rather than a region.

```scheme
((call_expression) @c (#range? @c "3:4" "3:23"))
```

In YAML this is `range: {start: {line: 3, column: 4}, end: {line: 3, column: 23}}`.

### Constraining a placeholder: `#pattern?` with pairs

A pattern's `$A` placeholders match any subtree. Trailing pairs of `PLACEHOLDER label` after the pattern string require each named placeholder to match a labelled pattern:

```scheme
[(identifier)] @name

((call_expression) @c (#pattern? @c "$A.len()" A name))
```

Now `$A` must be a bare identifier, so `foo().len()` no longer matches. In YAML this is `pattern: $A.len()` with `constraints: {A: {kind: identifier}}`.

Constraints are keyed by placeholder name across the whole file. Binding the same placeholder to two different labels in two patterns is an error:

| what you wrote | message |
| --- | --- |
| `(#nth-child? @a "x")` | `BadPosition("x")` |
| `(#range? @c "1:0" "nope")` | `BadPosition("nope")` |
| `$A` bound to `@s` in one pattern and `@i` in another | `ConstraintConflict { metavariable: "A", first: "s", second: "i" }` |
