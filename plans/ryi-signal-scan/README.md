# Signal usage scan over hafley-rxjs with multi-pattern `ryi query`

Corpus: `~/projects/hafley-rxjs`, `git ls-files '*.ts' '*.tsx'` = 1543 files, 6787748 bytes.
Binary: dev profile (crate opt-level 1, deps opt-level 3), M-series local, no rcargo.

## Run

```sh
cd ~/projects/hafley-rxjs
git ls-files '*.ts' '*.tsx' \
  | ryii query $(for f in ~/projects/hafley-rs/plans/ryi-signal-scan/*.scm; do printf -- '--scm %s ' "$f"; done) - \
  > signals.jsonl
```

Each row carries `"query": "<scm path>"` when more than one pattern runs.

## Patterns and hits

| file | shape | rows |
| --- | --- | --- |
| `0_signal_ctor.scm` | `Signal(...)` any form | 649 |
| `1_signal_computed.scm` | `Signal(() => ...)` | 163 |
| `2_signal_event.scm` | `Signal()` / `Signal<T>()` | 56 |
| `3_read.scm` | `x.$()` | 1654 |
| `4_write.scm` | `x.$(v)` | 810 |
| `5_immer.scm` | `x.setImmer(...)` | 0 |
| `6_meta.scm` | `x.$.$`, `x.$.path` | 11 |
| `7_helpers.scm` | `toSignal`/`useSignal`/`storageSignal`/`signalMap`/... | 124 |

`3_read`/`4_write` match any `.$(...)` receiver; no type check (bottom-up).

## Timing

| run | wall s |
| --- | --- |
| HEAD 4b27af3e, 8 processes, one `--query` each | 52.67 |
| HEAD, trivial query `(debugger_statement)`, 1 process | 3.21 |
| new, trivial query, 1 process (parse floor) | 0.92 |
| new, 8 processes, one `--scm` each | 7.50 |
| new, 1 process, all 8 `--scm` | 2.26 (median of 3: 2.52, 2.26, 2.27) |
| main `fb1d1fa7` (rayon pool), trivial query | 0.40 real / 1.31 user |
| main `fb1d1fa7`, all 8 `--scm` | 0.48 real / 2.93 user |

Row parity HEAD 8-run vs new 1-run: 3443 unique `(pattern, row)` both sides, sets equal.

Rows above `main` ran on branch `claude-375-py-cycle` (643 commits behind main, files read sequentially).
Cost split (branch binary, 1 process): parse ~0.92 s, each pattern's cursor pass ~0.17 s.
HEAD paid `hafley_scm::build` per file per pattern (1543 x 8 compiles, each
also building per-kind predicate queries) plus 8 x 1543 parses.
Next cut: merge the 8 patterns into one `Query` so one cursor pass per file.

## Release build, vscode `src` (11115 files, 173100279 bytes), main `fb1d1fa7`

| run | rows | real s | user s | max RSS |
| --- | --- | --- | --- | --- |
| trivial `(debugger_statement)` | 0 | 4.17 | 26.21 | 140 MB |
| 8 signal `.scm` | 3176 | 7.87 | 58.19 | 184 MB |
| 19 `heavy/*.scm` (no every-identifier) | 2799220 | 24.04 | 115.64 | 449 MB |
| 20 `heavy/*.scm` | 6581760 | 33.98 | 125.43 | 578 MB |

`sample` of the trivial run (3 s, 22060 stack-top samples):

| bucket | samples |
| --- | --- |
| `ts_*` (parse, lex, cursor) | 10192 |
| thread wait (`__psynch_cvwait`, `__semwait_signal`, `__ulock_*`) | 8411 |
| libc malloc/free (`tiny_*`, `free_tiny`, `_szone_free`) | 2430 |

Per extra pattern: (58.19 - 26.21) / 8 = 4.0 s user, one cursor walk of every tree.

## After the three fixes (release, vscode `src`, median of 3)

| run | before real / user | after real / user | max RSS after |
| --- | --- | --- | --- |
| trivial `(debugger_statement)` | 4.17 / 26.21 | 2.33 / 18.44 | 215 MB |
| 8 signal `.scm` | 7.87 / 58.19 | 2.61 / 19.69 | 212 MB |
| 20 `heavy/*.scm`, 6581760 rows | 33.98 / 125.43 | 4.93 / 37.50 | 874 MB |

Output byte-identical to `fb1d1fa7` for the 8-signal and 20-heavy sets (`cmp`).

| fix | where | measured step |
| --- | --- | --- |
| patterns merged into one query per language, rows mapped back by pattern index | `src/0_query.rs` `Merged` | 8 signal 7.87 -> 3.18 s |
| workers pull files from one counter, render JSONL, writer only reorders; replaces 1024-file chunks | `src/0_query.rs` `run_to` | (same build as above) |
| one write + flush per file; query runs `streaming`, which flushed per row (6.58M write syscalls) | `Output::stdout_block` | 20 heavy 14.25 -> 6.93 s |
| tree-sitter C allocator -> mimalloc (`tree_sitter::set_allocator`); Rust global allocator unchanged | `src/bin/ryi.rs` `main` | trivial 3.04 -> 2.33 s, heavy 6.93 -> 4.93 s |

Remaining profile of the trivial run: tree-sitter parse/lex (`ts_parser_parse`, `ts_lex`, `ts_stack_*`).
