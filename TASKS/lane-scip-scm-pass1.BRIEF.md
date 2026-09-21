# Lane: scip-scm pass 1, from the lab into `ryi`

Repo `~/projects/hafley-rs`, crate `crates/sprefa-extract` (binary `ryi`).
This crate is its OWN workspace root. Every cargo command runs from
`crates/sprefa-extract` inside your worktree. Never `cd` to another checkout.

Read first, in order:
1. `crates/lab-20260920-scm-locals-vs-fast/REPORT.md` (the measured verdict this
   lane promotes), then its `src/0_types.rs`, `src/1_query.rs`,
   `src/2_store.rs`, `src/3_engine.rs`, `tests/3_rows.rs`
2. `crates/sprefa-extract/src/lang/6_scm_family.rs` and
   `crates/sprefa-extract/queries/kotlin/call.scm` (the shipped two-step: L1
   lowers and selects spans, native `matches()` keeps capture grouping)
3. `crates/sprefa-extract/src/lang/5_scm_lower.rs`
4. `crates/sprefa-extract/tests/RATCHET.tsv` and its enforcer
   `tests/golden_parity.rs:1966` (`pin_ratchet_tsv`), called from
   `call_resolve_scip_ratchet_ts` (`golden_parity.rs:946`)
5. `issues/ryi-fast-tier/item.md`, `issues/lab-scopegraph-queries/item.md`
6. `crates/sprefa-extract/AGENTS.md`

## What ships

```
ryi --family scip_scm PATH...
```

Pass 1 of the SCIP-shaped wire, produced from a `.scm` file per language
through the existing `5_scm_lower.rs` path. No compiler, no indexer, no daemon.

Three rows, one per SCIP concept:

```
scip_scm_symbol     { symbol, path, kind }
scip_scm_occurrence { symbol, path, start, end, role }   role: "def" | "ref"
scip_scm_local      { fn, name, path, start, end }
```

`role` is `def` at the defining occurrence and `ref` at every resolved
non-defining one. `scip_scm_local` carries a binding the file does not export,
attributed to its enclosing callable. The symbol spelling is the lab's:
``scm . . `<path>`/<name>().``

Kotlin first: the lab's `queries/kotlin/locals.scm` (70 lines) becomes
`queries/kotlin/scip.scm`. TypeScript second: the lab's
`queries/typescript/locals.scm` (67 lines) becomes `queries/typescript/scip.scm`.
Both are vendored from helix under MPL-2.0; the header block naming the
upstream commit and the license travels with the file, unedited.

The rows feed the fast-mode SCIP ratchet. This lane must print the ratchet's
per-origin histogram before and after, with the scm rows as an input.

Out of scope, stated in the family's error text as such: any language other
than kotlin and typescript, any compiler or indexer leg, cross-repo symbols,
a persistent index.

## COMMIT CONTRACT

One commit per phase, six phases. Every commit carries, one `-m` per line
after the subject:

```
Boop-Status: wip
Boop-Check: <exact command> -> <exact counted result>
Boop-Trace: <trace file> -> <first span over 1s, or "none">
Refs-Issue: @ryi-fast-tier
Refs-Issue: @lab-scopegraph-queries
```

Never `rc=0`; counted results. Last commit `Boop-Status: done`. Stuck:
`Boop-Status: blocked` plus one `Boop-Ask`.

Moving lab code into `crates/sprefa-extract` commits as
`refactor(extract): ...`. A `refactor(extract):` commit changes no behavior and
its `Boop-Check` says which counted result is unchanged.

## TRACE LAW

Every command you launch runs under `timeout 10`; cargo builds and the full
gate under `timeout 600`. A hit 10s timeout is a defect, reported as such,
never re-run longer. Every `ryi` run carries
`HAFLEY_TRACE=<path under std::env::temp_dir()>` and
`RUST_LOG=sprefa_extract=debug`. The new tests set it the same way
(pattern `tests/150_fast_scm_kotlin.rs:63`). Spans are chrome `B`/`E` pairs;
pair by tid for durations.

The lab's first whole-corpus TypeScript run hit the 10s limit in recursive
traversal across 24 files (REPORT.md, TypeScript judge). Phase 4 states the
per-file wall time it measured and whether the whole-corpus run fits.

## Owned files

- NEW `src/lang/7_scip_scm.rs`, module `scip_scm`: the lab's `1_query.rs`
  capture pass, `3_engine.rs` scope-tree build and resolve, and the row emit,
  folded into one file. If it passes 600 lines, and only then, NEW
  `src/lang/8_scip_scm_store.rs` carries the lab's `2_store.rs` `Store` and
  `RESOLVE_SQL` verbatim.
- `src/lang/mod.rs`: the `#[path]` mod line or lines, next to `scm_family`.
- NEW `queries/kotlin/scip.scm`, NEW `queries/typescript/scip.scm`.
- `src/types.rs`: `FlatFact::ScipScmSymbolRow`, `ScipScmOccurrenceRow`,
  `ScipScmLocalRow`, placed with the other `Scip*Row` variants (`types.rs:3593`
  onward), each with the doc comment stating what it is.
- `schema/1_facts.tsp`: `ScipScmSymbol`, `ScipScmOccurrence`, `ScipScmLocal`
  next to `ScipLocal` (`1_facts.tsp:95`), plus the regenerated output under
  `schema/generated/`.
- `src/bin/ryi.rs`: one `FamilyMode::ScipScm` variant, its arm in
  `family_mode` (`bin/ryi.rs:386`), and the stream function next to
  `stream_scip_family`.
- `tests/RATCHET.tsv`: only ever rewritten by a run under `RATCHET_BUMP=1`.
  Hand-editing it is a defect.
- NEW `tests/157_scip_scm_rows.rs` (row and tsp shape),
  NEW `tests/158_scip_scm_kotlin.rs` (kotlin rows through the binary),
  NEW `tests/159_scip_scm_judge_kotlin.rs` (judge against real SCIP),
  NEW `tests/160_scip_scm_ts.rs` (typescript rows),
  NEW `tests/161_scip_scm_ratchet.rs` (ratchet before and after).
- DELETED in phase 6: the whole of `crates/lab-20260920-scm-locals-vs-fast/`.
  Nothing outside `TASKS/lane-lab-scm-locals-vs-fast.BRIEF.md` names it, so the
  deletion is the directory and nothing else.

FORBIDDEN: `docs/` in any form, `5_scm_lower.rs`, `1_ast_rule.rs`,
`6_scm_family.rs`, `tests/golden_parity.rs`, `tests/bench/mod.rs`, every
existing test. Every `src/lang/<lang>.rs` is forbidden with ONE exception: the
`extract` arm in `src/lang/kotlin.rs` that already switches on `RYI_FAST_SCM`
(`kotlin.rs:1701`). One arm, kotlin only. TypeScript adds no second arm: it
reaches its grammar the same way kotlin does, through the family file. If L1
lacks a predicate you need, `Boop-Status: blocked` with the predicate named.

No new Cargo dependency. `rusqlite` 0.40 (bundled), `tree-sitter` 0.25 and
`tree-sitter-kotlin-sg` 0.4 are already in `Cargo.toml`; the TypeScript
grammar comes from `RyiLang::get_ts_language` through ast-grep, never from a
new `tree-sitter-typescript` entry (AGENTS.md: no new deps without
adjudication).

## Phases

| phase | content | check |
| --- | --- | --- |
| 1 | the three rows in `types.rs` and `1_facts.tsp`, regenerated schema output, `FamilyMode::ScipScm` parsing; the family streams nothing yet | `timeout 600 cargo build --features cli` last line; `ryi --family scip_scm tests/fixtures/kotlin_receivers` -> 0 rows, exit 0; `timeout 600 cargo test --features cli --test 157_scip_scm_rows` counted |
| 2 | `queries/kotlin/scip.scm` plus `7_scip_scm.rs`: lower through `lower_scm` for span selection, execute natively for capture grouping, `did_exceed_match_limit()` true is a named stop, emit the three rows | `ryi --family scip_scm tests/fixtures/kotlin_receivers tests/fixtures/kotlin_module_resolve` -> counted symbol / occurrence / local rows, zero `ScmLowerError`; `tests/158_scip_scm_kotlin.rs` pins those counts |
| 3 | judge the kotlin rows against a real SCIP index over the same fixtures, keyed on (path, symbol name, span) | `tests/159_scip_scm_judge_kotlin.rs` prints and pins three counts: both, scm-only, scip-only. Every scm-only and scip-only row is listed with its cause in the test's failure text and in the commit body, the way `REPORT.md` lists the 8 kotlin edge disagreements |
| 4 | `queries/typescript/scip.scm`, no engine change | `tests/160_scip_scm_ts.rs` counted rows over `tests/fixtures/ts`; diff of `7_scip_scm.rs` against phase 3 is 0 lines, or every added line is named in the commit body |
| 5 | ratchet before and after, scm rows as an input | `call_resolve_scip_ratchet_ts`'s per-origin histogram, run twice, counted both times. Today's `tests/RATCHET.tsv` ts floors are `corpus_unique 8`, `receiver 1`, `scip 2`; the lab measured 0, 0 and 0 joining its own edges (REPORT.md, SCIP ratchet). Floors hold, or one `RATCHET_BUMP=1` run plants the new row and the commit body carries both histograms |
| 6 | the lab crate folded in and deleted: `tests/3_rows.rs` covered by 157, `0_judge.rs` by 159, `1_ts_judge.rs` by 160, `2_ratchet.rs` by 161; `rm -r crates/lab-20260920-scm-locals-vs-fast` | `timeout 600 cargo test --features cli --no-fail-fast` counted; baseline 194 binaries, 1016 passed. `grep -rl lab-20260920 --exclude-dir=.git .` names only `TASKS/lane-lab-scm-locals-vs-fast.BRIEF.md` |

No phase deletes Rust outside the lab crate. `ryi fast` keeps every path it has
today: the lab's verdict is that it does not replace it.

## Style laws

Match the surrounding file. No em dashes. No `honest`, `load-bearing`,
`substrate`, `provenance`, `regime`. Comments state facts. Tests through the
binary, no mocks. Numeric file prefixes as the crate does. Vendored `.scm`
headers keep their upstream commit and license lines.

## Open questions

1. Phase 3 has no kotlin indexer on this machine. `scip-java` is not on PATH,
   `JAVA_SPEC` (`src/scip.rs:308`) carries `Fallback::None`, and both kotlin
   fixtures are loose `.kt` files with no gradle or maven build for
   `scip-java index` to read. Install scip-java and add a build file beside a
   new fixture, or run phase 3's judge against `tests/fixtures/ts` with
   scip-typescript and state kotlin as unjudged?
2. Row tags. The lab emitted `scip_def`, `scip_ref` and `scip_local` with
   `repo` set to `scm` (`REPORT.md`, SCIP-SCM row shape; `tests/3_rows.rs`
   pins those field names). This brief mints `scip_scm_*` tags instead,
   because the pass-1 shape carries spans and a role and the existing tags do
   not, and because `types.rs:3580` names two shapes under one tag as the
   drift hazard the goldens exist to stop. Confirm the scm rows must not land
   under the indexer's tags.
3. Phase 5's join. Is a scm-sourced edge a new `ResolutionOrigin` variant with
   its own pinned `RATCHET.tsv` row, or do the scm rows only raise the counts
   of the existing origins? `ResolutionOrigin` (`types.rs:1664`) has no scm
   variant, and `pin_ratchet_tsv` asserts every emitted origin is already
   pinned.
4. What counts as a `local`. The lab emits `scip_local` only for captures
   whose label contains `variable` (`3_engine.rs:168`). SCIP's own `local N`
   covers every binding a document does not export, file-private functions
   included. Which rule is pass 1?
5. The resolver is a recursive SQL walk over an in-memory rusqlite graph
   (`2_store.rs:5`, `RESOLVE_SQL`) across the whole supplied path set.
   AGENTS.md states this crate is per-file, parallel, pure and cacheable, with
   no database. rusqlite is already a dependency and the store lives for one
   invocation, but the cross-file resolve is not per-file. Does the family
   keep the SQL walk as written, or does pass 1 ship per-file only and leave
   the cross-file join to the dl layer?

## Answers to the open questions (coordinator, 2026-09-21)

1. Phase 3 judges TypeScript against scip-typescript on `tests/fixtures/ts`. Kotlin is stated as unjudged in the commit body and REPORT. No scip-java install.
2. Confirmed: `scip_scm_*` tags. Never the indexer's tags.
3. New `ResolutionOrigin::ScmScope` with its own `RATCHET.tsv` row, pinned in phase 5 through `RATCHET_BUMP=1` at the measured floor. Existing origin counts must not move.
4. SCIP rule: every binding the document does not export is a `local`, file-private functions included.
5. Row emission is per file and pure. The cross-file `WITH RECURSIVE` resolve stays, but only behind the phase 5 join and the judge tests, never in the per-file family path.
