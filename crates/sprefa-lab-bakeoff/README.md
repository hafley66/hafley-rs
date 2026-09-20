# sprefa-lab-bakeoff

One table, case x tool. Ten CTF-style cases drawn from `sprefa-extract`'s own
fixtures; `ryi fast` is the baseline column. Issue: `@lab-context-tool-bakeoff`.

```
just lab-score
```

## Layout

| path | owner | contents |
| --- | --- | --- |
| `expected/<case-id>.json` | lab-interface | the answer every tool is scored against |
| `out/ryi-fast/<case-id>.json` | lab-interface | what `ryi fast` emits today |
| `out/<tool>/<case-id>.json` | that tool's lane | what the tool emits |
| `labs/<tool>/run.sh` | that tool's lane | produces its own `out/<tool>/*.json` |
| `src/bin/score.rs` | lab-interface | diffs `out/` against `expected/`, prints the table |

## Schema

One `CaseAnswer` per file (`src/lib.rs`):

```json
{ "case": "<case-id>", "answer": ["<entry>", "..."], "notes": null }
```

`answer` is orderless and compared as a set. A `notes` string starting with
`cannot` marks a tool that has no way to answer; the cell then reads
`cannot: <notes>` instead of a diff. A missing `out` file reads
`cannot (no out file)`.

## Entry grammar

Paths are relative to `crates/sprefa-extract`. Spans are byte offsets, copied
from the record `ryi` emits, `start` inclusive and `end` exclusive.

| shape | form | used by |
| --- | --- | --- |
| site | `<path>:<start>-<end>` | receiver and rename cases |
| call pair | `<caller>-><callee>@<path>:<start>-<end>` | call-set cases |
| decline | `<path>:<start>-<end> unresolved:<reason>` | kotlin-shadow-decline |
| module edge | `<dst_path>\|<kind>\|<symbols>` | deps-reach-app |
| module stop | `<module>\|unresolved:<reason>` | deps-reach-app |

The span in a call pair is the callee's definition span, which is what makes a
bind to a same-named decoy score as a diff instead of a match.

## Cases

| case id | shape | expected entries | command |
| --- | --- | --- | --- |
| chain-receiver-call | call pair | 6 | `ryi fast tests/fixtures/rust_call_grind/{widget,decoy}.rs` |
| deps-reach-app | module edge, module stop | 12 | `ryi --deps --project-root tests/fixtures/deps <7 files>` |
| go-field-promotion | site | 1 | `ryi fast tests/fixtures/go_field_promote/{caller/caller,lib/lib,base/base}.go` |
| kotlin-ambiguous-receiver | site | 1 | `ryi fast tests/fixtures/kotlin_receivers/{use,lib}.kt` |
| kotlin-shadow-decline | decline | 1 | same command |
| macro-cross-file-miss | call pair | 3 | `ryi fast tests/fixtures/rust_macro_callers/{user,decoys,macros,local}.rs` |
| rename-safe-occurrence-set | site | 2 | `ryi fast tests/fixtures/rust_rename/fnuse/before/src/{lib,util}.rs` |
| spelled-receiver-field-chain | site | 1 | `ryi fast tests/fixtures/rust_spelled_receiver/src/proj.rs` |
| spelled-receiver-trait-bound | site | 1 | same command |
| variant-literal-not-call | call pair | 0 | `ryi fast tests/fixtures/rust_call_grind/shapes.rs` |

`macro-cross-file-miss` is the one case whose `expected` set is not what `ryi
fast` emits: fast mints zero rows for the three macro-minted methods, so its
column reads `diff +0 -3`. Every other expected set is `ryi fast`'s own output,
hand-checked against the fixture.

## Adding a tool lane

1. Write `labs/<tool>/run.sh`, taking a case id, writing
   `out/<tool>/<case-id>.json`.
2. Emit one `CaseAnswer` per case, entries in the grammar above.
3. `just lab-score`.
