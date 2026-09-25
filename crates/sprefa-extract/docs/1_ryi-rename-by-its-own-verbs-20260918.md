# Renaming the `extract` binary to `ryi` with `move` and `rename`

Record of the 2026-09-18 rename, run on branch `feat/ryi-rename` off `39189edb`
in the worktree `~/projects/hafley-rs-wt/ryi`.

## The job

| scope | decision | count |
| --- | --- | --- |
| `[[bin]]` target `extract` -> `ryi` | rename | 1 manifest line |
| `src/bin/extract.rs`, `src/bin/extract/` -> `ryi` | move | 3 files |
| `CARGO_BIN_EXE_extract` -> `CARGO_BIN_EXE_ryi` | mechanical | 183 hits / 129 files |
| crate name `sprefa-extract` | unchanged | 211 files untouched |
| bare word `extract` as the English verb | unchanged | 1228 hits / 279 files |
| `Source::extract` across 11 language front-ends | unchanged | trait contract |

## Locating the brand words

`grep` was the wrong instrument and produced `<no decl found>` for six of nine
names. The binary answers it directly: `node` records carry `kind` and `name`.

```
extract --family cst <FILE>
  | grep '"kind":"(struct_item|enum_item|trait_item|type_item|function_item|mod_item|const_item|static_item)"'
  | grep -i '"name":"[A-Za-z_]*extract'
```

101 single-file runs returned 29 declarations carrying the brand word, with the
declaring file attached. Splitting brand from verb is the only judgment step;
everything else below is mechanical.

## Step trace

```
step 0  rename src/types.rs#ExtractOutput RyiOutput  --text-refs
        exit 6   no plan emitted
        stderr   src/bin/extract.rs byte 1905: path attr twice reaches the symbol
                 tests/0_sqlite.rs byte 56: path attr twice reaches the symbol

step 1  byte 1905 -> src/bin/extract.rs:40   #[path = "extract/0_sqlite.rs"]
        byte 56   -> tests/0_sqlite.rs:4     #[path = "../src/bin/extract/0_sqlite.rs"]

step 2  grep -rn '#\[path' src tests
        src/bin/extract.rs carries 9, six of them '../' into the lib's own files
        (0_query.rs, 0_move.rs, 2_move_text.rs, 3_region_writer.rs, 4_watch.rs,
        5_diff.rs, 0_rename.rs), so those compile into both lib and bin

step 3  move --list moves.tsv --text-refs        (dry run)
        exit 0
        plan     3 moves
        manifest Cargo.toml: bin[0].path src/bin/extract.rs -> src/bin/ryi.rs
        repair   src/bin/extract.rs:37,40 and tests/0_sqlite.rs:4
        report   19 text-refs, none rewritten

step 4  move --list moves.tsv --commit --verify "cargo test --features cli"
        exit 2   stderr: move source is not a file: .../src/bin/extract.rs
        tree     all three moves applied, all four repairs applied

step 5  sed Cargo.toml   name = "extract" -> name = "ryi"
        sed 129 files    CARGO_BIN_EXE_extract -> CARGO_BIN_EXE_ryi
        sed schema/README.md  --bin extract -> --bin ryi
        residue          0 and 0

step 6  cargo test --features cli
        exit 101   303 passed, 1 failed
        23_flow_cli_dispatch::bench_runs_the_cfg_pass_when_the_family_names_it
        "no bench event for [--bench --family cfg ...]"

step 7  tests/23_flow_cli_dispatch.rs:105  RUST_LOG=extract=info -> ryi=info
        the bench event's tracing target is the BIN CRATE name, so the rename
        silenced the filter and the event never reached stderr

step 8  cargo test --features cli --no-fail-fast
        exit 0     967 passed, 0 failed, same count as main at 39189edb
```

Terminates green at step 8.

## The one breakage no path or symbol tool can see

A `tracing` target is the crate name, and the bin crate's name is the `[[bin]]`
target name. `RUST_LOG=extract=info` became a filter matching nothing the moment
`name` changed, and the failure surfaced as a missing log event rather than a
compile error. One site:

| file:line | before | after |
| --- | --- | --- |
| `tests/23_flow_cli_dispatch.rs:105` | `RUST_LOG=extract=info` | `RUST_LOG=ryi=info` |

Neither `move` nor `rename` has a plane that reaches a string inside an env
filter. `grep -rn 'extract=' src tests` found it after the test failed.

## What a sed script would have broken

`scripts/rename-extract-to-ryi.sh` was written first and handles the manifest
`path` line and the 183 test handles. It rewrites none of these:

| file:line | literal before | literal after |
| --- | --- | --- |
| `src/bin/extract.rs:37` | `extract/help.rs` | `ryi/help.rs` |
| `src/bin/extract.rs:40` | `extract/0_sqlite.rs` | `ryi/0_sqlite.rs` |
| `tests/0_sqlite.rs:4` | `../src/bin/extract/0_sqlite.rs` | `../src/bin/ryi/0_sqlite.rs` |

`src/edit/rust_rehome.rs` is the module that repairs them.

## Text-refs reported and deliberately skipped

19 spellings survive in plain text. `--text-refs` reports them; the plan does not
touch them.

| location | count | reason skipped |
| --- | --- | --- |
| `plans/reviews/*.md`, `reports/*.md`, `docs/0_architecture-matrix-20260917.md` | 9 | historical records |
| `tests/fixtures/data/tables.toml:14` | 1 | standalone fixture manifest, not the real file |
| `tests/fixtures/data/goldens/tables.toml.jsonl` | 2 | the parse of that fixture |
| `tests/fixtures/kind_vocab/wire_golden.jsonl` | 6 | frozen golden, same fixture |

Both goldens are read by `tests/29_data_family.rs` and `tests/6_kind_vocab.rs`.
Rewriting them would have broken two test files to fix nothing.

## Defects surfaced

| # | site | behavior |
| --- | --- | --- |
| 1 | `move --commit --list` | applies every move, then exits 2 re-reading a source path that no longer exists |
| 2 | `rename` | exits 6 with no plan when `#[path]` gives two routes to one symbol |
| 3 | diagnostics | report byte offsets, not `file:line` |
| 4 | multi-file argv | `extract --family cst src` prints `src is a directory; --resolve takes files` when `--resolve` was never passed; an explicit file list then fails `exactly one PATH is required unless --resolve is given` |

## Cost

| approach | operations |
| --- | --- |
| `move` dry run, inspect, `--commit` | 3 invocations, 1 tsv of 3 rows |
| hand editing | 134 files, 188 insertions, 2169 deletions |

The manifest line and the 183 `CARGO_BIN_EXE_extract` handles fell outside both
verbs' semantic scope and were done with `sed`.
