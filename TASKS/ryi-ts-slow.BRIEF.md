# ryi TS: slow tier on TS (D14, D15, D17) + JSX slow groundwork

Repo hafley-rs, crate crates/sprefa-extract (its own workspace root). User goal 2026-10-02: zero TS dogfood failures for
fast and slow today; end goal JSX static analysis fast and slow.
Read: plans/2026-10-01-ryi-ts-utility.md (defect table rows named below: repro, expected, observed), crates/sprefa-extract/dogfood/ts/README.md,
skill sprefa-extract-add-language. Issues: issues/<slug>/item.md for each card below.

Gate (all must hold before each commit):
- Corpus: your own, outside the repo: `crates/sprefa-extract/dogfood/ts/0_corpus.sh ~/projects/rxjs-corpus-$BOOP_LANE` (a hafley-rxjs worktree at d0802620).
  Never write into ~/projects/hafley-rxjs-ryi-dogfood or ~/projects/hafley-rxjs.
- Write one case script per defect you own: crates/sprefa-extract/dogfood/ts/Dnn.sh (two-digit), exit 0 iff the plan's expected column holds.
- `cargo build --release --bin ryii --features cli,ts-checker,typespec`; `RYII=... CORPUS=... dogfood/ts/run.sh <your cases>` all ok.
- `cd crates/sprefa-extract && cargo test --features cli`: no new failures (pre-existing: golden_parity ported_facets_match_v5, rust_doc_parity).
Other lanes edit the same crate in parallel (rtkq/jsx facts; exports-dist; refactor edits; fast graph; slow; misc). Keep diffs inside your area; no drive-by renames or formatting.
Commit per defect; subject names the defect id; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`. No push. No Python. CARGO_BUILD_JOBS=4.
On done: `issuectl note <slug> --as $BOOP_LANE "<commit, gate line>"` per card (do not close). REPORT.md: per defect before/after line.

## Own
- D14 ryi-ts-slow-misses-method-callers (the 12 cache.render sites), D15 ryi-ts-slow-silent-no-checker (missing checker = non-zero exit naming the feature; never 0 edges exit 0), D17 ryi-ts-rename-method-fast (method rename without --slow: either plan via local receiver types or one-line refusal naming --slow, not 286 lines).
- JSX groundwork: --slow resolves a JSX element's component to its declaration and its attributes to the props type members (tsgo). Case J01.sh over a tsx fixture: <Foo bar={x}/> -> Foo decl + FooProps.bar. Coordinate names with lane feature-ryi-ts-rtkq-jsx (fast JSX facts) via record names only; do not edit its files.
