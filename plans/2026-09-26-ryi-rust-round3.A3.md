# ryi fast Rust vs CodeQL, lane A3

Base `96292970`. Key: `(source file, enclosing item, target file, target name)`; macro expansions excluded. `R` is ryi-only, `C` CodeQL-only. Frozen root is 423 Rust files from cached CodeQL `src.zip`; sprefa staged source is 615 Rust files including A3 fixtures, versus A2's 584.

| Corpus / edges | A2 agree/R/C | A3 agree/R/C | C reduction |
| --- | ---: | ---: | ---: |
| Root types | 7791/24/525 | 8087/24/229 | 296 |
| Sprefa types | 1693/970/144 | 1838/980/33 | 111 |
| Sprefa calls | 4197/1983/145 | 4305/2026/77 | 68 |

Root A2 type C525 buckets: impl `Self` 221, expression type arguments 66, absolute `#[path]` module types 9, remaining 229. A CodeQL syntax query classifies C229 by precedence: parameter 79, alias 68, nested generic 39, impl 19, local 9, field 5, other 10. Top target names: `Span` 19, `Effect` 14, `Strings` 13. Root R24 is unchanged from A2's source-matching variants/macro exclusions.

Root full-call CodeQL query exceeds its 536 MiB Java evaluator heap under `--ram=2048 --threads=4`; call-only, method-only, source-filtered, altered heap split, and a smaller database also failed. A boop-only method query exited 99 after four minutes without evaluator diagnostics. Path-call and struct-literal queries completed separately: their union is 10,221 CodeQL rows. On frozen root A3 agrees with 9,731; partial C is 490, down from 677 before A3 calls. Partial R cannot stand for full-call R because methods are absent. Remaining C: path calls 442 (`Default` 256, `new` 46, `reduce` 20); struct literals 48 (`Span` 9, `TurnQuery` 6, `LiveSessions` 6).

Changes: impl `Self` type references (`ec5650b9`), body expression type arguments (`eac9b7be`), `Self {}` and `#[path]` module calls (`c30fea0c`), absolute `#[path]` normalization (`e036a030`), and fixture import scoping (`fb368446`). The same-named `First::rows`/`Second::rows` ladder row binds each call to its own method. Sprefa test call C fell 84 to 40; edit call C fell 22 to 9; edit type C fell 22 to 2.

Sprefa total call C77 remains five rows above the half-of-145 threshold (72). The residual 77 includes `src/bin/ryi*` 21 and test fixtures 20, both outside A3's edit list; the in-scope edit bucket is 9.

The 174 and 180 ladders pass (12 and 4 tests). The 170 type ratchet fails: both 910, fast-only 49, slow-only 1 versus pin 904/3/2. The new fast `Self` edges need a matching projection of SCIP `impl#[Type]` references in `src/read/2_slow.rs`, outside A3's authorized file list. Scope extension requested and pending; no pin was changed. Full crate gate therefore remains red.

The two added 174 golden rows are `clone -> NoField` and `from -> OneField`, both return-type edges from S1. The new negative fixture row rejects a false cross-fixture target in relative and absolute runs; the relevant fixture files have identical bytes and one content ID, so their distinct paths cannot be emitted reliably. `2026-09-26-ryi-rust-round3.A3-AUDIT.tsv` gives 20 verdicts each for sprefa call R growth, current sprefa type R, and root rows unmatched by the partial call query, plus all seven sprefa type R additions on the same 615-file snapshot; every sampled target matches source syntax.

Review follow-up WIP adds Cargo target and dependency-kind visibility, exact qualified module checks, and scoped associated-type lookup with new 174/180 fixture rows. All counts above predate this follow-up. The user deferred every build, test, ryi, and CodeQL run; the new rows and resolver behavior are unverified.
