# `.scm` relational predicates (scm++)

Relational predicates select a captured node by its ancestors, descendants, or
named siblings. A relation names its related node either by grammar kind or by a
nested tree-sitter pattern. The query surface is tree-sitter `.scm` plus these
predicates, called scm++; ast-grep and CSS supply the reference semantics.

scm++ compiles one query into plain tree-sitter patterns, one per nesting level,
and one SQL statement. `ryii query --scmpp` runs each pattern, writes
`scmpp_capture` rows and, when a relation predicate reads them, `scmpp_node`
rows, and runs the SQL over them. scm++ itself only
compiles; ryi owns the rows, the SQLite database and the one SQL run.
`ryii query --query` and the `.scm` files the crate runs through
`hafley_scm::build` are plain tree-sitter plus `#emit!`: a relation predicate or
`#contains?` there is an error naming `ryii query --scmpp`.

Every example below runs against this file, `x.rs`:

```rust
fn host(items: &[u32]) -> usize {
    let n = items.len();
    log(n);
    let f = |x: u32| x.count_ones();
    f(1);
    n
}

fn other() -> u32 {
    host(&[1, 2]);
    log(0);
    return 7;
}
```

Each query is saved as `q.scm` and run with:

```sh
ryii query --scmpp q.scm x.rs
```

## Calls and their enclosing function

```scheme
((call_expression) @call
 (#has-ancestor? @call (function_item name: (identifier) @fn) rows: each))
```

```
{"path":"x.rs","call__start":46,"call__end":57,"call__text":"items.len()","fn__start":3,"fn__end":7,"fn__text":"host"}
{"path":"x.rs","call__start":63,"call__end":69,"call__text":"log(n)","fn__start":3,"fn__end":7,"fn__text":"host"}
{"path":"x.rs","call__start":92,"call__end":106,"call__text":"x.count_ones()","fn__start":3,"fn__end":7,"fn__text":"host"}
{"path":"x.rs","call__start":112,"call__end":116,"call__text":"f(1)","fn__start":3,"fn__end":7,"fn__text":"host"}
{"path":"x.rs","call__start":151,"call__end":164,"call__text":"host(&[1, 2])","fn__start":130,"fn__end":135,"fn__text":"other"}
{"path":"x.rs","call__start":170,"call__end":176,"call__text":"log(0)","fn__start":130,"fn__end":135,"fn__text":"other"}
```

Each row is one JSON object: `path`, then `NAME__start`, `NAME__end` and
`NAME__text` for every exported capture. `start` and `end` are byte offsets,
end exclusive. Level-0 captures are always exported; `rows: each` also exports
the captures of the nested pattern.

The remaining examples show only the `__text` columns:

```sh
ryii query --scmpp q.scm x.rs | jq -c 'with_entries(select(.key | endswith("__text")))'
```

## Relation arguments

A relation is `(#RELATION? @capture TARGET OPTIONS...)`. `@capture` is the node
the relation starts at. `TARGET` is a list of grammar kind names or one nested
pattern. Kind names can be bare symbols or quoted strings; any listed kind
satisfies the relation.

```scheme
((function_item name: (identifier) @name body: (block) @body)
 (#has? @body return_expression))
```

```
{"name__text":"other","body__text":"{\n    host(&[1, 2]);\n    log(0);\n    return 7;\n}"}
```

A nested pattern uses tree-sitter's query syntax, including fields,
alternation, anchors, quantifiers, captures, and native text predicates:

```scheme
((function_item name: (identifier) @name body: (block) @body)
 (#has? @body (call_expression function: (identifier) @callee (#eq? @callee "log"))))
```

```
{"name__text":"host","body__text":"{\n    let n = items.len();\n    log(n);\n    let f = |x: u32| x.count_ones();\n    f(1);\n    n\n}"}
{"name__text":"other","body__text":"{\n    host(&[1, 2]);\n    log(0);\n    return 7;\n}"}
```

Kinds, fields and nested patterns are validated against the grammar of each
input's language. A query naming a kind the grammar lacks skips that
language's files with a diagnostic; malformed patterns and unknown options are
errors.

## Descendants, ancestors, and parents

`#has?` tests strict descendants, named and anonymous. The default
`stopBy: end` searches the whole subtree; `stopBy: neighbor` tests direct
children only.

```scheme
((function_item name: (identifier) @name body: (block) @body)
 (#has? @body let_declaration stopBy: neighbor))
```

```
{"name__text":"host","body__text":"{\n    let n = items.len();\n    log(n);\n    let f = |x: u32| x.count_ones();\n    f(1);\n    n\n}"}
```

The CSS forms are `block:has(return_expression)` and
`block:has(> let_declaration)`.

`#has-ancestor?` tests strict ancestors up to the root; `stopBy: neighbor`
limits it to the direct parent.

```scheme
((call_expression) @call (#has-ancestor? @call closure_expression))
```

```
{"call__text":"x.count_ones()"}
```

`#has-parent?` tests only the direct parent and takes no `stopBy`.

```scheme
((call_expression) @call (#has-parent? @call expression_statement))
```

```
{"call__text":"log(n)"}
{"call__text":"f(1)"}
{"call__text":"host(&[1, 2])"}
{"call__text":"log(0)"}
```

The CSS forms are `closure_expression call_expression` and
`expression_statement > call_expression`.

## Later and earlier siblings

`#precedes?` searches later named siblings; `#follows?` searches earlier named
siblings. `stopBy: neighbor` tests exactly the adjacent named sibling; the
default searches to the end of the sibling list. Anonymous tokens are skipped;
named comments count. Searches never leave the parent.

```scheme
((let_declaration) @let (#precedes? @let expression_statement stopBy: neighbor))
```

```
{"let__text":"let n = items.len();"}
{"let__text":"let f = |x: u32| x.count_ones();"}
```

```scheme
((expression_statement) @stmt
 (#follows? @stmt (let_declaration pattern: (identifier) @var (#eq? @var "f"))))
```

```
{"stmt__text":"f(1);"}
```

The CSS forms are `let_declaration:has(+ expression_statement)` and
`let_declaration ~ expression_statement`. Direction refers to the captured
node: a node that precedes another searches forward.

## Named sibling positions

`#nth-child? @capture N` holds when the node is the Nth named child of its
parent, counting from 1. A root node has no position.

```scheme
((expression_statement) @stmt (#nth-child? @stmt 2))
```

```
{"stmt__text":"log(n);"}
{"stmt__text":"log(0);"}
```

`of TARGET` counts only the siblings matching a kind or nested pattern; the
captured node must match it too. Supertype kinds such as `_expression` expand
to their subtypes.

```scheme
((let_declaration) @let (#nth-child? @let 2 of let_declaration))
```

```
{"let__text":"let f = |x: u32| x.count_ones();"}
```

The CSS forms are `expression_statement:nth-child(2)` and
`let_declaration:nth-child(2 of let_declaration)`.

## Stopping and fields

`stopBy: (PATTERN)` ends a walk at the first node matching the stop pattern.
The stop is inclusive: a visited node is tested for the relation, then the walk
ends there if it matches the stop. The stop pattern exports no captures.

```scheme
((call_expression) @call
 (#has-ancestor? @call function_item stopBy: (closure_expression)))
```

```
{"call__text":"items.len()"}
{"call__text":"log(n)"}
{"call__text":"f(1)"}
{"call__text":"host(&[1, 2])"}
{"call__text":"log(0)"}
```

`x.count_ones()` is missing: its walk ends at the closure before it reaches
`host`. For descendants, a matching stop node ends that branch; for siblings,
it ends the walk in the selected direction.

`field: NAME` requires the step next to the related node to carry that grammar
field, as in ast-grep. The field must exist in the grammar.

```scheme
((call_expression) @call (#has? @call identifier field: function stopBy: neighbor))
```

```
{"call__text":"log(n)"}
{"call__text":"f(1)"}
{"call__text":"host(&[1, 2])"}
{"call__text":"log(0)"}
```

The CSS form is `call_expression:has(> identifier[field="function"])`.

## Negation

Every relation has a `not-` form: `#not-has?`, `#not-has-ancestor?`,
`#not-has-parent?`, `#not-precedes?`, `#not-follows?`, `#not-nth-child?`.
Negation exports no captures from the target.

```scheme
((call_expression) @call (#not-has-ancestor? @call closure_expression))
```

```
{"call__text":"items.len()"}
{"call__text":"log(n)"}
{"call__text":"f(1)"}
{"call__text":"host(&[1, 2])"}
{"call__text":"log(0)"}
```

## Captures across levels

A nested pattern can read captures of the levels around it. `#eq? @inner @outer`
compares their text, and `#match? @outer "re"` inside the nested pattern tests
an enclosing capture's text:

```scheme
((let_declaration pattern: (identifier) @var) @let
 (#precedes? @let ((expression_statement (call_expression arguments: (arguments (identifier) @arg))) (#eq? @arg @var))))
```

```
{"var__text":"n","let__text":"let n = items.len();"}
```

```scheme
((call_expression) @call
 (#has-ancestor? @call ((function_item) (#match? @call "^log"))))
```

```
{"call__text":"log(n)"}
{"call__text":"log(0)"}
```

A capture name used both inside and outside names the same node:

```scheme
((identifier) @id (#has-parent? @id (call_expression function: (identifier) @id)))
```

```
{"id__text":"log"}
{"id__text":"f"}
{"id__text":"host"}
{"id__text":"log"}
```

`#contains? @capture "literal"+` holds when the capture's text contains every
literal; `#not-contains?` is its complement.

An optional capture (`?`, `*`) can be absent from a match. `#contains?`, and
`#eq?` or `#match?` across levels, fail on an absent capture; their `not-`
forms hold. `(#not-contains? @n "9")` over
`(arguments (integer_literal)? @n)` keeps `g()`, whose `@n` is absent.
`#eq?` and `#match?` between captures of one level are tree-sitter's own
predicates and follow tree-sitter's rules.

```scheme
((call_expression) @call (#contains? @call "log"))
```

```
{"call__text":"log(n)"}
{"call__text":"log(0)"}
```

## Result rows: `rows: first`, `rows: each`, `rows: list`

`rows: first`, the default, keeps a match when any related node exists and
exports nothing from the target, so each outer match appears once:

```scheme
((call_expression) @call
 (#has-ancestor? @call (function_item name: (identifier) @fn) rows: first))
```

```
{"call__text":"items.len()"}
{"call__text":"log(n)"}
{"call__text":"x.count_ones()"}
{"call__text":"f(1)"}
{"call__text":"host(&[1, 2])"}
{"call__text":"log(0)"}
```

`rows: each` emits one row per related node and exports its captures. Levels
nest to any depth:

```scheme
((identifier) @id
 (#has-ancestor? @id
   ((closure_expression) @closure
    (#has-ancestor? @closure (function_item name: (identifier) @fn) rows: each))
   rows: each))
```

```
{"id__text":"x","closure__text":"|x: u32| x.count_ones()","fn__text":"host"}
{"id__text":"x","closure__text":"|x: u32| x.count_ones()","fn__text":"host"}
```

A capture name bound by two exported levels is an error; use two names and
`#eq?`.

`rows: list` keeps every outer match and adds one column, `NAME__list`, named
after the target's first capture. The column holds a JSON array with one
object per related node, in document order (preorder of the target's root).
Each object has the target level's `NAME__start`, `NAME__end` and
`NAME__text` keys, plus those of levels nested under it with `rows: each` or
`rows: list`. An outer match with no related node gets `[]`. Capture names
inside the array are a namespace of their own. A `rows: list` target needs at
least one capture.

Over this file:

```rust
// one
// two
#[cfg(test)]
// between
fn helper() { assert!(true) }

#[cfg(test)]
#[inline]
fn bare() { 1; }
```

```scheme
((function_item name: (identifier) @name) @fn
 (#follows? @fn ((line_comment) @comment) stopBy: (function_item) rows: list))
```

prints (`path`, `__start`, `__end` and `fn` columns of the outer level left out):

```
{"name__text":"helper","comment__list":[{"comment__start":0,"comment__end":6,"comment__text":"// one"},{"comment__start":7,"comment__end":13,"comment__text":"// two"},{"comment__start":27,"comment__end":37,"comment__text":"// between"}]}
{"name__text":"bare","comment__list":[]}
```

## Optional relations: `optional: true`

`optional: true` keeps the outer match when no node relates to it.
With `rows: each` the target level left-joins: an outer match with no
related node gives one row with `null` in the target's columns; one with
related nodes gives one row per node, as without the option. With
`rows: first` the relation holds for every outer match and exports nothing.
`rows: list` keeps every outer match already, and `optional:` with it is an
error, as it is on a `not-` relation. The target of an optional relation
holds only `rows: first` relations.

```scheme
((function_item name: (identifier) @name) @fn
 (#follows? @fn ((line_comment) @comment) stopBy: (function_item) rows: each optional: true))
```

Over the file in the `rows: list` example this prints (`__text` columns only):

```
{"name__text":"helper","fn__text":"fn helper() { assert!(true) }","comment__text":"// one"}
{"name__text":"helper","fn__text":"fn helper() { assert!(true) }","comment__text":"// two"}
{"name__text":"helper","fn__text":"fn helper() { assert!(true) }","comment__text":"// between"}
{"name__text":"bare","fn__text":"fn bare() { 1; }","comment__text":null}
```

## Quantified captures

A capture under `*` or `+` binds one node per repetition, and every exported
capture joins on its own. One match emits one row per combination of its
captures' nodes: the cartesian product, across levels when `rows: each`
exports them. Here `@i` binds 2 nodes and `@s` binds 3, so the one array
gives 6 rows:

```scheme
((array_expression (integer_literal)* @i) @a
 (#has-ancestor? @a (function_item body: (block (expression_statement)+ @s)) rows: each))
```

```
{"i__text":"1","a__text":"[1, 2]","s__text":"host(&[1, 2]);"}
{"i__text":"1","a__text":"[1, 2]","s__text":"log(0);"}
{"i__text":"1","a__text":"[1, 2]","s__text":"return 7;"}
{"i__text":"2","a__text":"[1, 2]","s__text":"host(&[1, 2]);"}
{"i__text":"2","a__text":"[1, 2]","s__text":"log(0);"}
{"i__text":"2","a__text":"[1, 2]","s__text":"return 7;"}
```

A `*` or `?` capture that binds no node contributes one row with `null` in
its columns.

## `--query`, bundled queries, and `--sqlite`

`ryii query --query TEXT` and the `.scm` files the crate runs through
`hafley_scm::build` take plain tree-sitter, its native predicates and `#emit!`.
A relation predicate or `#contains?` there stops the query before any file is
read:

```sh
ryii query --query '((call_expression) @call (#not-has-ancestor? @call closure_expression))' x.rs
```

```
query (rust): pattern 0: #not-has-ancestor? runs only under ryii query --scmpp
```

`--query` prints one JSON object per match: `path`, `line`, `end_line`, and
one key per capture name. A capture that bound one node is its text. A
capture that bound two or more nodes in the match (`*`, `+`, or a name used
twice) is an array of their texts in document order. A capture that bound no
node has no key.

```scheme
((line_comment)* @before . (function_item name: (identifier) @name))
```

```
{"before":["// one","// two"],"end_line":3,"line":1,"name":"helper","path":"x.rs"}
```

`ryii query --scmpp q.scm --sqlite db.sqlite x.rs` keeps the run and prints
no rows to stdout. Every string sits once in a dictionary table
(`scmpp_dict_path`, `scmpp_dict_kind`, `scmpp_dict_field`,
`scmpp_dict_capture`, `scmpp_dict_text`: `id`, `text`); the other tables hold
integer ids:

| table | columns |
| --- | --- |
| `scmpp_row` | the result rows, as printed without `--sqlite` |
| `scmpp_capture` | `file`, `pattern` (level), `match`, `capture`, `node` (`pre` of the captured node), `start`, `end`, `text` (none for the level root) |
| `scmpp_node` | `file`, `pre` (preorder number in the file), `last` (`pre` of the subtree's last node), `parent` (-1 at the root), `depth`, `sib` (index among named siblings; none for anonymous nodes), `idx` (index among all siblings), `kind`, `field` (0 for none), `start`, `end`, `named` |

`scmpp_node` holds every node, named and anonymous, and is written only when
the query has a relation predicate, its one reader. A descendant of node `n`
is a node `d` of the same file with `n.pre < d.pre <= n.last`. Without `--sqlite` the rows live in an in-memory database for
the run and print to stdout.

`--timeout SECS` bounds the whole `--scmpp` run, file reads and the SQL: past
it the SQL is interrupted, no rows print, and `ryii query` exits 3, as
`ryii graph --timeout` does.

## Reference

| Predicate | Reach | CSS form for target `A` and related node `B` |
| --- | --- | --- |
| `#has? @a B` | Strict descendants | `A:has(B)` |
| `#has? @a B stopBy: neighbor` | Direct children | `A:has(> B)` |
| `#has-ancestor? @a B` | Strict ancestors | `B A` |
| `#has-ancestor? @a B stopBy: neighbor` | Direct parent | `B > A` |
| `#has-parent? @a B` | Direct parent | `B > A` |
| `#precedes? @a B stopBy: neighbor` | Next named sibling | `A:has(+ B)` |
| `#precedes? @a B` | Later named siblings | `A:has(~ B)` |
| `#follows? @a B stopBy: neighbor` | Previous named sibling | `B + A` |
| `#follows? @a B` | Earlier named siblings | `B ~ A` |
| `#nth-child? @a N` | Named sibling position | `A:nth-child(N)` |
| `#nth-child? @a N of B` | Position after filtering | `A:nth-child(N of B)` |

| Option | Values | Applies to |
| --- | --- | --- |
| `stopBy:` | `end` (default), `neighbor`, `(PATTERN)` | has, has-ancestor, precedes, follows |
| `field:` | a grammar field name | has, has-ancestor, has-parent, precedes, follows |
| `rows:` | `first` (default), `each`, `list` | has, has-ancestor, has-parent, precedes, follows |
| `optional:` | `false` (default), `true` | has, has-ancestor, has-parent, precedes, follows; not with `rows: list` or `not-` |

CSS comparisons apply to named nodes. See the
[ast-grep relation reference](https://ast-grep.github.io/reference/rule) for
the sibling directions and inclusive stop semantics.
