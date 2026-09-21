# Lane: `ryi fast` reads its call and def facts from a `.scm` file, one language

Repo `~/projects/hafley-rs`, crate `crates/sprefa-extract` (binary `ryi`).
This crate is its OWN workspace root. Every cargo command runs from
`crates/sprefa-extract` inside `$PWD` (your worktree). Never `cd` to another
checkout.

Cards: `issues/ryi-fast-tier/item.md` (parent), `issues/lab-scopegraph-queries/item.md`
(L1 shipped: `.scm` lowers to ast-grep rules through
`src/lang/5_scm_lower.rs`; host predicates `#inside? #has? #precedes?
#follows? #nth-child? #range? #pattern?` in `src/lang/1_ast_rule.rs`; guide
`docs/2_scm-with-ast-grep-relations-20260920.md`).

## The question this lane answers

The structure audit found 61% of `src/` is per-language Rust and 13 walker fns
hand-copied across go/kotlin/rust/ts. `kotlin.rs` is 2135 lines. Can the
CallF family (`kt_walk_call_defs`, `kt_walk_call_sites`, `kotlin.rs:23`) be
produced by a `.scm` program lowered through L1, with the Rust reduced to
capture-to-fact mapping? Kotlin only. One family only. Byte-identical output
is the bar.

## COMMIT CONTRACT

One commit per phase, four phases. Every commit carries, one `-m` per line after
the subject:

```
Boop-Status: wip
Boop-Check: <exact command> -> <exact counted result>
Boop-Trace: <trace file> -> <first span over 1s, or "none">; <ms fast-scm> vs <ms fast-rust> on the fixture
Refs-Issue: @ryi-fast-tier
```

Never write `rc=0`. Last commit is `Boop-Status: done`. Stuck means
`Boop-Status: blocked` plus one `Boop-Ask`.

## TRACE LAW

Every command runs under `timeout 10`. A hit timeout is reported as such,
never re-run longer. Every `ryi` run carries
`HAFLEY_TRACE=$PWD/traces/<phase>-<n>.json RUST_LOG=sprefa_extract=debug`.
Spans are chrome `B`/`E` pairs; pair by tid for durations. `traces/` is in
the crate `.gitignore` already. The phase-4 commit carries the wall-time
ratio scm-vs-rust on `tests/fixtures/kotlin_receivers`.

## Owned files

- NEW `queries/kotlin/call.scm` (with a header stating what it produces)
- NEW `src/lang/6_scm_family.rs`: run a `.scm` program through L1 on one
  file, map captures to `CallF` rows
- `src/lang/kotlin.rs`: ONLY the `extract` arm that picks scm vs rust for
  CallF behind an env var `RYI_FAST_SCM=1`; both paths stay
- `src/lang/mod.rs`: one `mod` line
- NEW test `tests/150_fast_scm_kotlin.rs`

FORBIDDEN: `5_scm_lower.rs`, `1_ast_rule.rs` (if L1 lacks a predicate you
need, `Boop-Status: blocked` with the predicate named), every other
`lang/*.rs`, `docs/`, `schema/`, any other test.

## Phases

| phase | content | check |
| --- | --- | --- |
| 1 | `queries/kotlin/call.scm` capturing call defs and call sites with the captures `@def.name @def.span @site.callee @site.receiver @site.span`; lowers with zero `ScmLowerError` | `ryi query --scm queries/kotlin/call.scm FILE` (or the existing L1 entry; find it in `bin/ryi.rs`) -> N matches on `tests/fixtures/kotlin_receivers` |
| 2 | `6_scm_family.rs`: captures -> `CallF` rows; env switch in `kotlin.rs` | `RYI_FAST_SCM=1 ryi fast FILE` runs, row count |
| 3 | parity: `tests/150_fast_scm_kotlin.rs` runs both paths over every file in `tests/fixtures/kotlin_receivers` and `kotlin_module_resolve`, diffs sorted CallF JSONL | 0 differing rows, or every difference listed in the test's failure text and in the commit body |
| 4 | receipt: `.scm` lines, `6_scm_family.rs` lines, lines of `kotlin.rs` the scm path makes dead (count with the fn names) | full gate `timeout 600 cargo test --features cli --no-fail-fast` counted; baseline 190 binaries, 1005 passed |

No deletion of Rust in this lane. The receipt says what could go.

## Style laws

Match the surrounding file. No em dashes. No `honest`, `load-bearing`,
`substrate`, `provenance`, `regime`. Comments state facts. Tests through the
binary, no mocks.
