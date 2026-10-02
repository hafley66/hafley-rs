# ryi TS: rename/move/cleave edit plans (D3-D8, D20, D22, ryi-move-stale-plan)

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
- D3 ryi-ts-move-dep-edit, D4 ryi-ts-move-type-import, D5+D6 ryi-ts-cleave-public-api, D7 ryi-ts-cleave-mutable-binding, D8 ryi-ts-cleave-import-style, D20 ryi-ts-move-rollback-dir, D22 ryi-state-root-inconsistent.
- ryi-move-stale-plan: a --commit never replays a plan staged under different flags/inputs; key the stage on the full request.
