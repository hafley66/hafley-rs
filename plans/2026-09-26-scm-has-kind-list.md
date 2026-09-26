# scm `#has?` kind lists

- Scope: `hafley_scm` predicate parser, its build entrypoint, and predicate test rows.
- `#has?` and `#not-has?` now parse one or more kinds and optional trailing `neighbor` or `end` like `#has-ancestor?`.
- The existing evaluator applies `any()` to node kind lists; negation remains on the predicate result.
- `#has?`, `#has-ancestor?`, and `#has-parent?` validate every kind with `Language::id_for_node_kind(name, true)`.
- Unknown kinds, including misspelled stop words, return an error naming the predicate, spelling, and pattern index.
- The existing `UnknownOperator` error shape carries this diagnostic so `read/lang/**` stays untouched.
- New table assertions cover the four descendant kinds, neighbor scope, unknown names, and pattern indices 0 and 1.
- Existing single-kind test rows were left byte-identical.
- Verification: static diff review and `git diff --check`; no build, test, or ryi/codeql command run.
