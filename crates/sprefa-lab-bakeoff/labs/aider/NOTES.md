# aider lane notes

Issue: `@lab-context-tool-bakeoff`. Tool: aider 0.86.2 (`aider-chat`), python 3.12
venv at `labs/aider/.venv` (python 3.14 fails the build: no setuptools backend).
`uv venv --python 3.12 labs/aider/.venv && uv pip install --python labs/aider/.venv/bin/python aider-chat`.

## Files

| file | job |
| --- | --- |
| `run.sh <case>` | map.sh, then cases.py; writes `out/aider/<case>.json` |
| `map.sh` | mechanism 1: `aider --show-repo-map` on a scratch git repo of the case's fixture files |
| `tags.py` | mechanism 2: every def/ref tag with byte spans, from aider's own `.scm` and grammar |
| `cases.py` | mechanism 3: pairs and sites from the tags; per-case answer and notes |
| `rank.py` | mechanism 2b: `RepoMap.get_ranked_tags` order for a case |
| `out/aider/maps/` | mechanism 1 output per case |
| `out/aider/tags/` | tag dump per case: path, kind, tag, name, line, node span, name span |
| `out/aider/rank/` | rank.py output for the four name-join cases |

## Mechanisms tried

| # | mechanism | result |
| --- | --- | --- |
| 1 | `--show-repo-map --map-tokens 8192` | Prints file-grouped def lines plus their enclosing context. No call edge, no line numbers, no byte spans. Run in the worktree it maps 1062 files and litters `.aider.*`; map.sh runs it in a scratch repo instead. Not parseable into any answer shape. Kept as evidence. |
| 2 | `RepoMap.get_tags_raw` | Tag(name, kind, line). tags.py re-runs the same query on the same tree to recover node spans; cases.py asserts (name, kind, line) equals aider's own list for every file. |
| 2b | `RepoMap.get_ranked_tags` | Orders files, then defs. Never picks between two defs of one name: chain-receiver-call lists decoy.rs first (the chat file's own defs are excluded), kotlin lists `Decoy.run` (lib.kt:23) before `Widget.run` (lib.kt:9). Order carries no receiver information. Not used. |
| 3 | pair each `reference.call` in a fn body with the same-named fn defs | Answer source for every non-`cannot` case. |

## Span conversion

aider Tag has `line` only. tags.py takes the `@name.definition.*` capture and the
smallest `@definition.*` capture that contains it.

| language | def span | why |
| --- | --- | --- |
| rust | name node start to `@definition.*` node end | matches expected: `new` 45-81 has name start 45, `pub` starts at 38 |
| go, kotlin | `@definition.*` node start to node end | matches expected: go `func` at 128, kotlin `fun` at 323 |
| any, name-join site | `@name.*` node span | rename case: `Helper` at util.rs 11-17 |

The method pattern and the function pattern of `rust-tags.scm` both match a fn
inside `impl`; tags.py dedupes on (kind, name, span).

Caller of a call ref: smallest function/method def whose node span contains the
ref. Callee: every fn/method def tagged with the ref's name, in the case's files.
aider joins refs to defs on the bare name only (`repomap.py`, `defines[ident]`).
Where several defs share the name, every one is emitted; no candidate is chosen.

## Case rows

| case | mechanism used | what aider saw | why the entries are what they are |
| --- | --- | --- | --- |
| chain-receiver-call | 3 over 2 | refs `tick` x2, `read` x2 in widget.rs; defs `tick`, `read` in widget.rs and decoy.rs | Name-join emits both files' defs (4 extra). `Widget::new()` has a scoped_identifier callee, which the rust query does not tag, so both `->new` pairs are missing (2 missing). |
| variant-literal-not-call | 3 | zero refs in shapes.rs | `Shape::Circle { radius: 1 }` is a struct_expression, untagged. No call ref, so no pair; empty set equals expected by absence, no receiver reasoning. |
| macro-cross-file-miss | 1, 2, 3 | refs `mint_helpers`, `mint_single` in user.rs; macro defs in macros.rs and local.rs; no fn def named alpha, beta, gamma | Macro bodies are token trees. `cannot`: cases.py asserts no minted fn def and no pair exists. |
| go-field-promotion | 3 over 2, 2b | ref `Ring` in UseOuter (caller.go:14); defs `Ring` in caller.go:10 (Decoy) and base.go:7 (Widget) | Name-join emits both defs; base.go:128-176 matches, caller.go's decoy is the extra. `o.Part` has no field-type tag, so aider cannot pick. Rank lists only base.go's defs because caller.go is the chat file. |
| kotlin-ambiguous-receiver | 3 over 2, 2b | ref `run` in fieldLeg (use.kt:21); defs `run` at lib.kt:9 (Widget), lib.kt:23 (Decoy), use.kt:30 (Inner) | Name-join emits all three; lib.kt:323-342 matches, two extras. Rank puts Decoy.run above Widget.run. |
| spelled-receiver-field-chain | 3 | ref `run` in field_leg (proj.rs:39); one fn def `run` (proj.rs:25) | `fn run(&self) -> u32;` in `trait Proj` is a function_signature_item, which the rust query does not tag, so the join has one candidate. Match holds because the trait def is invisible; aider did not read `b.inner`. |
| spelled-receiver-trait-bound | 3 | ref `run` in trait_bound_leg (proj.rs:48); same single def proj.rs:25 | Same join emits Widget::run 330-371; expected is the untagged Proj::run signature 185-202. diff +1 -1. |
| rename-safe-occurrence-set | 3 (name-join) | tags named `Helper`: defs at lib.rs:3 and util.rs:1, ref `implementation` at util.rs:3 | aider has no scopes, so every tag named `Helper` is emitted at its name span. util.rs:11-17 matches. lib.rs's own struct and the `impl Helper` ref are extras; `Helper::new()` in fn a is a scoped callee, untagged, so lib.rs:69-80 is missing. diff +2 -1. |
