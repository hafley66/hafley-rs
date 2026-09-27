---
created: 2026-09-19
updated: 2026-09-20
type: feature
status: wontfix
priority: high
epic: extract-parity-move-rename
related: ['@lab-scopegraph-queries', '@extract-graph-verb']
labels: [extract]
closed: 2026-09-20
---

# lab: run the AI context tools against one corpus and size what ryi fast absorbs

## Description

Goal: find any case, drawn from `sprefa-extract`'s own test corpus, where a
surveyed AI-coding-context tool answers a question `ryi fast` gets wrong or
misses, so the lab produces evidence rather than a survey table. Pattern: one
sonnet lane per tool, one at a time, each wagging its tail trying to make that
tool solve a fixed set of CTF-style cases drawn from `ryi fast`'s own test
cases, to see if any tool beats fast mode on any case. An opus lane designs
the common lab test interface every tool lab adheres to first, so the result
is one table: case x impl.

### What was measured 2026-09-19

Every claim below came from reading the tool's source, not its README.

| tool | static structure | mechanism | ranking | budgeting |
| --- | --- | --- | --- | --- |
| Aider | file-to-file graph, ident-labeled edges | vendored `*-tags.scm` | **PageRank, personalized** | binary search over ranked-tag prefix |
| Continue | flat chunks | web-tree-sitter, **hardcoded node-type lists, no `.scm`** | none | per-chunk token ceiling by body collapse |
| Repomix | flat per-file outline | WASM tree-sitter + per-language `.scm` + strategy classes | none | compress mode strips bodies to signatures |
| Zed | flat per-buffer outline | `outline.scm`, `runnables.scm` | fuzzy string match | none, it is editor UI |
| Cursor | AST-boundary chunks | tree-sitter chunking + Merkle re-sync | embedding similarity | AST-merge rule |
| Codex CLI | **none** | tree-sitter only parses shell commands for sandbox policy | n/a | n/a |
| Claude Code | **none, by design** | agentic grep/glob/read per turn | n/a | no index |
| Cline | contract only | `list_code_definition_names` declared, impl absent from public source | unverified | unverified |
| Sourcegraph SCIP | compiler-exact symbol graph | per-language compiler indexers | none, exact | n/a |
| Serena (MCP) | live symbol graph | real LSP per language, no tree-sitter fallback | n/a | n/a |

### Aider's mechanism, the only real ranking found

`aider/repomap.py`, 867 lines.

Nodes are FILES, not symbols: `G = nx.MultiDiGraph()` at `:470`, keyed by
`rel_fname`. One edge `referencer -> definer` per identifier referenced in one
file and defined in another. Weight is `mul * sqrt(num_refs)`:

| condition | multiplier |
| --- | --- |
| identifier mentioned in chat | 10x |
| long snake/kebab/camel name, >=8 chars | 10x |
| identifier starts with `_` | 0.1x |
| identifier has more than 5 definers | 0.1x |
| referencer already open in chat | 50x |

Then `nx.pagerank(G, weight="weight", personalization=...)` at `:525`, with
chat-open files given `100/num_files` personalization mass. Each file's rank is
redistributed across its out-edges proportional to weight and summed per
`(definer_file, identifier)`, which is how a file-level graph produces a ranked
list of identifier DEFINITIONS.

Budget: binary search over how many top-ranked tags to include, starting at
`max_map_tokens // 25` (`:676`), rendering and counting tokens each iteration,
converging within 15%.

Tags come from `aider/queries/tree-sitter-language-pack/<lang>-tags.scm`.
Where a grammar yields only definitions, pygments lexing backfills references.

Cache: sqlite keyed on `(fname, mtime)`.

## The cases

Every fixture path is relative to `crates/sprefa-extract`. `ryi fast` is the
`fast` CLI alias, pinned to `--family diet_scip` (`src/bin/ryi.rs:308-320`);
`--deps` is the diet module-graph resolver, a separate flag over the same
parse pass. Commands below were run from `crates/sprefa-extract` against
`target/debug/ryi`.

| case id | fixture path | question | ryi fast answer today | scoring rule |
| --- | --- | --- | --- | --- |
| chain-receiver-call | `tests/fixtures/rust_call_grind/{widget.rs,decoy.rs}` | who does `chain_caller`/`hop_caller` call, resolving `Gauge::read` through a same-file return and a local binding | `ryi fast widget.rs decoy.rs` emits 6 `resolved_edge` rows, both callers reaching `tick` and `read` | exact set match on (caller, callee) pairs |
| variant-literal-not-call | `tests/fixtures/rust_call_grind/shapes.rs` | does `build_circle()` call the `Circle` variant (a struct-variant literal must not mint a call) | `ryi fast shapes.rs` emits zero `build_circle -> Circle` rows | exact set match against the empty set |
| kotlin-ambiguous-receiver | `tests/fixtures/kotlin_receivers/{use.kt,lib.kt}` | receiver type of `h.w.run()` in `fieldLeg`, where `Widget.run` and `Decoy.run` share a bare name | `ryi fast use.kt lib.kt` binds `fieldLeg -> lib.kt:323-342` (`Widget.run`), never `Decoy` | exact answer, single target span |
| kotlin-shadow-decline | same fixture as above | `shadow(run: () -> Int)` calling `run()`: does fast decline, and with what reason | same command emits `{"record":"unresolved","family":"call","reason":"inferred"}` at span 752-755 | exact match on reason string |
| rename-safe-occurrence-set | `tests/fixtures/rust_rename/fnuse/before/src/{lib.rs,util.rs}` | renaming `util::Helper` to `Tool`, which sites are safe to touch (must exclude the local shadow `struct Helper` and `fn b()`) | `ryi fast lib.rs util.rs` resolves exactly one call edge (`a`'s `Helper::new()` into `util.rs`) and one type edge; `fn b()`'s local `Helper` mints nothing | exact set match |
| deps-reach-app | `tests/fixtures/deps/*` | which files does `app.ts` reach, and by what import kind | `ryi --deps --project-root tests/fixtures/deps <files>` emits 9 `file_edge` rows plus 3 `file_unresolved` stop rows | exact set match on (dst_path, kind, symbols) |
| spelled-receiver-trait-bound | `tests/fixtures/rust_spelled_receiver/src/proj.rs` | receiver type of `p.run()` in `trait_bound_leg<P: Proj>(p: P)` | `ryi fast proj.rs` binds `trait_bound_leg -> proj.rs:185-202` (`Proj::run`) | exact answer, single target span |
| spelled-receiver-field-chain | same fixture as above | receiver type of `b.inner.run()` in `field_leg(b: &Box)`, a two-hop field access | same command binds `field_leg -> proj.rs:330-371` (`Widget::run`) | exact answer, single target span |
| go-field-promotion | `tests/fixtures/go_field_promote/{caller,lib,base}/*.go` | what does `o.Part.Ring()` resolve to, through `Outer`'s embedded `Inner` field | `ryi fast caller.go lib.go base.go` binds `UseOuter -> base.go Widget::Ring`, `resolution_origin:"alias_chain"` | exact answer, single target span |
| macro-cross-file-miss | `tests/fixtures/rust_macro_callers/{user.rs,decoys.rs,macros.rs,local.rs}` | who do `Widget::alpha`/`beta`/`gamma` (macro-minted in a separate file) call | `ryi fast user.rs decoys.rs macros.rs local.rs` emits ONE row total, `local.rs`'s same-file macro expansion; the three cross-file mints in `user.rs` produce no row and no `unresolved` row | exact set match against a 3-row expected set (a real gap, not a placeholder) |

### Receipts

```
$ ./target/debug/ryi fast tests/fixtures/rust_call_grind/widget.rs tests/fixtures/rust_call_grind/decoy.rs
{"record":"resolved_edge","caller_path":"tests/fixtures/rust_call_grind/widget.rs","caller_name":"chain_caller","callee_path":"tests/fixtures/rust_call_grind/widget.rs","callee_name":"new","caller_site_start":270,"caller_site_end":281,"callee_start":45,"callee_end":81,"kind":"name_resolve","resolution_origin":"self_type"}
{"record":"resolved_edge","caller_path":"tests/fixtures/rust_call_grind/widget.rs","caller_name":"chain_caller","callee_path":"tests/fixtures/rust_call_grind/widget.rs","callee_name":"read","caller_site_start":302,"caller_site_end":306,"callee_start":183,"callee_end":219,"kind":"name_resolve","resolution_origin":"receiver"}
{"record":"resolved_edge","caller_path":"tests/fixtures/rust_call_grind/widget.rs","caller_name":"chain_caller","callee_path":"tests/fixtures/rust_call_grind/widget.rs","callee_name":"tick","caller_site_start":284,"caller_site_end":288,"callee_start":94,"callee_end":136,"kind":"name_resolve","resolution_origin":"receiver"}
{"record":"resolved_edge","caller_path":"tests/fixtures/rust_call_grind/widget.rs","caller_name":"hop_caller","callee_path":"tests/fixtures/rust_call_grind/widget.rs","callee_name":"new","caller_site_start":358,"caller_site_end":369,"callee_start":45,"callee_end":81,"kind":"name_resolve","resolution_origin":"self_type"}
{"record":"resolved_edge","caller_path":"tests/fixtures/rust_call_grind/widget.rs","caller_name":"hop_caller","callee_path":"tests/fixtures/rust_call_grind/widget.rs","callee_name":"read","caller_site_start":414,"caller_site_end":418,"callee_start":183,"callee_end":219,"kind":"name_resolve","resolution_origin":"receiver"}
{"record":"resolved_edge","caller_path":"tests/fixtures/rust_call_grind/widget.rs","caller_name":"hop_caller","callee_path":"tests/fixtures/rust_call_grind/widget.rs","callee_name":"tick","caller_site_start":396,"caller_site_end":400,"callee_start":94,"callee_end":136,"kind":"name_resolve","resolution_origin":"receiver"}
{"record":"resolved_type_edge","owner_path":"tests/fixtures/rust_call_grind/decoy.rs","owner_name":"Other","owner_start":11,"owner_end":16,"target_path":"tests/fixtures/rust_call_grind/decoy.rs","target_name":"Other","kind":"uses","resolution_origin":"same_file"}
{"record":"resolved_type_edge","owner_path":"tests/fixtures/rust_call_grind/widget.rs","owner_name":"Gauge","owner_start":151,"owner_end":156,"target_path":"tests/fixtures/rust_call_grind/widget.rs","target_name":"Gauge","kind":"uses","resolution_origin":"same_file"}
{"record":"resolved_type_edge","owner_path":"tests/fixtures/rust_call_grind/widget.rs","owner_name":"Widget","owner_start":11,"owner_end":17,"target_path":"tests/fixtures/rust_call_grind/widget.rs","target_name":"Widget","kind":"uses","resolution_origin":"same_file"}
```

```
$ ./target/debug/ryi fast tests/fixtures/rust_call_grind/shapes.rs
{"record":"resolved_edge","caller_path":"tests/fixtures/rust_call_grind/shapes.rs","caller_name":"origin","callee_path":"tests/fixtures/rust_call_grind/shapes.rs","callee_name":"Point","caller_site_start":242,"caller_site_end":247,"callee_start":68,"callee_end":73,"kind":"import_resolve","resolution_origin":"module_plane"}
```

(`build_circle` mints no row at all: `origin -> Point`, a plain struct
literal, is the only call-shaped row `shapes.rs` produces.)

```
$ ./target/debug/ryi --deps --project-root tests/fixtures/deps tests/fixtures/deps/app.ts tests/fixtures/deps/side.ts tests/fixtures/deps/widget/index.ts tests/fixtures/deps/lib/bare.ts tests/fixtures/deps/lib/helper.ts tests/fixtures/deps/lib/util.ts tests/fixtures/deps/lib/mapped.ts
{"record":"file_edge","src_path":"app.ts","dst_path":"lib/bare.ts","kind":"named","symbols":1}
{"record":"file_edge","src_path":"app.ts","dst_path":"lib/bare.ts","kind":"namespace","symbols":1}
{"record":"file_edge","src_path":"app.ts","dst_path":"lib/helper.ts","kind":"named","symbols":2}
{"record":"file_edge","src_path":"app.ts","dst_path":"lib/mapped.ts","kind":"named","symbols":1}
{"record":"file_edge","src_path":"app.ts","dst_path":"lib/util.ts","kind":"default","symbols":1}
{"record":"file_edge","src_path":"app.ts","dst_path":"lib/util.ts","kind":"named","symbols":1}
{"record":"file_edge","src_path":"app.ts","dst_path":"lib/util.ts","kind":"reexport","symbols":1}
{"record":"file_edge","src_path":"app.ts","dst_path":"side.ts","kind":"side_effect","symbols":1}
{"record":"file_edge","src_path":"app.ts","dst_path":"widget/index.ts","kind":"named","symbols":1}
{"record":"file_unresolved","src_path":"app.ts","module":"./gone.ts","reason":"relative_unresolved"}
{"record":"file_unresolved","src_path":"app.ts","module":"/lib/util.ts","reason":"absolute_path"}
{"record":"file_unresolved","src_path":"app.ts","module":"rxjs","reason":"node_modules_boundary"}
```

```
$ ./target/debug/ryi fast tests/fixtures/go_field_promote/caller/caller.go tests/fixtures/go_field_promote/lib/lib.go tests/fixtures/go_field_promote/base/base.go
{"record":"resolved_edge","caller_path":"tests/fixtures/go_field_promote/caller/caller.go","caller_name":"UseOuter","callee_path":"tests/fixtures/go_field_promote/base/base.go","callee_name":"Ring","caller_site_start":376,"caller_site_end":387,"callee_start":128,"callee_end":176,"kind":"name_resolve","resolution_origin":"alias_chain"}
{"record":"resolved_import","src_path":"tests/fixtures/go_field_promote/caller/caller.go","name":"*","local":"lib","target_path":"tests/fixtures/go_field_promote/lib","target_name":"lib","kind":"local","hops":0}
{"record":"resolved_import","src_path":"tests/fixtures/go_field_promote/lib/lib.go","name":"*","local":"base","target_path":"tests/fixtures/go_field_promote/base","target_name":"base","kind":"local","hops":0}
{"record":"resolved_type_edge","owner_path":"tests/fixtures/go_field_promote/lib/lib.go","owner_name":"Inner","owner_start":208,"owner_end":243,"target_path":"tests/fixtures/go_field_promote/base/base.go","target_name":"Widget","kind":"field","resolution_origin":"module_plane"}
{"record":"resolved_type_edge","owner_path":"tests/fixtures/go_field_promote/lib/lib.go","owner_name":"Outer","owner_start":250,"owner_end":273,"target_path":"tests/fixtures/go_field_promote/lib/lib.go","target_name":"Inner","kind":"impl","resolution_origin":"same_file"}
```

```
$ ./target/debug/ryi fast tests/fixtures/rust_macro_callers/user.rs tests/fixtures/rust_macro_callers/decoys.rs tests/fixtures/rust_macro_callers/macros.rs tests/fixtures/rust_macro_callers/local.rs
{"record":"resolved_edge","caller_path":"tests/fixtures/rust_macro_callers/local.rs","caller_name":"alpha","callee_path":"tests/fixtures/rust_macro_callers/local.rs","callee_name":"helper_two","caller_site_start":407,"caller_site_end":428,"callee_start":437,"callee_end":466,"kind":"name_resolve","resolution_origin":"same_file"}
```

(`Widget::alpha`, `Widget::beta`, `Widget::gamma`, minted by the
`mint_helpers!`/`mint_single!` macros invoked in `user.rs` from a body defined
in `macros.rs`, mint zero rows and zero `unresolved` rows. `stderr` carries no
warning either. This is the strongest candidate for a tool beating fast: any
tool that source-greps the macro body text would at least surface the three
call names, which fast does not emit today.)

## Lab interface (opus designs, every lane obeys)

Sketch only. The opus lane (`lab-interface`) owns the final shape; this is the
shared contract every sonnet tool-lane is handed before it starts.

Directory: `crates/sprefa-lab-bakeoff/`, its own tree, zero edits under
`crates/sprefa-extract/src`.

```
crates/sprefa-lab-bakeoff/
  labs/
    aider/run.sh      <case-id>   -> out/aider/<case-id>.json
    repomix/run.sh     <case-id>   -> out/repomix/<case-id>.json
    continue/run.sh    <case-id>   -> out/continue/<case-id>.json
    serena/run.sh      <case-id>   -> out/serena/<case-id>.json
  expected/
    <case-id>.json                (lab-interface writes these, derived from ryi fast + hand-check)
  out/
    <tool>/<case-id>.json         (each tool-lane writes only its own subtree)
  score/                          (lab-interface owns; diffs out/ against expected/)
```

Answer schema, one JSON object per `out/<tool>/<case-id>.json`:

```rust
/// One tool's answer to one case. `answer` is the canonical, orderless site
/// set the scoring rule diffs against `expected/<case>.json`: a
/// `file:start-end` span string where the case has one, a `(caller,callee)`
/// pair string where it has a set. `notes` carries a `cannot` reason when the
/// tool has no way to answer (e.g. macro expansion, cross-language receiver
/// typing).
struct CaseAnswer {
    case: String,
    answer: Vec<String>,
    notes: Option<String>,
}
```

TypeSpec equivalent:

```
model CaseAnswer {
  case: string;
  answer: string[];
  notes: string | null;
}
```

`score`, pseudo-code:

```
for case_file in expected/*.json:
  case = case_file.stem
  expected_set = load(case_file).answer as Set
  row = { case }
  for tool in labs/*:
    out_file = out/{tool}/{case}.json
    if not exists(out_file):
      row[tool] = "cannot (no out file)"
      continue
    got = load(out_file)
    got_set = got.answer as Set
    if got_set == expected_set:
      row[tool] = "match"
    else:
      row[tool] = "diff +{got_set - expected_set} -{expected_set - got_set}"
      if got.notes == "cannot":
        row[tool] = "cannot: " + got.notes
  emit_row(row)
print_table(rows, columns = case x [ryi-fast, aider, repomix, continue, serena])
```

`ryi-fast`'s own column in `out/ryi-fast/<case>.json` is written by
`lab-interface` too, from the commands already verified in the Receipts
section above, so the table always has a working baseline column even before
any tool-lane starts.

## Lanes

| lane | model | owns | done when |
| --- | --- | --- | --- |
| lab-interface | opus | `CaseAnswer` schema, `score`, `expected/*.json` for every case, `out/ryi-fast/*.json`, the `just lab-score` recipe | `just lab-score` prints a table with a `ryi-fast` column filled for every case |
| aider | sonnet | `labs/aider/` only | every case has `out/aider/<case-id>.json`; table row filled (a cell may say `cannot`, reason in `notes`) |
| repomix | sonnet | `labs/repomix/` only | every case has `out/repomix/<case-id>.json`; table row filled (`cannot` allowed, reason in `notes`) |
| continue | sonnet | `labs/continue/` only | every case has `out/continue/<case-id>.json`; table row filled (`cannot` allowed, reason in `notes`) |
| serena | sonnet | `labs/serena/` only | every case has `out/serena/<case-id>.json`; table row filled (`cannot` allowed, reason in `notes`) |

## Acceptance Criteria

- [ ] frozen corpus sha and file count recorded
- [ ] `expected/` answers derived from `ryi fast` and hand-checked, one file per case
- [ ] every tool row filled: aider, repomix, continue, serena
- [ ] a written verdict per case naming any tool that beat fast, or stating none did
- [ ] `ryi slow` untouched, and the verdict says so explicitly
- [ ] zero edits under `crates/sprefa-extract/src`

## Tests Run

## Implementation Notes

## Comments

## Decisions

### 2026-09-20T18:05:50Z · @chris

One sonnet lane per tool, one at a time, each wagging its tail trying to make that tool solve a fixed set of CTF-style cases drawn from ryi fast's own test cases, to see if any tool beats fast mode on any case. One opus lane first designs the common lab test interface every tool lab adheres to, so the result is one table: case x impl.

