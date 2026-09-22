# `.scm` queries with ast-grep relations

Tree-sitter queries (`.scm` files) can describe a node and its children. They cannot look upward, sideways, or count siblings. ast-grep's rule language can. This guide shows how to write ast-grep's relational rules inside an ordinary `.scm` file.

Every example below runs against this file, `x.rs`:

```rust
fn plain(name: &str) -> bool {
    let a = 1;
    drop(a);
    name.contains("ab")
}

fn wrapped(n: &str) -> bool {
    let f = || n.contains("cd");
    f()
}

impl Widget {
    fn is_empty(&self) -> bool { self.items.is_empty() }
    fn render(&self) { self.items.first(); draw(self.items.len(), 1, 2) }
}
```

## The problem

Find every `.contains(...)` call that is not inside a closure. The answer is `name.contains("ab")`, and only that.

A plain `.scm` query gets as far as the calls:

```scheme
((call_expression) @c
 (#match? @c "contains"))
```

```
name.contains("ab")
n.contains("cd")
```

Both calls match. `n.contains("cd")` is inside the closure `|| n.contains("cd")`, and nothing in the query can say "and no closure above this". A pattern describes a node and what is inside it. There is no syntax for what is around it.

ast-grep has that syntax. Its YAML rule for the same question:

```yaml
rule:
  all:
    - kind: call_expression
    - regex: contains
    - not:
        inside:
          kind: closure_expression
          stopBy: end
```

```
name.contains("ab")
```

`inside` walks up through the ancestors. `not` inverts it. `stopBy: end` means walk all the way to the root.

## The solution

The same rule, written in `.scm`:

```scheme
[(closure_expression)] @closure

((call_expression) @c
 (#match? @c "contains")
 (#not-inside? @c closure))
```

```
name.contains("ab")
```

Two things are new. A top-level pattern with a capture on it, `[(closure_expression)] @closure`, defines a name. A predicate, `#not-inside?`, refers to that name. Everything else is the `.scm` you already write.

## How it works

Tree-sitter's query parser accepts any predicate spelled `(#anything? ...)` and leaves its meaning to the program running the query. This crate defines ast-grep's operators as predicates, translates the query into ast-grep's rule tree, and lets ast-grep run it. The translation is one-to-one, so anything ast-grep can match, the `.scm` file can ask for.

| `.scm` | ast-grep YAML |
| --- | --- |
| `(function_item)` | `kind: function_item` |
| `[(function_item) (impl_item)]` | `any: [{kind: function_item}, {kind: impl_item}]` |
| `[(closure_expression)] @closure` at top level | `utils: {closure: {kind: closure_expression}}` |
| `(#inside? @c closure)` | `inside: {matches: closure, stopBy: end}` |
| `(#has? @c x)`, `(#follows? @c x)`, `(#precedes? @c x)` | `has:`, `follows:`, `precedes:` |
| `(#match? @c "^is_")` | `regex: ^is_` |
| `(#pattern? @c "$A.len()")` | `pattern: $A.len()` |
| `(#not-inside? @c closure)`, any `not-` form | `not: {inside: ...}` |
| several predicates on `@c` | `all: [...]` |
| `(#nth-child? @c "2")` | `nthChild: 2` |
| `(#range? @c "3:4" "3:23")` | `range: {start: {line: 3, column: 4}, end: {line: 3, column: 23}}` |
| `(#pattern? @c "$A.len()" A ident)` | `pattern: $A.len()` plus `constraints: {A: {matches: ident}}` |

## Naming a pattern

A top-level pattern with a capture directly on it defines a name. It reports nothing by itself.

```scheme
[(function_item) (impl_item)] @scope
[(closure_expression)] @closure
[(let_declaration)] @let
```

Any pattern without a top-level capture is a query, and the file reports its matches. A file with several such patterns reports all of them. A file with only named patterns reports every named one.

The capture has to sit directly on the top-level node. This defines `closure`:

```scheme
(closure_expression) @closure
```

This does not; `@body` is inside the pattern and names a child:

```scheme
(closure_expression body: (_) @body)
```

Each name can be defined once. Defining `@scope` twice is an error.

## Looking up: `#inside?`

Calls inside an `impl` block:

```scheme
[(impl_item)] @imp

((call_expression) @c (#inside? @c imp))
```

```
self.items.is_empty()
self.items.first()
draw(self.items.len(), 1, 2)
self.items.len()
```

The walk goes all the way up by default. `self.items.len()` is three levels below the `impl_item` and still matches.

## Looking down: `#has?`

Calls that have a field access as a direct child, which is what a method call looks like in the Rust grammar:

```scheme
[(field_expression)] @recv

((call_expression) @c (#has? @c recv "neighbor"))
```

```
name.contains("ab")
n.contains("cd")
self.items.is_empty()
self.items.first()
self.items.len()
```

`"neighbor"` restricts the walk to direct children. Without it, `#has?` would also match `draw(self.items.len(), 1, 2)`, because a field access sits somewhere below it.

## Looking sideways: `#follows?` and `#precedes?`

Calls that come after a `let` in the same block:

```scheme
[(let_declaration)] @let

((call_expression) @c (#follows? @c let))
```

```
name.contains("ab")
f()
```

`drop(a)` is not in the list. In the Rust grammar `drop(a);` is wrapped in an `expression_statement`, so the sibling of the `let` is the statement, not the call.

Calls that come before a `let`:

```scheme
[(let_declaration)] @let

((call_expression) @c (#precedes? @c let))
```

```
(no matches)
```

## How far to walk

Every relation takes an optional third argument.

```scheme
[(function_item)] @fn
[(block)] @block
[(closure_expression)] @closure
```

Walk all the way (the default, also spelled `"end"`):

```scheme
((call_expression) @c (#inside? @c fn))
```

```
drop(a)
name.contains("ab")
n.contains("cd")
f()
self.items.is_empty()
self.items.first()
draw(self.items.len(), 1, 2)
self.items.len()
```

Check only the direct parent:

```scheme
((call_expression) @c (#inside? @c block "neighbor"))
```

```
name.contains("ab")
f()
self.items.is_empty()
draw(self.items.len(), 1, 2)
```

These are the calls that sit directly in a block: tail expressions and the ones not wrapped in a statement.

Walk up until a node matching another name is reached:

```scheme
((call_expression) @c (#inside? @c closure block))
```

```
n.contains("cd")
```

Only one call reaches a closure before it reaches a block. The others hit their enclosing block first and stop.

If you have written ast-grep YAML before: there, an absent `stopBy` means `neighbor`. Here, an absent third argument means `"end"`, because "anywhere above" is the question people ask most.

## Text and shape

`#match?` tests the node's source text against a regular expression:

```scheme
((identifier) @i (#match? @i "^is_"))
```

```
is_empty
```

`#pattern?` tests the node against a code snippet. `$A` stands for any one subtree, `$$$` for any sequence:

```scheme
((call_expression) @c (#pattern? @c "$A.len()"))
```

```
self.items.len()
```

```scheme
((call_expression) @c (#pattern? @c "$R.contains($A)"))
```

```
name.contains("ab")
n.contains("cd")
```

## Negation

Prefix any predicate with `not-`:

```scheme
((call_expression) @c (#not-match? @c "contains|is_empty"))
```

```
drop(a)
f()
self.items.first()
draw(self.items.len(), 1, 2)
self.items.len()
```

## Which node is reported

A pattern can have several captures. The predicates decide which one is reported: the node carrying the capture that the predicates name.

```scheme
((function_item name: (identifier) @n)
 (#match? @n "^is_"))
```

```
is_empty
```

The reported node is the identifier, because `@n` is what the predicate constrains. The surrounding `function_item` still has to be there; an `is_empty` identifier elsewhere would not match.

All predicates in one pattern must name the same capture. A pattern with no predicate reports the whole thing.

## Putting it together

Method calls named `contains`, `first` or `len`, on a field, inside a function or impl block, outside any closure:

```scheme
[(function_item) (impl_item)] @scope
[(closure_expression)] @closure
[(field_expression)] @receiver

((call_expression
   function: (field_expression field: (field_identifier) @name)) @m
 (#inside? @m scope)
 (#not-inside? @m closure)
 (#has? @m receiver "neighbor")
 (#match? @m "contains|first|len")
 (#not-match? @m "^is_"))
```

```
name.contains("ab")
self.items.first()
self.items.len()
```

## Position among siblings: `#nth-child?`

Positions count from 1. The first call in each block:

```scheme
((call_expression) @c (#nth-child? @c "1"))
```

```
drop(a)
self.items.is_empty()
self.items.first()
self.items.len()
```

`self.items.len()` is first among the children of its argument list. `drop(a)` is first among the children of its statement.

`"reverse"` counts from the end:

```scheme
((call_expression) @c (#nth-child? @c "1" "reverse"))
```

```
drop(a)
name.contains("ab")
n.contains("cd")
f()
self.items.is_empty()
self.items.first()
draw(self.items.len(), 1, 2)
```

The CSS `An+B` form works. Every odd-positioned integer literal:

```scheme
((integer_literal) @n (#nth-child? @n "2n+1"))
```

```
2
```

A name as the third argument counts only siblings matching that name:

```scheme
[(integer_literal)] @int

((integer_literal) @n (#nth-child? @n "2" int))
```

This selects the second integer among the integer siblings, ignoring anything else in between.

## Exact position in the file: `#range?`

Two `"line:column"` points, both zero-based. The node must start and end at exactly those points:

```scheme
((call_expression) @c (#range? @c "3:4" "3:23"))
```

```
name.contains("ab")
```

This is not a window. Asking for `"0:0"` to `"20:0"` returns nothing, because no call starts at the very beginning of the file and ends on line 20. Use `#range?` to name one specific node, for example the one under a cursor.

## Constraining a placeholder

After the pattern string, `#pattern?` accepts pairs of `PLACEHOLDER name`. Each placeholder must then match the named pattern.

Require the receiver of `.len()` to be a bare identifier:

```scheme
[(identifier)] @ident

((call_expression) @c (#pattern? @c "$A.len()" A ident))
```

```
(no matches)
```

The only `.len()` call is on `self.items`, which is a field access, not an identifier. Require a field access instead:

```scheme
[(field_expression)] @field

((call_expression) @c (#pattern? @c "$A.len()" A field))
```

```
self.items.len()
```

Constraints are shared across the whole file. Binding `$A` to `ident` in one pattern and to `field` in another is an error.

## Syntax that is accepted but not enforced

Some `.scm` syntax has no equivalent in ast-grep. It is read and the node constraint underneath is kept.

| written | translated as |
| --- | --- |
| `function: (identifier)` | a direct child of kind `identifier`; the field name is not checked |
| `(identifier)+`, `(identifier)?` | a single child of kind `identifier` |
| `expression/identifier` | kind `identifier` |
| `(call_expression !arguments)` | no direct child of kind `arguments` |

The last row works only when a grammar spells the field and the node type the same way. It does for `arguments` in Rust. Check the grammar's `node-types.json` for others.

The wildcard `(_)` and `(MISSING x)` are refused.

## Errors

These are caught when the query is read:

| query | error |
| --- | --- |
| `(identifier` | `Syntax { row: 0, message: "unparsed .scm text: (identifier" }` |
| `(_) @m` | `Syntax { row: 0, message: "a wildcard node _ has no AstRule" }` |
| `((identifier) @m (#frob? @m "x"))` | `UnknownPredicate("frob?")` |
| `((identifier) @m (#inside? @m))` | `PredicateArity { operator: "inside?", got: 1 }` |
| `((identifier) @m (#inside? @m nowhere))` | `UnboundReference("nowhere")` |
| `((identifier) (#inside? @m scope))` | `UnboundReference("m")` |
| `[(a)] @s` and `[(b)] @s` | `DuplicateLabel("s")` |
| `(#inside? @m scope "sideways")` | `UnknownStopBy("sideways")` |
| `(#match? @a "x")` and `(#match? @b "y")` in one pattern | `FocusConflict { first: "a", second: "b" }` |
| `(#nth-child? @a "x")` | `BadPosition("x")` |
| `(#range? @c "1:0" "nope")` | `BadPosition("nope")` |
| `$A` bound to `s` in one pattern and `i` in another | `ConstraintConflict { metavariable: "A", first: "s", second: "i" }` |

This one is caught when the query runs, because only then is the target language known:

| query | error |
| --- | --- |
| `(function_itm)` over a Rust file | `UnknownKind { kind: "function_itm", language: "Rust" }` |

ast-grep itself matches nothing for a misspelled kind and says nothing. The check exists so a typo is an error instead of an empty result.

## Languages

The language comes from the target file's extension. Twenty-eight are available: Bash, C, C++, C#, CSS, Elixir, Go, Haskell, HTML, Java, JavaScript, JSON, Kotlin, Lua, PHP, Python, Ruby, Rust, Scala, Swift, TSX, TypeScript, YAML, and, added by this crate, Prolog, Markdown, Markdown inline, GDScript and Common Lisp. When naming a language directly, `md`, `md_inline`, `gd`, `lisp` and `cl` are accepted as aliases.

## Checking it yourself

`ryi query` runs a `.scm` query through the real binary, the real grammar, and
the crate's `.scm` engine, streaming one JSONL row per match:

```bash
cd crates/sprefa-extract
cargo run --features cli --bin ryi -- query --lang rust \
  --query '(call_expression function: (identifier) @call)' -- src/project.rs
```

```
{"call":"read_inputs_with_modules","end_line":240,"line":240}
{"call":"resolve_project_inputs","end_line":241,"line":241}
{"call":"read_inputs_with_modules","end_line":280,"line":280}
{"call":"push_raw","end_line":282,"line":282}
...
```

