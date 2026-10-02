# scm++ correlated subqueries (design, 2026-10-02)

Extends tree-sitter `.scm`: a relation predicate's argument is a nested pattern written with bare parens.
The nested pattern is a correlated subquery: it may reference the enclosing levels' captures.
Relations (`has`, `has-ancestor`, `has-parent`, `precedes`, `follows`, `nth-child`) choose where the subquery searches.

## Surface

```scheme
(function_item name: (identifier) @fn body: (_) @body)
(#has? @body
  (call_expression function: (identifier) @callee
    (#eq? @callee @fn))          ; @fn: outer capture, correlated
  rows: each)                    ; first (default) | each
```

Options after the pattern: `stopBy: neighbor|end|(pattern)`, `field: NAME`, `rows: first|each`.
Negation: the `not-` prefix only (`#not-has?` ...). A negated relation binds nothing.

## Correlation rules (decided)

| name appears | meaning |
| --- | --- |
| captured only outside, referenced inside (`@fn` in `#eq?`) | outer value passed into the subquery |
| captured both outside and inside | join on node identity (same node id) |
| captured only inside | new binding, exported unless under `not-` |
| subquery inside a subquery | sees every enclosing level, innermost first |

Text equality across levels: use two names and `#eq?`.
`rows: first` keeps the outer match once with the first inner hit (today's behaviour).
`rows: each` emits one outer row per inner match (SQL join fan-out).

## Types

```rust
enum Rel {
    Walk { walk: Walk, stop: Stop, field: Option<u16>, rows: Rows, target: Target },
    NthChild { index: u32, of: Option<Target>, rows: Rows },
    Contains(Vec<Box<[u8]>>),
}
enum Target { Kinds(Vec<u16>), Pattern(Box<SubQuery>) }
enum Stop { Neighbor, End, Pattern(Box<SubQuery>) }
enum Rows { First, Each }

struct SubQuery {
    query: tree_sitter::Query,
    root: u32,
    joins: Vec<(u32, OuterRef)>,      // inner capture == outer node (node id)
    params: Vec<(String, OuterRef)>,  // "__outer_NAME" -> outer node text
    rels: Vec<(u32, Rel, bool)>,      // (capture, relation, negated)
}
struct OuterRef { depth: u8, capture: u32 }

fn compile(lang: &Language, text: &str, outer: &[&[Box<str>]]) -> Result<SubQuery, QueryExtError>;
fn eval<'t>(rel: &Rel, node: Node<'t>, env: &Env<'t>) -> Vec<Bindings<'t>>; // First: len <= 1
```

## Compile timeline (once per query)

1. Pre-pass: one walk with a paren counter that skips strings and `;` comments. Inside each `(#...?` predicate,
   each balanced `( ... )` argument becomes `"__p{n}"`; its text goes to `nested[n]`.
2. In `nested[n]`, `@name` not captured by that subquery but captured by an enclosing level becomes `"__outer_name"`.
3. `Query::new` once per level; recurse into `nested[n]` with the outer capture tables.
Replaces the current `normalize` (tree-sitter parse errors as lexer, repeated whole-query compiles).

## Eval timeline (per match)

1. tree-sitter yields a match; push its bindings as an `Env` frame.
2. Per `(capture, rel)`: walk candidates; on each, run the subquery match, check `joins` by node id, bind `params`
   before text predicates, then its own `rels` recursively with the frame pushed.
3. `First`: stop at the first hit. `Each`: collect every hit; the outer row multiplies.
4. Merged bindings go to the match arena; `not-` keeps nothing.

## Storage

None. A `SubQuery` tree lives as long as the compiled `QueryExt`; nested queries are owned by their parent.

## Open

- `field:` meaning: field the related node sits in (code now) vs field the target sits in under it (ast-grep `inside`).
