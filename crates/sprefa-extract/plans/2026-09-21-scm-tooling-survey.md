# Survey: who already runs `.scm` with predicates beyond the built-in four

Survey date 2026-09-21, worktree `main-codex-attribution` at `426dbc5c`.
Companion to `plans/2026-09-21-scm-superset.md`, which established that this
crate needs N captures per match, ancestor/descendant/sibling predicates, and a
scope layer over captures. The question this answers: does something already
exist that we can take instead of building it.

Short answer: no single project covers it. Two maintained pieces cover most of
the ground, and the relation predicates themselves are nobody's published
library anywhere, in any language.

## 1. The structural fact that shapes every answer

Tree-sitter's C library does not evaluate predicates at all. From the upstream
guide: "Predicates and directives are not handled directly by the Tree-sitter C
library. They are just exposed in a structured form so that higher-level code
can perform the filtering." The bindings then each implement a small set on
their own, and the documented set is `#eq?`, `#match?`, `#any-of?`, `#is?`,
`#is-not?`, `#set!`, `#select-adjacent!`, `#strip!`, plus the `not-` and `any-`
cross products of the first two families.

Everything past that is per host. The Rust binding hands unrecognized operators
back through `Query::general_predicates(pattern_index)` as
`QueryPredicate { operator: Box<str>, args: Vec<QueryPredicateArg> }`, where
each arg is a capture index or a string. That is the whole extension point, and
it is the same one every project below uses. There is no upstream RFC to
standardize `#inside?` or `#has-ancestor?`; the docs say only "in the future,
more standard predicates and directives may be added."

So the relation predicates are not a library we can install. They are a match
loop we write against `general_predicates()`. The survey question therefore
narrows to: whose predicate names and semantics do we copy, and whose scope
layer do we reuse rather than invent.

- https://tree-sitter.github.io/tree-sitter/using-parsers/queries/3-predicates-and-directives.html
- https://docs.rs/tree-sitter/latest/tree_sitter/struct.Query.html
- https://github.com/tree-sitter/tree-sitter/issues/880

## 2. Candidate table

| Candidate | Lang | Maintained | Runs `.scm`, N captures | Extra predicates | API | License | Verdict |
|---|---|---|---|---|---|---|---|
| `tree-sitter-tags` | Rust | yes, 0.27.0 2026-08-30 | yes | `#select-adjacent!`, `#strip!`, `#is-not? local`, `#set! local.scope-inherits` | Rust + C | MIT | adopt |
| `tree-sitter-highlight` | Rust | yes, 0.27.0 2026-08-30 | yes | none new; reads `#is-not? local`, `#set!` | Rust + C | MIT | borrow the locals algorithm |
| Neovim core `vim.treesitter.query` | Lua | yes | yes | `has-ancestor?`, `has-parent?`, `contains?`, `lua-match?`, `vim-match?`, `any-*`, generic `not-` prefix, `offset!`, `gsub!`, `trim!` | `add_predicate` / `add_directive` | Vim + Apache-2.0 | borrow the names |
| Helix + `tree-house` | Rust | yes | yes | `not-kind-eq?`, `same-line?`, `one-line?` and their `not-` twins; explicit `not-`/`any-` cross product | `UserPredicate` callback | MPL-2.0 | borrow, reimplement |
| Topiary `topiary-core` | Rust | yes, last commit 2026-09-18, v0.7.3 | yes | 10 bang directives: `delimiter!`, `scope_id!`, `single_line_only!`, `multi_line_scope_only!`, `query_name!`, and five more | crates.io library | MIT | borrow the directive shape |
| Zed `language` crate | Rust | yes | yes | `has-parent?` / `not-has-parent?`, one kind only | none | GPL-3.0 family | ignore |
| Emacs `treesit.el` | C | yes | yes | `eq?`, `match?`, `pred?` only | none | GPL-3.0 | ignore, steal the `pred?` idea |
| `tree-sitter-graph` | Rust | repo live, last commit 2024-12-11, v0.12.0 | yes, stanza binds every capture | `scan`, `source-text`, `node-type`, `named-child-index`, position accessors, host functions via `Functions::add` | Rust only | MIT / Apache-2.0 | borrow, do not depend |
| `stack-graphs` + `tree-sitter-stack-graphs` | Rust | archived 2025-09-09, last release 2024-12-13 | via `.tsg` | stack-graph attribute vocabulary | Rust, plus a C API in `stack_graphs::c` | MIT / Apache-2.0 | ignore |
| `basemind-tree-sitter-graph` | Rust | fork, crate 2026-07-20, repo 2026-09-18 | yes | upstream set, ported to tree-sitter 0.26 | Rust only | MIT / Apache-2.0 | port reference, not a dependency |
| Sourcegraph `syntax-analysis` (ex `scip-treesitter`) | Rust | archived, monorepo last touched 2024-08-08 | yes | `#transform!`, `#filter!`, `#set! hoist`, `#set! kind global`, `;;include <lang>` | library, unpublished | MIT | borrow the capture scheme |
| `syntastica-query-preprocessor` | Rust | v0.6.1 2025-06-19 | rewrites text only | rewrites `lua-match?`, `any-of?`, `contains?` into `#match?` | Rust | MPL-2.0, GPL dep | ignore |
| `tree-sitter-utils` | Rust | v0.1.4 2026-03-17, very new | runs queries, no `.scm` loading | Rust-side `has_ancestor_kind`, `has_parent_kind`, `find_ancestor` | Rust | MIT | ignore, the ancestor walk is ten lines |
| `tree-sitter-grep` | Rust | dormant since 2023-07 | yes | none; `.so` filter plugins instead | CLI plus plugin ABI | MIT / Unlicense | ignore |
| `srgn` | Rust | v0.14.2 2026-02-22, commits to 2026-08 | yes, `--rust-query-file` takes `.scm` | none; `_SRGN_IGNORE` capture prefix | CLI, library is 0.x | MIT | ignore |
| `weggli` / `wegglix` | Rust | fork active 2026-02 | no, own C-like pattern language | n/a | Rust, plus `weggli-native` C API | Apache-2.0 | ignore |
| `semgrep` | OCaml | very active | no, own pattern language over its own AST | n/a | CLI and JSON only | LGPL-2.1 | ignore |
| `comby` | OCaml | low activity | no, delimiter templates | n/a | CLI, Rust wrapper shells out | Apache-2.0 | ignore |
| `ast-grep` | Rust | active | one node per match, already rejected | rule algebra, not predicates | Rust | MIT | already in the tree, keep for the fast tier |
| `difftastic` | Rust | active | consumes `highlights.scm` for atom hints | none | binary | MIT | ignore |
| `tree-sitter query` CLI | Rust | yes | yes, `--captures` reorders output | none | CLI, text output, no JSON | MIT | ignore |

## 3. The candidates that matter, in detail

### `tree-sitter-tags`, the one maintained crate that does our job

Shipped inside the tree-sitter repo at `crates/tags`, released with the core on
2026-08-30 as 0.27.0, MIT, 1.2M downloads. It compiles a tags query and a
locals query into one `Query`, runs it, and makes two passes over
`mat.captures()`, so N captures per match is native. Its capture vocabulary is
enforced rather than conventional: `@name`, `@ignore`, `@doc`,
`@local.scope`, `@local.definition`, `@local.reference`, `@definition.<kind>`,
`@reference.<kind>`, and anything else is `Error::InvalidCapture`. The kind
after the dot becomes `syntax_type_id`.

The API is `TagsConfiguration::new(language, tags_query, locals_query)`, a
reusable `TagsContext`, and `generate_tags` yielding
`Tag { range, name_range, line_range, span, utf16_column_range, docs,
is_definition, syntax_type_id }`. There is a C API in `c_lib`.

What it does not give us: relation predicates. Its only extras are
`#select-adjacent!`, `#strip!`, `#is-not? local`, and
`#set! local.scope-inherits false`.

- https://crates.io/crates/tree-sitter-tags
- https://github.com/tree-sitter/tree-sitter/blob/master/crates/tags/src/tags.rs

### `tree-sitter-highlight`, the reference locals resolver

Same repo, same release cadence, same license. It registers zero custom
operators. What it has that we want is the scope layer: a `LocalScope` stack
built from `@local.scope`, `@local.definition`, `@local.definition-value` and
`@local.reference`, where a reference walks scopes innermost outward matching by
name text, `#set! local.scope-inherits false` halts inheritance, and patterns
carrying `#is-not? local` are dropped once the node resolved as a local. That is
the whole scope/graph layer over captures that the brief asked about, and it is
about two hundred lines of algorithm shipped by tree-sitter itself.

Its output is highlight spans, so it is not a dependency for us. The algorithm
is the thing to copy.

- https://github.com/tree-sitter/tree-sitter/blob/master/crates/highlight/src/highlight.rs

### Neovim, the de facto naming authority for relation predicates

`runtime/lua/vim/treesitter/query.lua`. `has-ancestor?` takes a capture plus N
node-type strings and is true if any node of the capture has an ancestor of any
listed type; `has-parent?` is the same against the direct parent only. Both
delegate the walk to the C side. `contains?` takes a capture plus N literal
substrings and requires all of them. `lua-match?` and `vim-match?` are the
pattern-dialect variants we do not want.

Two design moves are worth more than the list. First,
`Query:_process_patterns()` strips a leading `not-` from any predicate name and
inverts the result, so the negation of every predicate is free. Second,
`add_predicate(name, handler, opts)` and `add_directive` are a public
registration API with a collision error, which is the shape a host-predicate
table should have.

Directives worth noting: `offset!`, `gsub!`, `trim!`, all of which write into
per-capture metadata rather than filtering.

Neovim is where `has-ancestor?` originated; Nova and Cursorless both copied it
rather than inventing their own. If we pick different names we are the odd one
out.

- https://github.com/neovim/neovim/blob/master/runtime/lua/vim/treesitter/query.lua
- https://neovim.io/doc/user/treesitter/

### Helix, and the two predicates nobody else has

Predicate parsing moved out of `helix-core/src/syntax.rs` into the `tree-house`
crate at the 25.07 release. `tree-house` parses the built-in families with the
`not-` and `any-` cross product spelled out explicitly in the parser rather than
derived by prefix strip, and hands everything else to a
`UserPredicate::Other(predicate)` callback. Helix's own additions live in
`helix-core/src/indent.rs`: `not-kind-eq?` (capture, string), `same-line?` and
`not-same-line?` (capture, capture), `one-line?` and `not-one-line?` (capture).

`same-line?` and `one-line?` are the interesting pair. They are capture-to-
capture positional relations, cheap to implement, and they cover a class of
question that `#inside?` and `#precedes?` do not.

License is MPL-2.0 on both repos, which is file-level copyleft. Reimplement
from the described semantics, do not paste.

- https://github.com/helix-editor/tree-house/blob/master/bindings/src/query/predicate.rs
- https://github.com/helix-editor/helix/blob/master/helix-core/src/indent.rs

### Topiary, the best example of `general_predicates()` in production Rust

MIT, `topiary-core` on crates.io since 2024-05-17, last commit 2026-09-18,
v0.7.3 on 2025-12-31. `topiary-core/src/tree_sitter.rs` reads
`query.general_predicates(m.pattern_index)` and dispatches on
`predicate.operator()`, treating the trailing `!` as part of the name. Ten
directives, split into five that take one string argument (`delimiter!`,
`scope_id!`, `single_line_scope_only!`, `multi_line_scope_only!`,
`query_name!`) and five zero-argument flags. Missing arguments produce
`"{op} needs an argument"`, unknown operators produce
`"... is an unknown predicate. Maybe you forgot a \"!\"?"`, and
`check_predicates` enforces that at most one of the four line-mode directives
appears on a pattern.

Notably the things people assume are predicates, `@append_hardline`,
`@prepend_space`, `@leaf`, are capture names resolved elsewhere. Topiary is the
model for how to structure our own dispatch: bang-suffixed directives, arity
checks at compile time of the query, a per-pattern config struct, and mutual
exclusion validation. It is the safest license to learn from.

- https://github.com/tweag/topiary/blob/main/topiary-core/src/tree_sitter.rs

### `tree-sitter-graph`, the closest thing to what we would otherwise build

A `.tsg` file is a list of stanzas, each a standard tree-sitter S-expression
pattern followed by a `{ ... }` block that runs once per match with every
capture of that match bound. Unused captures are an error unless prefixed `@_`.
Patterns accept `?`, `+` and `*` quantifiers. The block language has
`node`/`edge`/`attr` for graph construction, `let`/`var`/`set`, scoped variables
`@node.var`, `if`/`elif`/`else` with `some`/`none`, `for ... in`,
comprehensions, and `scan` for regex-arm dispatch on text. The standard function
set is `eq`, `is-null`, `not`, `and`, `or`, `plus`, `format`, `replace`,
`concat`, `is-empty`, `join`, `length`, `node`, plus syntax accessors
`node-type`, `source-text`, `named-child-index`, `named-child-count`,
`start-row`, `start-column`, `end-row`, `end-column`.

Crucially, host functions register from Rust:
`Functions::add<F>(&mut self, name: Identifier, function: F)` with
`call(&self, name, graph, source, parameters) -> Result<Value, ExecutionError>`.
The upstream note is that the executing process controls which functions it
provides. That is the host-predicate requirement satisfied, in a form already
designed.

The cost: last commit 2024-12-11, release 0.12.0 the same day, pinned to
`tree-sitter ^0.24` while the world is on 0.27. Rust only, no C API. Adopting it
means either pinning our tree-sitter or maintaining a port. `Goldziher/basemind`
publishes `basemind-tree-sitter-graph` 0.12.0 on crates.io as of 2026-07-20 with
a tree-sitter 0.26 port and edition 2024, repo active to 2026-09-18, but it is a
vendored-for-a-product fork with roughly a hundred downloads, not a community
successor. Useful as the patch set for a port, not as a dependency.

Note also that the accessor set is itself the design lesson: `node-type`,
`named-child-index` and the four position accessors are exactly the primitives
that `#nth-child?` and `#range?` need, expressed as functions over captures
rather than as fixed predicates.

- https://github.com/tree-sitter/tree-sitter-graph
- https://docs.rs/tree-sitter-graph/latest/tree_sitter_graph/functions/struct.Functions.html
- https://crates.io/crates/basemind-tree-sitter-graph

### `stack-graphs`, archived

GitHub archived `github/stack-graphs` on 2025-09-09, read only, with the README
saying the repository is no longer supported or updated and recommending a fork.
Last real releases are `stack-graphs` 0.14.1 and `tree-sitter-stack-graphs`
0.10.0, both 2024-12-13. It does have a C API at `stack_graphs::c` with a
header at `include/stack-graphs.h`, and the path-stitching name resolution is
genuine research work, but it consumes `.tsg` rather than `.scm` and it inherits
the frozen tree-sitter pin. None of the 172 forks is an active continuation.

Take the vocabulary, not the code.

- https://github.com/github/stack-graphs
- https://github.blog/open-source/introducing-stack-graphs/

### Sourcegraph's `scip-syntax`, the design we should copy hardest

There is no `sourcegraph/scip-syntax` repository. The crates lived first in
`sourcegraph/scip-semantic`, archived 2023-03-16, then in
`sourcegraph/sourcegraph-public-snapshot` under
`docker-images/syntax-highlighter/crates/`, archived 2024-09-02 with that path
last touched 2024-08-08. They were renamed on the way: `scip-treesitter` became
`syntax-analysis`, `scip-treesitter-languages` became `tree-sitter-all-languages`,
and `scip-syntax` is the CLI. None of them is on crates.io. The pin is
`tree-sitter = "0.20.9"`. License is MIT via
`docker-images/syntax-highlighter/LICENSE`. Upstream SCIP itself moved to
`github.com/scip-code/scip` and is active to 2026-09-20, but that is the
protobuf and bindings crate, not the tree-sitter layer.

The design is still the best in the survey, and it runs `.scm` with N captures
per match in both `globals.rs::parse_tree` and `locals.rs`. Its globals queries
live at `queries/<lang>/scip-tags.scm` and use `@descriptor.namespace`,
`@descriptor.type`, `@descriptor.term`, `@descriptor.method` for SCIP descriptor
suffixes, `@kind.<one of 28>` for `SymbolInformation::Kind`, plus `@scope`,
`@enclosing` and `@local`. Several captures land on one node, for example
`name: (_) @descriptor.type @kind.trait) @scope`. The locals file uses
`@scope.<kind>`, `@definition.*`, `@reference.*` and `@occurrence.skip`.

Its extra predicates are the interesting part, because none of them is a
relation: `#transform! <regex> <replace>` rewrites a descriptor name,
`#filter! @cap <node-kinds>` restricts by kind, `#set! hoist <scope-kind>` makes
a definition visible before its lexical position, and `#set! kind global` widens
reference visibility. A first-line `;;include <lang>` directive inherits another
language's file. The resolver is an id-arena scope tree with a `hoisted_definitions`
map, lexically sorted `definitions`, a bounded parent walk and a global fallback.

`syntax-analysis` is a real library, with `get_globals(ParserId, &str)` and
`get_locals(ParserId, &str, LocalResolutionOptions)`, so it is reusable in
principle. Unpublished plus archived means vendor or reimplement.

- https://github.com/sourcegraph/sourcegraph-public-snapshot/tree/main/docker-images/syntax-highlighter/crates
- https://github.com/scip-code/scip

## 4. The candidates that do not apply, and why

`semgrep`, `comby` and `weggli` all match code, none of them runs `.scm`.
Semgrep parses with tree-sitter and then maps the CST onto its own generic AST,
so its metavariables are not tree-sitter captures and there is no Rust or C
library API. Comby is delimiter templates with no parse tree at all, and the
only Rust crate shells out to the binary. Weggli has its own C-like query
language; the maintained `wegglix` fork and the `weggli-native` C bindings are
real, but they answer a different question and only for C and C++.

`tree-sitter-grep` had the right instinct, a `(&Node) -> bool` filter plugin
loaded from a shared library, and has been dormant since July 2023 at version
`0.1.0-dev.0`. `srgn` accepts a raw `.scm` file per language via
`--rust-query-file` and friends, supports several captures in a pattern, and
adds exactly one convention, a `_SRGN_IGNORE` capture-name prefix; no
predicates. `syntastica-query-preprocessor` only rewrites query text so Neovim
queries compile against the Rust binding, and it drags a GPL-only
`lua-pattern` dependency. `tree-sitter-utils` is four months old with four
releases in two days and its ancestor walk is ten lines we would write anyway.
`difftastic` consumes `highlights.scm` to decide which nodes are atoms and adds
nothing. The `tree-sitter query` CLI `--captures` flag only reorders output; it
has no JSON mode.

Zed is a strict subset of Neovim with `has-parent?` limited to one kind and
unknown operators silently satisfied by a `_ => true` arm, which is the bug we
already have on the `#set!` path. Emacs supports three predicates total, but its
`pred?` is worth noting: arity 1 or more, a Lisp function symbol followed by N
capture names, called with the nodes. That is the cheapest possible host escape
hatch and it generalizes everything above it.

## 5. Recommendation

Take `tree-sitter-tags` 0.27 for the definition and reference rows and copy
`tree-sitter-highlight`'s `LocalScope` resolver for the scope layer, since
together they are the only maintained, published, MIT, N-capture pair that ships
today, and they already agree with the `@definition.*` / `@local.*` vocabulary
that Helix, Neovim and Sourcegraph all vendor. We would still write the
relation-predicate evaluator ourselves over `Query::general_predicates()`,
because it exists in no library in any language: about 500 lines in
`2_source_query.rs` for `#inside?`, `#has?`, `#precedes?`, `#follows?`,
`#nth-child?`, `#range?`, Helix's `same-line?` and `one-line?`, Neovim's
`has-ancestor?` and `has-parent?` spellings as aliases, Neovim's generic `not-`
prefix strip, the `any-` quantified-capture flag, and Topiary-style arity
checking at query-compile time, plus roughly 300 lines of tests. The biggest
risk is name divergence: `#inside?` is our spelling and `#has-ancestor?` is the
one every other host uses, so any `.scm` vendored from Neovim or Helix will
either fail to compile or, worse, hit the same silent-acceptance path that
already swallows `#set!` today, which is why the evaluator must reject unknown
operators rather than pass them.
