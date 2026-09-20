---
created: 2026-09-19
updated: 2026-09-20
type: feature
status: open
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
- [ ] `file_edge` condensed into SCCs and topologically sorted, deterministic across runs
- [ ] `stratum` rows carry depth and the proposed numeric prefix; index files get none
- [ ] `stratum_cycle` rows print every member of a multi-file SCC
- [ ] `locality` rows carry internal, touching, score, lines, target_lines
- [ ] `stratify_move` rows for each of the four reasons, on a fixture that exhibits all four
- [ ] the emitted plan is accepted by `ryi move` without hand editing
- [ ] the verb writes nothing; a test asserts the tree is byte-identical after a run
- [ ] `ryi stratify --from` on this crate reports its own strata, and the output is pasted into the issue as the receipt

## Tests Run

## Implementation Notes

Blocked on @extract-graph-verb for the `--from` traversal and on
@extract-lines-flag for clickable output.

## Decisions

### 2026-09-19T19:12:29Z · @claude-opus-5

Multi-entrypoint, user-set 2026-09-19. --from is repeatable. UNRANKED: the entrypoint set is a union and a file's depth is the MIN over every entrypoint that reaches it. RANKED: --from PATH:NAME=RANK, and a file takes its depth from the highest-ranked entrypoint that reaches it, ties falling back to the min. A file reached by no entrypoint lands in an 'unreached' row and is never renumbered. The stratum row gains a 'via' column naming which entrypoint gave it its depth.
