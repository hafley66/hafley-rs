# Lane: does helix `locals.scm` plus a scope-tree engine replace `ryi fast`?

Repo `~/projects/hafley-rs`. You work in `$PWD` (your worktree). Never `cd` to
another checkout.

Issue: `issues/lab-scopegraph-queries/item.md`. Read all of it first. L1 is
shipped inside `crates/sprefa-extract` (`.scm` lowers to ast-grep rules; see
`crates/sprefa-extract/docs/2_scm-with-ast-grep-relations-20260920.md`). This
lane is L2 through L6.

## Isolation

New crate `crates/lab-20260920-scm-locals-vs-fast` (lab naming law:
`.claude/skills/2026-09-20-lab-naming/SKILL.md`). Its own workspace root, like
`crates/sprefa-extract`. Depends on `sprefa-extract` by path for parse and
query only. ZERO edits under `crates/sprefa-extract/src`. If you need a
`pub` that is not there, write `Boop-Status: blocked` plus `Boop-Ask` naming
the item; do not fork the code into the lab.

## COMMIT CONTRACT

One commit per step L2..L6. Every commit carries, one `-m` per line after the
subject:

```
Boop-Status: wip
Boop-Check: <exact command> -> <exact counted result>
Boop-Trace: <trace file> -> <first span over 1s, or "none">
Refs-Issue: @lab-scopegraph-queries
```

Never write `rc=0`. Last commit is `Boop-Status: done`.

## TRACE LAW

Every command you launch runs under `timeout 10`. A hit timeout is a defect,
reported as-is, never re-run longer. Every `ryi` and every lab binary run
carries `HAFLEY_TRACE=$PWD/traces/<step>-<n>.json`. The lab binary installs
`hafley_observe` the same way `crates/sprefa-extract/src/trace.rs:install`
does (`chrome_layer` included). Spans are chrome `B`/`E` pairs; pair them by
tid to get durations. `traces/` is gitignored in the lab crate.

## Steps

| step | content | done when |
| --- | --- | --- |
| L2 | vendor helix `runtime/queries/kotlin/locals.scm` under `queries/kotlin/locals.scm` with upstream URL, commit sha and MPL-2.0 notice in a header; extend for defs, calls, imports the convention omits | additions under 200 lines, counted in the commit |
| L3 | scope graph in sqlite, not a hand walk: the engine runs `locals.scm` with `matches()` only, checks `did_exceed_match_limit()` after every run (true is a named error), and writes `node(id, kind, sym, blob, span_start, span_end)` and `edge(src, dst)` rows with `kind` in `root, scope, def, ref, push, pop, export, import` (the card's `NodeKind`). Resolution is ONE `WITH RECURSIVE` query carrying the symbol stack as a text column: `push` appends, `pop` must match the head, `def` with an empty stack wins. Rust holds the parse, the capture-to-row mapping, and the SQL string. | `src/` under 800 lines, counted; the resolve SQL pasted in the commit body |
| L4 | judge on `crates/sprefa-extract/tests/fixtures/kotlin_receivers` and `kotlin_module_resolve` against `ryi fast` (receiver 7, module_plane 11) plus the unresolved set | zero disagreements, or every one listed with its cause in `REPORT.md` |
| L5 | ts via vendored helix `locals.scm`, same fixture-vs-`ryi fast` judge on `tests/fixtures/ts5_findings/module_plane` | engine diff 0 lines, counted |
| L6 | scip ratchet: run the lab's edges through the same floors `RATCHET.tsv` holds | floors hold, numbers pasted |

Judge = a test under `tests/` in the lab crate that runs both, diffs the edge
sets keyed `(path, name) -> (path, name)`, and prints the three-way split:
both, lab-only, fast-only.

## Verdict (in `REPORT.md`, last commit)

One table: language / `.scm` lines / engine lines / `ryi fast` Rust lines for
that language / disagreements. One sentence: replaces `ryi fast` or does not.
No recommendation beyond that sentence.

## Style laws

Match `crates/sprefa-extract` file style. No em dashes. No `honest`,
`load-bearing`, `substrate`, `provenance`, `regime`. Comments state facts.
Tests through binaries or the public API, no mocks.
