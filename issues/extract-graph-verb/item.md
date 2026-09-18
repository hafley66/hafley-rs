---
created: 2026-09-18
updated: 2026-09-18
type: feature
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
related: ['@extract-diff-verb', '@extract-lines-flag', '@cdg-facet-and-throw-edges']
---

## Description

User 2026-09-18: "extract must be as capable as possible, its okay to have it have new things like this, we will host or re-use it in dl8 later." Placement settled in-crate; `crates/sprefa-extract/AGENTS.md:38` amended to license one-shot traversal.

The facts already exist. Traversal, line numbers, and reverse edges do not.

Measured 2026-09-18 on `~/projects/instant`, 7 TS files, `extract --resolve --family call,type`, 0.31s wall:

| record | rows |
| --- | --- |
| `resolved_edge` | 221 |
| `resolved_type_edge` | 61 |
| `resolved_import` | 23 |
| `unresolved` | 494 |

A `WITH RECURSIVE` CTE over the sqlite export reproduces reachability from `main` in 4ms. The CTE is the oracle this verb must agree with, not a reason to skip the verb: it needs a visited set written by hand every time, it cannot close the file universe, and its output carries byte offsets no human can click.

## Ordering

`--lines` and `--callers` ship before the verb. Both make the existing surface usable and neither depends on it.

| step | issue | why first |
| --- | --- | --- |
| 1 | `extract-lines-flag` | `worktrees.ts:43256` is byte 43256 of a 58252-byte file, which is line 1042. Every span in every verb is unclickable today. |
| 2 | this issue, `--callers` arm | "who calls X" is the most common question and is the same join with columns swapped |
| 3 | this issue, `--from` and `--uses` arms | ride 1 and 2 |
| 4 | `cdg-facet-and-throw-edges` | research increment, must not hold 1-3 hostage |

## Verb shape

Verbs, not flags. `extract.rs:578,588,596` already dispatch `watch`, `move`, `rename` as subcommands, while `extract.rs:325-326` bolts `fast`/`slow` on as argv rewrites into `--family`. This issue adds a real verb and leaves both spellings working.

```
extract graph --from PATH:NAME [--depth N] [--kind call|type|both] [PATH...]
extract graph --callers NAME                                       [PATH...]
extract graph --uses TYPE                                          [PATH...]
extract graph --cfg PATH:NAME
```

Bare `extract FILE` keeps today's per-file behavior. `extract resolve A B C` is the verb spelling of `--resolve`; the flag stays as an alias so the 228 in-repo call sites do not move.

New records:

```
record=graph_node   path=<string>  name=<string|null>  depth=<u32>  grade=<+|~|->  line=<u32|null>
record=graph_edge   from_path  from_name  to_path  to_name  kind=<slug>  grade=<+|~|->
record=graph_root   path  name  span={start,end}  found=<bool>
```

## No refusals

A state that cannot answer prints what it can plus the commands that would answer it. Pattern taken from `~/projects/hafley-rxjs/packages/bewpp` (`cli/5_render.ts:8`, `cli/6_expand.ts:20-31`) and written up in `~/projects/plans/20260918.0.cli-philosophy.md`.

`extract --resolve --family call src/main.ts` alone returns 1 edge, rc=0, silent. The same command over the 3 files `main.ts` imports returns 6. The wrong answer is indistinguishable from the right one. The fix is not a refusal:

```
$ extract graph --from src/main.ts:main
1 edge, 1 file. 3 imports leave the supplied set        :-a

next
  extract graph --from src/main.ts:main --expand    close the universe, +3 files
  extract x a                                       the 3 unsupplied import targets
  extract graph --from src/main.ts:main --grade +   only edges followed through a real import
```

`--expand` is `expand` from rxjs: emit, feed the output back as input, stop when nothing new appears.

```
step 0  set={main.ts}                            imports -> worktrees.ts, tabs.ts
step 1  set={main.ts, worktrees.ts, tabs.ts}     imports -> 0_boopGraph.ts
step 2  set={..., 0_boopGraph.ts}                imports -> {} already in set
step 3  fixpoint, resolve over the closed set
```

Terminates at a fixpoint, not a base case. Budget in tokens, `ceil(bytes / 4)`, resolution order copied from `bewpp/src/cli/1_config.ts:39`: `--budget` flag, `EXTRACT_BUDGET` env, `extract.config.json`, default.

Handles are cursors, never stored rows. Extract is "per-file, parallel, pure, cacheable" (`AGENTS.md:3`), so `{argv, input digests, offset}` re-derives the page. dl8 owns row storage.

## Grades

`resolution_origin` already carries confidence. Surface it as one column, and let the reader filter.

| grade | origin | rows, 7-file run | trust |
| --- | --- | --- | --- |
| `+` | `module_plane` | 22 | followed a real import |
| `~` | `corpus_unique` | 199 | bare name matched once across the corpus; wrong if it ever repeats |
| `-` | unresolved | 494 | no answer |

90% of edges are name-match guesses and nothing in the output says so today.

## Acceptance Criteria
- [ ] `extract graph --from src/main.ts:main --expand` reaches a fixpoint and reports which files it pulled in
- [ ] `extract graph --callers NAME` returns reverse edges with the same grade column
- [ ] `extract graph --uses TYPE` returns the `param`/`returns`/`uses`/`field` rows for that type
- [ ] every `graph_node` and `graph_edge` row carries `grade`; a summary line prints the `+`/`~`/`-` split
- [ ] no input path exits with a refusal; an unclosed universe prints a `next` block naming `--expand`
- [ ] `--from` naming a missing definition emits `graph_root found=false`, exit 0
- [ ] a golden over a fixture directory closed under `resolved_import`, not the instant corpus
- [ ] depth counts agree with the recursive-CTE oracle over the same fixture
- [ ] `cargo test --features cli` green

## Tests Run

## Implementation Notes

Reachability is a fixpoint, so the trace terminates at a steady state. Cycles are real: the worklist keeps a visited set keyed by `(path, name)` and the first depth wins. Measured 2026-09-18: keying by `(path, name)` and by `name` alone give identical depth counts on the instant corpus, so the key is not the variable; the corpus is. A 7-file run gives 1/6/13/10 and a 3-file run gives 1/5/13/10, which is why the golden moves to a closed fixture.

Do not build a persistent cross-run index (`AGENTS.md:39`). One resolve pass in memory, traverse, print, exit.

Split out of this issue: `--lines` (`extract-lines-flag`), post-dominance and CDG and `--slice` (`cdg-facet-and-throw-edges`), the sqlite `unresolved` doc fix (`sqlite-resolve-contract`), and the per-language default family set (`cst-out-of-default`).

## Decisions

### 2026-09-18T20:16:38Z · @chris

Session 2026-09-18 design pass. Fable review overruled on cutting --from/--uses (user: extract must be as capable as possible). Split out: --lines, CDG+throw edges, sqlite contract, cst default. Ordering flipped so --lines and --callers ship first. No refusals anywhere; HATEOAS next-block pattern from bewpp. Handles are cursors, not caches. Full philosophy at ~/projects/plans/20260918.0.cli-philosophy.md.
