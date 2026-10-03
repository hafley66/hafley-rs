# ryi one-shot questions: eager loads vs demand walk (inventory)

Rule: .claude/skills/2026-10-03-demand-driven-questions (every one-shot question walks
a worklist from its seed; no eager whole-project build per question). scm++ joins the
rule later: dynamic extraction (rows only for files/nodes the query can reach).

Source: ripgrep over crates/sprefa-extract/src at main f5214b48, plus ryii graph
dogfood (bugs in issues/ryii-dogfood-self-20261003).

| command | seed | eager load today | site |
| --- | --- | --- | --- |
| graph (fast) | `[PATH#]NAME` | `resolve_project_with_tsi_tiers` over every input file, then SQL walk | src/0_graph.rs:51 `load_store`, :116 |
| graph --slow | `[PATH#]NAME` | `slow_project` / `slow_project_with_raw`: every file, every body inferred | src/0_graph.rs:95-113; hafley_scm read/lang/rust_checker_ra.rs:150 `answer` |
| graph --at / --compare | `[PATH#]NAME` + revision | `load_store` per revision into a scratch tree | src/0_graph.rs:659 |
| rename (fast, Rust) | `FILE#OLD` | `Corpus::open` reads every Rust file's text (`cx.files_of(&RustSource)`) to find spellings | src/edit/rust_rename.rs:320-328 |
| rename --slow (Rust) | `FILE#OLD` | rust-analyzer workspace load, rename via RA | src/edit/1f_ra_rename.rs |
| cleave | `FILE#ITEM DEST` | `resolve_project` (call arm) over the root, plus RA Names-tier def maps | src/edit/_7_cleave.rs:2167 |
| move | `FILE DEST` | specifier repair over every importer | src/edit/_6_move.rs:31 |
| query --scmpp | query + paths | capture rows for every input file; CST rows when the plan has relations; then one SQL | src/0_query.rs:140 `run_scmpp` |
| stratify | entrypoints | `resolve_project` over every path, then rank | src/0_stratify.rs:83 |
| diff | two revisions | `resolve_project` per revision (whole-project delta by definition) | src/5_diff.rs:90 |
| slow / fast / scip (dump verbs) | none | whole project by definition (not one-shot questions) | src/bin/ryi.rs:143-177 |

## Demand shape per command

| command | worklist item | frontier rule | stop |
| --- | --- | --- | --- |
| graph --from / --call-path | body | resolve calls in popped body; project targets enqueue; extern targets record edge; Passed / TraitImpl / Rule edges enqueue (plans/2026-10-03-graph-slow-demand-walk.md) | queue empty, depth, deadline |
| graph --callers | candidate body | text prefilter (bodies spelling NAME, from the syntax index) then resolve only those | candidates exhausted |
| rename | file | files that spell OLD or a re-export of its owner; add files that import those modules | no new files |
| cleave / move | file | source, destination, importers of the moved item's module | no new importers |
| query --scmpp | file, then node range | files matching the level-0 pattern; nodes only inside matched roots' ranges (later) | — |
| stratify | file | import edges from entrypoints | no new files |
