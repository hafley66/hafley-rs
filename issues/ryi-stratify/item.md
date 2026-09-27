---
created: 2026-09-19
updated: 2026-09-27
type: feature
status: fixed
priority: normal
epic: ryi-new-verbs
blocked_by: ['@extract-graph-verb', '@extract-lines-flag']
related: ['@extract-graph-verb', '@extract-lines-flag']
labels: [artifact-cli, extract, intent-architecture]
---

# ryi stratify: toposort an entrypoint into numbered strata, size by locality

## Description

## Description

A dry-run verb that reads an entrypoint, walks the file graph, and proposes a
stratification: which file gets which numeric prefix, and which files are the
wrong size for how cohesive they are. It emits a `ryi move` plan and applies
nothing.

Three of the four pieces already exist.

| piece | state | where |
| --- | --- | --- |
| file-level DAG | exists | `file_edge{src_path, dst_path, kind, symbols}`, `src/types.rs:3641` |
| per-file size | exists | `file{path, digest, bytes, lines}`; `line_start` after @extract-lines-flag |
| edge endpoints by path | exists | `resolved_edge{caller_path, callee_path}`, `schema/1_facts.tsp:64` |
| reachability from an entrypoint | filed | @extract-graph-verb, `--from PATH:NAME` |
| move, respell every reference, manifests, plan check | exists | `ryi move`; dry run is the DEFAULT, `--commit` is opt-in (`src/0_move.rs:55`) |
| toposort, SCC condensation, locality score | NEW | this issue |

## Plane 1: the ordering

`file_edge` is a directed graph over paths. Condense it into strongly connected
components, topologically sort the condensation, and assign each SCC a depth.
The depth IS the numeric prefix: dependencies get lower numbers, consumers get
higher ones, per the repo's filesystem ordering convention.

Cycles are normal in Rust module graphs, so the SCC step is not optional. Every
member of one SCC shares one number, and the verb prints the cycle members so
the reader can decide whether to break it.

Ties inside a depth are broken by in-degree, then by path, so the output is
deterministic across runs.

Double digits (`00_`, `01_`) when the depth count exceeds 10. Index files
(`index.ts`, `mod.rs`, `lib.rs`) take no number.

## Plane 2: the locality score

For a file F, of every edge with at least one endpoint in F, the share whose
BOTH endpoints are in F.

```prolog
edge_touching(F, Src, Dst) :- resolved_edge(_, _, Src, _, _), file_edge(Src, Dst, _, _), (Src = F ; Dst = F).
internal(F, N)  :- count(Src = F, Dst = F).
touching(F, M)  :- count(edge_touching(F, _, _)).
locality(F, N / M).
```

A file at locality 0.9 keeps its content to itself. A file at locality 0.2 is a
grab bag whose parts answer to other files.

## Plane 3: size proportionate to locality

The claim this verb makes: a file EARNS its line count with its locality. Target
lines for F are `base_lines * locality(F)`, with `base_lines` a flag defaulting
to the corpus median.

| observed | proposal |
| --- | --- |
| high locality, at or under target | leave it |
| high locality, well under target, and an SCC sibling is too | merge candidate, print both paths and the shared edge count |
| low locality, over target | split candidate, print the internal SCCs of its own symbol graph as the suggested cut lines |
| low locality, small | move candidate, print the file its edges mostly point at |

Every proposal is a `ryi move` plan entry, so the respell, manifest and
plan-check machinery already handles the mechanics.

## Shape

```
ryi stratify --from src/main.rs [--base-lines N] [--kind call|type|both] [PATH...]
```

Output records:

```
record=stratum       depth=<u32>  path=<string>  prefix=<string>  scc=<u32>
record=stratum_cycle scc=<u32>    members=<u32>  paths=<string>
record=locality      path=<string> internal=<u32> touching=<u32> score=<f32> lines=<u32> target_lines=<u32>
record=stratify_move from_path=<string> to_path=<string> reason=<slug>
```

`reason` vocabulary: `depth_prefix`, `merge_candidate`, `split_candidate`,
`move_candidate`.

Exit 0 with the plan on stdout. The verb never writes. Feeding its plan to
`ryi move --commit` is a separate, explicit act.

## Acceptance Criteria
- [x] `file_edge` condensed into SCCs and topologically sorted, deterministic across runs
- [x] `stratum` rows carry depth and the proposed numeric prefix; index files get none
- [x] `stratum_cycle` rows print every member of a multi-file SCC
- [x] `locality` rows carry internal, touching, score, lines, target_lines
- [x] `stratify_move` rows for each of the four reasons, on a fixture that exhibits all four
- [x] the emitted plan is accepted by `ryi move` without hand editing
- [x] the verb writes nothing; a test asserts the tree is byte-identical after a run
- [x] `ryi stratify --from` on this crate reports its own strata, and the output is pasted into the issue as the receipt

## Tests Run

- `cargo nextest run --workspace -j 2 --no-fail-fast --status-level fail -E 'not (test(/e2e|live|tmux|tui_sigint|omp_live/))'`: 1363 passed, 203 skipped.
- `cd crates/sprefa-extract && cargo nextest run --features cli -j 2 --no-fail-fast --test all`: 1117 passed, 18 skipped.
- Fixture tests: `t_156_stratify::stratify_is_deterministic_and_leaves_the_tree_unchanged`, `t_156_stratify::stratify_rejects_unknown_edge_kind`, `t_156_stratify::entrypoints_union_unranked_paths_and_ranked_paths_choose_the_highest_rank`; CLI help capture: `t_178_ryi_help::generated_clap_help_matches_captured_main`.
- `ryii stratify --from src/bin/ryi.rs --root . .` from `crates/sprefa-extract`: locality=1297, stratify_move=583, stratum=40, stratum_cycle=12, unreached=1257; self row: `{"depth":7,"entry_depth":0,"path":"src/bin/ryi.rs","prefix":"7_","record":"stratum","scc":72,"via":"src/bin/ryi.rs"}`.

```jsonl
{"depth":5,"entry_depth":1,"path":"src/0_graph.rs","prefix":"5_","record":"stratum","scc":66,"via":"src/bin/ryi.rs"}
{"depth":5,"entry_depth":1,"path":"src/0_query.rs","prefix":"5_","record":"stratum","scc":67,"via":"src/bin/ryi.rs"}
{"depth":5,"entry_depth":1,"path":"src/0_stratify.rs","prefix":"5_","record":"stratum","scc":68,"via":"src/bin/ryi.rs"}
{"depth":5,"entry_depth":1,"path":"src/3_region_writer.rs","prefix":"5_","record":"stratum","scc":69,"via":"src/bin/ryi.rs"}
{"depth":5,"entry_depth":1,"path":"src/4_watch.rs","prefix":"5_","record":"stratum","scc":70,"via":"src/bin/ryi.rs"}
{"depth":5,"entry_depth":1,"path":"src/5_diff.rs","prefix":"5_","record":"stratum","scc":71,"via":"src/bin/ryi.rs"}
{"depth":7,"entry_depth":0,"path":"src/bin/ryi.rs","prefix":"7_","record":"stratum","scc":72,"via":"src/bin/ryi.rs"}
{"depth":0,"entry_depth":1,"path":"src/bin/ryi/0_cli.rs","prefix":"0_","record":"stratum","scc":73,"via":"src/bin/ryi.rs"}
{"depth":0,"entry_depth":1,"path":"src/bin/ryi/0_revision.rs","prefix":"0_","record":"stratum","scc":74,"via":"src/bin/ryi.rs"}
{"depth":1,"entry_depth":1,"path":"src/bin/ryi/0_sqlite.rs","prefix":"1_","record":"stratum","scc":75,"via":"src/bin/ryi.rs"}
{"depth":0,"entry_depth":2,"path":"src/bin/ryi/0a_bind.rs","prefix":"0_","record":"stratum","scc":76,"via":"src/bin/ryi.rs"}
{"depth":5,"entry_depth":1,"path":"src/bin/ryi/1_inputs.rs","prefix":"5_","record":"stratum","scc":77,"via":"src/bin/ryi.rs"}
{"depth":0,"entry_depth":1,"path":"src/bin/ryi/gen/server_auto.rs","prefix":"0_","record":"stratum","scc":78,"via":"src/bin/ryi.rs"}
{"depth":0,"entry_depth":1,"path":"src/bin/ryi/ops.rs","prefix":"0_","record":"stratum","scc":79,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":2,"path":"src/edit.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":2,"path":"src/edit/_0_seams.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":2,"path":"src/edit/_1_move_cx.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":2,"path":"src/edit/_1_rename_cx.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":0,"entry_depth":2,"path":"src/edit/_2_drain.rs","prefix":"0_","record":"stratum","scc":81,"via":"src/bin/ryi.rs"}
{"depth":1,"entry_depth":2,"path":"src/edit/_3_stage.rs","prefix":"1_","record":"stratum","scc":82,"via":"src/bin/ryi.rs"}
{"depth":3,"entry_depth":2,"path":"src/edit/_4_move_scip.rs","prefix":"3_","record":"stratum","scc":83,"via":"src/bin/ryi.rs"}
{"depth":5,"entry_depth":1,"path":"src/edit/_5_move_text.rs","prefix":"5_","record":"stratum","scc":84,"via":"src/bin/ryi.rs"}
{"depth":6,"entry_depth":1,"path":"src/edit/_6_move.rs","prefix":"6_","record":"stratum","scc":85,"via":"src/bin/ryi.rs"}
{"depth":6,"entry_depth":1,"path":"src/edit/_6_rename.rs","prefix":"6_","record":"stratum","scc":86,"via":"src/bin/ryi.rs"}
{"depth":5,"entry_depth":2,"path":"src/edit/_6_rename_verify.rs","prefix":"5_","record":"stratum","scc":87,"via":"src/bin/ryi.rs"}
{"depth":6,"entry_depth":1,"path":"src/edit/_7_cleave.rs","prefix":"6_","record":"stratum","scc":88,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":3,"path":"src/edit/kotlin_rehome.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":3,"path":"src/edit/kotlin_rename.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":3,"path":"src/edit/prolog_rehome.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":3,"path":"src/edit/prolog_rename.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":3,"path":"src/edit/rust_mutate.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":3,"path":"src/edit/rust_rehome.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":4,"path":"src/edit/rust_rehome/cross.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":3,"path":"src/edit/rust_rename.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":3,"path":"src/edit/ts_mutate.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":2,"path":"src/edit/ts_rehome.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":3,"path":"src/edit/ts_rehome/cross.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":2,"entry_depth":3,"path":"src/edit/ts_rename.rs","prefix":"2_","record":"stratum","scc":80,"via":"src/bin/ryi.rs"}
{"depth":4,"entry_depth":1,"path":"src/lib.rs","prefix":"","record":"stratum","scc":89,"via":"src/bin/ryi.rs"}
{"depth":0,"entry_depth":2,"path":"src/trail.rs","prefix":"0_","record":"stratum","scc":90,"via":"src/bin/ryi.rs"}
```

## Implementation Notes

Blocked on @extract-graph-verb for the `--from` traversal and on
@extract-lines-flag for clickable output.

## Decisions

### 2026-09-19T19:12:29Z · @claude-opus-5

Multi-entrypoint, user-set 2026-09-19. --from is repeatable. UNRANKED: the entrypoint set is a union and a file's depth is the MIN over every entrypoint that reaches it. RANKED: --from PATH:NAME=RANK, and a file takes its depth from the highest-ranked entrypoint that reaches it, ties falling back to the min. A file reached by no entrypoint lands in an 'unreached' row and is never renumbered. The stratum row gains a 'via' column naming which entrypoint gave it its depth.

### 2026-09-27 · @codex

Decision: include `split_candidate` in the initial verb. The acceptance criteria
require all four proposal reasons, and the description specifies symbol-graph
SCC cut lines for split proposals.

Current CLI repro: `/Users/chrishafley/.agent/lanes/chore-the-gang-cli/target/debug/ryii stratify --from src/main.rs`
exits 2 with `unexpected argument '--from'` and prints the root `ryii <PATH>...`
usage. No `stratify` verb is present.

## Plan

After the graph traversal and line output prerequisites settle, compute the reachable file graph, condense cycles into SCCs, and assign deterministic dependency-first strata; join file sizes and resolved edge endpoints for locality scores, derive target sizes from the corpus median, and emit reason-coded move proposals that `ryi move` can validate without applying. Build a fixture covering cycles and all four proposal reasons, assert the tree stays byte-identical, then run the verb on this crate and record the output.

Receipt: commit `feat(ryi): add graph stratification planning`; test `t_156_stratify::stratify_is_deterministic_and_leaves_the_tree_unchanged`.
