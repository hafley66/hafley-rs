---
created: 2026-09-18
updated: 2026-09-20
type: feature
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
---

# Lab: per-language .scm query files plus a scope-tree engine, isolated crate

## Description

Rewritten 2026-09-19 after research. The original body proposed parroting three
ideas from archived upstreams. Two of those three now have measured evidence
behind them, one is dropped outright, and a fourth mechanism was found that
changes the design.

### What the research established

**1. `.scm` as the per-language extension point is the dominant pattern.**
Verified by directory listing, not code-search noise:

| project | stars | per-language query files |
| --- | --- | --- |
| zed-industries/zed | 90584 | 9+: highlights, indents, injections, outline, overrides, runnables, textobjects, brackets, debugger |
| helix-editor/helix | 46280 | 7: highlights, indents, injections, locals, rainbows, tags, textobjects |
| nvim-treesitter | 14404 | GitHub reports its primary language as "Tree-sitter Query" |

GitHub-wide file counts, 2026-09-20: `highlights.scm` 48768, `injections.scm`
31936, `folds.scm` 16192, `locals.scm` 14688, `tags.scm` 7088.

**2. `.tsg` (tree-sitter-graph) is DROPPED.** crates.io reverse dependencies:
2 crates total, both published by stack-graphs itself. `github/stack-graphs` is
archived, last push 2025-09-09. There is no ecosystem to buy into, and the
per-language cost is measured: `tree-sitter-stack-graphs-python/src/
stack-graphs.tsg` is 1377 lines for ONE language.

**3. There is no live `scip-syntax`.** `sourcegraph/sourcegraph` returns 404.
SCIP moved to the `scip-code` org (`scip` 807 stars, `scip-java` 134,
`scip-go` 71, `scip-rust` 11) and contains no `scip-syntax`.
`sourcegraph/scip-semantic` is archived since 2023-03-16. The design is
documented, the code is gone.

**4. The input format is public and mature.** Helix ships `locals.scm` per
language under an open license. The two-language proof:

```scheme
; helix runtime/queries/rust/locals.scm
[ (function_item) (struct_item) (enum_item) (union_item) (type_item)
  (trait_item) (impl_item) (closure_expression) (block) ] @local.scope
```

```scheme
; helix runtime/queries/python/locals.scm
[ (module) (function_definition) (lambda) ] @local.scope
```

Nine node kinds versus three, zero overlap in spelling, ONE capture name. The
capture name IS the normalized vocabulary, and it is a text file per language.

The convention carries a taxonomy, not a flag:

```
@local.scope
@local.reference
@local.definition.variable.parameter
@local.definition.variable.mutable
@local.definition.variable.builtin
@local.definition.namespace
@_                                   match but discard
```

### The mechanism that changes the design

`.scm` has NO relational operators. Measured against ast-grep's rule algebra:

| ast-grep operator | native `.scm`? |
| --- | --- |
| `inside` (arbitrary ancestor) | NO, depth-1 only via nesting |
| `has` (arbitrary descendant) | NO |
| `not` (whole subpattern) | NO, only `!field` and `#not-eq?` |
| `precedes` / `follows` at distance | NO, the `.` anchor is adjacency only |
| `all` / `any` | partial: `(...)` groups, `[...]` alternates |
| cross-match constraints | NO |
| arithmetic, counting, recursion | NO |

`tree-sitter/tree-sitter#880`, "Specify descendant or ancestor in query," opened
2021-01-13, is still OPEN with no accepted syntax.

The escape hatch: **tree-sitter's C library does not evaluate predicates at
all.** `query.c`'s `ts_query__parse_predicate` parses any identifier ending in
`?` or `!` generically with no special-casing, and `api.h` exposes only
`ts_query_predicates_for_pattern`, returning a flat step array for the host.

So hosts add their own and this is the sanctioned path, not a hack:

- Neovim: `#has-parent?`, `#has-ancestor?`, `#contains?`, `#lua-match?`, in
  `runtime/lua/vim/treesitter/query.lua`. Neovim-only, absent from stock
  tree-sitter, py-tree-sitter and web-tree-sitter.
- Helix: `#same-line?`, `#one-line?`, `#not-kind-eq?` for indent queries.

The Rust binding splits them for you: the text family (`#eq?`, `#match?`,
`#any-of?` and negations) is auto-evaluated, `#is?` goes to
`property_predicates()`, `#set!` to `property_settings()`, and EVERYTHING ELSE
arrives raw at `general_predicates()`.

Therefore this lab implements `#inside?`, `#has?`, `#precedes?`, `#follows?` as
host predicates in Rust, re-infusing ast-grep's relational vocabulary into the
`.scm` surface with no fork and no dependency on #880.

### Two hazards to pin before any query runs

**Match limits fail SILENTLY.** `api.h` states that an exceeded limit "silently
drops the earliest-starting in-progress match." Default is unlimited and grows
dynamically. Zed set its limit to 64 and carries an open bug,
`zed-industries/zed#22042`, where Python files with many sequential assignments
silently under-highlight. This lab must call `did_exceed_match_limit()` and
treat a true as a hard error with a named stop, never a silent partial result.

**`matches()` and `captures()` are not interchangeable.** A `*` or `+` capture
binds multiple nodes WITHIN one match. `matches()` groups them into a list;
`captures()` flattens them and destroys the grouping. Maintainers flag mixing
the two as a correctness defect. Pick one and rail it.

### The engine

The scope-tree walk is the whole job and it is small:

```
step 0  run locals.scm, get captures: scope[], definition[], reference[]
step 1  sort scopes by span, nest them into a tree
step 2  attach each definition to its innermost enclosing scope
step 3  for each reference, walk outward from its innermost scope
step 4  first scope with a matching definition wins
step 5  emit: local symbol, definition role at the def, reference role at the ref
step 6  every unresolved reference carries a reason, never silence
```

Terminates by walking to the root, a base case, not a fixpoint.

Cross-file is the only part that needs more: export nodes per blob, and the
symbol-stack idea from stack-graphs for qualified names, where `a.b()` pushes
`b` then `a` so the search satisfies `a` before `b` binds. Spelled receivers
become an edge from the binding's def into the type's member scope, so an
untyped receiver has no path. That is the existing decline law, expressed
structurally.

```rust
enum NodeKind { Root, Scope, Def(Symbol), Ref(Symbol), Push(Symbol), Pop(Symbol), Export, Import(Path) }
struct Node { id: u32, kind: NodeKind, span: Option<Span>, blob: ContentId }
struct Graph { nodes: Vec<Node>, edges: Vec<(u32, u32)> }
struct Corpus { graphs: BTreeMap<ContentId, Graph>, exports: BTreeMap<String, Vec<(ContentId, u32)>> }
fn build(lang: &str, query: &str, src: &[u8], blob: ContentId) -> Graph;
fn resolve(corpus: &Corpus, blob: ContentId, r: u32) -> Outcome;
```

### Isolation

Isolated crate `crates/sprefa-lab-scopegraph`. Depends on sprefa-extract for
parse and query only. ZERO edits under `crates/sprefa-extract/src`.

Original motivation stands: the structure audit
(`plans/reviews/2026-09-18-structure-audit-glm-REPORT.md`) found 13 function
names hand-copied across go/kotlin/rust/ts and 61% of `src/` per-language. The
`.scm` route replaces hand-written Rust per language with a vendored text file.

### Steps

| step | content | done when |
| --- | --- | --- |
| L1 | crate skeleton, host predicate plumbing through `general_predicates()` | builds, `#inside?` evaluates on a fixture |
| L2 | vendor helix `locals.scm` for kotlin, extend for defs/calls/imports the convention omits | under 200 lines of local additions |
| L3 | the scope-tree engine per the step trace | under 800 lines |
| L4 | judge against `tests/fixtures/kotlin_receivers` and `kotlin_module_resolve` vs `ryi fast` (receiver 7, module_plane 11) plus the unresolved set | zero disagreements or each explained |
| L5 | ts via vendored helix `locals.scm`, no engine change | engine diff 0 lines |
| L6 | scip ratchet over lab edges | floors hold |

### Relationship to the two modes

User-set 2026-09-19: this lab exists to decide whether `.scm` plus an engine can
REPLACE `ryi fast`. `ryi slow` stays compiler-absolute and oriented on SCIP,
CodeQL and Glean, and is out of scope here.

## Acceptance Criteria

- [ ] L1: `#inside?`, `#has?`, `#precedes?`, `#follows?` evaluate as host predicates through `general_predicates()`
- [ ] `did_exceed_match_limit()` is checked on every query run and a true is a named stop, never a silent partial
- [ ] the engine uses `matches()` or `captures()` exclusively and a test pins which
- [ ] L1-L4 on kotlin: zero unexplained disagreements with `ryi fast`
- [ ] L5 on ts with zero engine edits
- [ ] `.tsg` is not used anywhere in the crate
- [ ] vendored `.scm` files carry their upstream source and license in a header
- [ ] a written verdict: replaces `ryi fast` or does not, with the `.scm` and engine line counts against the 61% per-language baseline

## Decisions

### 2026-09-20T17:52:18Z · @chris

2026-09-20: L1 (host predicates, .scm -> AstRule) shipped, see crates/sprefa-extract/docs/2_scm-with-ast-grep-relations-20260920.md. L2-L6 (vendored helix locals.scm, scope-tree engine) untouched; no .scm file is vendored in the repo yet.
