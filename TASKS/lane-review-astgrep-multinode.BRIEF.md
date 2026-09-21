# Review: can ast-grep return more than one node per match?

Read-only. No repo edits. Answer one question with evidence from source and docs.

Claim under review (`crates/sprefa-extract/plans/2026-09-21-scm-superset.md`
sections 1 and 4): an ast-grep rule match is exactly one node plus a
`MetaVarEnv`; there is no way to get an N-capture tuple with positional
identity (left vs right child) the way a tree-sitter `.scm` query returns it.

Check every ast-grep feature that could refute it, in the vendored source
`~/.cargo/registry/src/*/ast-grep-core-0.38.7` and `ast-grep-config-0.38.7`,
AND the newest docs at https://ast-grep.github.io (0.45.x): `Pattern` with
`$A`/`$$$A` meta-variables and `MetaVarEnv::get_match`/`get_multiple_matches`;
`transform`; `constraints`; `has`/`inside` with `field:` and `stopBy`;
`nthChild` with `ofRule`; ESQuery selectors in `kind` (`:nth-child`, `:has`);
`rewriters`; the napi/pyo3 `getMultipleMatches`, `getMatch`; `--json` output of
`sg run` (does it list every metavar with its range?). Write a 30-line Rust
probe under `timeout 60 cargo run` in a scratch crate (not in this repo) that
parses `a + b` (javascript) with the rule `kind: binary_expression` plus
whatever combination of `has: {field: left, pattern: $L}` and `has: {field:
right, pattern: $R}` you find, and print `$L` and `$R` text and ranges.

Verdict, one of: (a) claim holds, one node plus env, env CAN carry N distinct
nodes when patterns/fields pin them, and here is the exact rule shape that
yields `L=a, R=b`; (b) claim false, here is the API that returns tuples.
Write the verdict, probe source, probe output, and `path:line` quotes to
`TASKS/lane-review-astgrep-multinode.REPORT.md`. Commit only that file:
subject `review: ast-grep multi-node match verdict`, trailers
`Boop-Status: done`, `Boop-Check: probe -> <printed L and R>`.
Every shell command under `timeout 10` except the cargo run.
