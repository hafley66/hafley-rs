# `.scm` relational predicates

Use relational predicates to select a captured node by its ancestors, descendants,
or named siblings. A nested tree-sitter pattern can also bind a related node into
the result. The query surface extends tree-sitter `.scm`; ast-grep and CSS supply
reference semantics.

For example, select calls and capture their enclosing function names:

```scheme
((call_expression) @call
 (#has-ancestor? @call (function_item name: (identifier) @fn)))
```

The same query for TypeScript uses `function_declaration`:

```scheme
((call_expression) @call
 (#has-ancestor? @call (function_declaration name: (identifier) @fn)))
```

Each kept match contains `@call` and `@fn`. Ancestors are searched from the direct
parent toward the root, so the first matching function supplies the capture.

## Relation arguments

Every directional relation accepts either a list of grammar kind names or one
nested tree-sitter pattern:

```scheme
(#has? @body return_expression try_expression)
(#has? @body (return_expression))
(#has-ancestor? @call (function_item name: (identifier) @fn))
(#has-parent? @call (expression_statement) @statement)
```

Kind names can be bare symbols or quoted strings. A kind list means that any one
listed kind can satisfy the relation. A nested pattern uses tree-sitter's query
syntax, including fields, alternation, anchors, quantifiers, captures, and native
text predicates. Kind names and nested patterns are validated against the selected
grammar. Unknown kinds, unknown fields, and malformed patterns are errors.

Native text predicates inside a related pattern need a grouping expression:

```scheme
((call_expression) @call
 (#has-ancestor? @call
   ((function_item name: (identifier) @fn)
    (#eq? @fn "host"))))
```

## Descendants, ancestors, and parents

`#has?` tests strict descendants. `end`, the default, searches in preorder;
`neighbor` tests direct children. These walks visit named and anonymous children.
The captured node itself is excluded.

```scheme
((function_item body: (block) @body) @function
 (#has? @body return_expression end))

((function_item body: (block) @body) @function
 (#has? @body (expression_statement) neighbor))
```

The CSS forms are `block:has(return_expression)` and
`block:has(> expression_statement)`.

`#has-ancestor?` tests strict ancestors, nearest first. `neighbor` limits it to the
direct parent. `#has-parent?` always tests only the direct parent, including when
an explicit `stopBy: end` option is supplied.

```scheme
((call_expression) @call
 (#has-ancestor? @call function_item end))

((call_expression) @call
 (#has-parent? @call expression_statement))
```

The CSS forms are `function_item call_expression` and
`expression_statement > call_expression`.

## Later and earlier siblings

`#precedes?` searches later named siblings. `#follows?` searches earlier named
siblings. `neighbor` tests exactly the adjacent named sibling; `end` searches to
the end of that sibling list. Punctuation tokens are skipped, while named
comments participate in the list. Searches never cross the parent boundary.

```scheme
((expression_statement) @statement
 (#precedes? @statement expression_statement neighbor))

((expression_statement) @statement
 (#follows? @statement expression_statement end))
```

The CSS forms are `expression_statement:has(+ expression_statement)` and
`expression_statement ~ expression_statement`. Direction refers to the captured
node: a node that precedes another node searches forward; one that follows
another node searches backward.

A nested pattern can capture a later sibling's call name:

```scheme
((expression_statement) @statement
 (#precedes? @statement
   (expression_statement
     (call_expression function: (identifier) @next))
   stopBy: neighbor))
```

## Named sibling positions

`#nth-child?` takes a positive 1-based position among named siblings. The captured
node must be named and have a parent. A root node has no sibling position.

```scheme
((expression_statement) @statement
 (#nth-child? @statement 3))

((expression_statement) @statement
 (#nth-child? @statement 2 of expression_statement))
```

The CSS forms are `expression_statement:nth-child(3)` and
`expression_statement:nth-child(2 of expression_statement)`.

The optional `of` argument filters the named sibling list before counting. It can
be a kind name or a nested pattern. The captured node must pass that filter.
Captures from the selected node's `of` pattern are included in the result:

```scheme
((expression_statement) @statement
 (#nth-child? @statement 2 of
   (expression_statement
     (call_expression function: (identifier) @callee))))
```

## Stopping and fields

Directional relations accept `stopBy: neighbor`, `stopBy: end`, or a nested stop
pattern. The legacy trailing `neighbor` and `end` arguments remain supported.
A stop pattern is inclusive: test a visited node for the relation first, then stop
if it matches the stop pattern. The stop pattern exports no captures.

```scheme
((call_expression) @call
 (#has-ancestor? @call
   (function_item name: (identifier) @fn)
   stopBy: (closure_expression)))
```

This searches toward the enclosing function and stops if a closure is reached
first. For descendants, a matching stop node ends the whole preorder walk. For
siblings, it ends the walk in the selected direction. Parents remain direct.
A bounded walk has no general translation using only a single CSS combinator.

`field: name` requires the related node to occupy that field in its own parent.
The field must exist in the grammar. It applies to all directional relations.

```scheme
((expression_statement) @statement
 (#has? @statement (identifier) field: function))

((call_expression) @call
 (#has-ancestor? @call (block) field: body))
```

Their CSS forms are `expression_statement:has(identifier[field="function"])`
and `block[field="body"] call_expression`.

## Negation and related captures

Every relation has a `not-` form: `#not-has?`, `#not-has-ancestor?`,
`#not-has-parent?`, `#not-precedes?`, `#not-follows?`, and `#not-nth-child?`.
Negation complements the relation and exports no related captures.

```scheme
((call_expression) @call
 (#not-has-ancestor? @call closure_expression end))
```

A positive relation exports captures from the first matching related node in its
walk order, without creating additional result rows. Later extension predicates
can inspect those captures, and `#emit!` can use them:

```scheme
((call_expression) @call
 (#has-ancestor? @call (function_item name: (identifier) @fn))
 (#contains? @fn "host")
 (#emit! "enclosing" "call" @call "function" @fn))
```

Place predicates that consume related captures after the relation that binds
them. Native outer-query text predicates run before relation evaluation; use
native text predicates inside the nested pattern when filtering related text.

## Reference

| Predicate | Default reach | CSS form for target `A` and related node `B` |
| --- | --- | --- |
| `#has? @a B` | Strict descendants | `A:has(B)` |
| `#has? @a B neighbor` | Direct children | `A:has(> B)` |
| `#has-ancestor? @a B` | Strict ancestors | `B A` |
| `#has-ancestor? @a B neighbor` | Direct parent | `B > A` |
| `#has-parent? @a B` | Direct parent | `B > A` |
| `#precedes? @a B neighbor` | Next named sibling | `A:has(+ B)` |
| `#precedes? @a B end` | Later named siblings | `A:has(~ B)` |
| `#follows? @a B neighbor` | Previous named sibling | `B + A` |
| `#follows? @a B end` | Earlier named siblings | `B ~ A` |
| `#nth-child? @a N` | Named sibling position | `A:nth-child(N)` |
| `#nth-child? @a N of B` | Position after filtering | `A:nth-child(N of B)` |

CSS comparisons apply to named nodes. Descendant and ancestor predicates retain
support for anonymous nodes in tree-sitter patterns. See the
[ast-grep relation reference](https://ast-grep.github.io/reference/rule) for the
sibling directions and inclusive stop semantics.
