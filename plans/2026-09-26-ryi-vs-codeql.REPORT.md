# ryi vs CodeQL baseline, lane Q

Implementation tip before this report: `454bc937`. CodeQL CLI 2.27.1; Rust pack 0.2.22; JavaScript pack 2.10.2; worktree ryi SHA-256 `daae7eff8ad3ddbe929556a71a4c53653687645322e4b7120b5ca081be86442a`.
Both tools read each frozen corpus. Rust queries exclude macro expansions. The four-column key is `(source file, enclosing item, target file, target name)`. Build means database creation. Times are seconds, sizes are MiB; Rust Cargo dependencies were cached. Rust type owner names were corrected and re-queried on the fresh databases after the timed runs.
Root extraction rewrites only a private copy of `ra_ap_parser-0.0.349/test_data/lexer/err/incomplete_frontmatter_before_unicode.rs`, whose `-│` makes CodeQL panic at a UTF-8 byte boundary.
The recorded timings predate the new 2048 MiB limits. Subsequent runs pass `RYI_MAX_MEM_MB=2048`, CodeQL `--ram=2048 --threads=4 -J=-Xmx2g`, and remove temporary databases on exit.

| Corpus (files) | Type agree / ryi-only / CodeQL-only | Call agree / ryi-only / CodeQL-only | ryi build / query / DB | CodeQL build / query / DB |
| --- | ---: | ---: | ---: | ---: |
| Root Rust (423) | 7126 / 13 / 1151 | 10568 / 2843 / 3191 | 18 / 1.338 / 604.7 | 109 / 873 / 1253.0 |
| sprefa Rust (552) | 1302 / 1142 / 415 | 3617 / 1818 / 605 | 9 / 0.462 / 261.4 | 53 / 38 / 296.4 |
| TypeScript (1651) | 11326 / 18600 / 6350 | 878 / 6241 / 3890 | 52 / 2.832 / 2272.1 | 87 / 31 / 676.3 |

Causes below partition each disagreement bucket by observable pattern. Sample verdict cells in `disagreements.tsv` remain blank for human adjudication. R means ryi-only; C means CodeQL-only.

| Bucket | Cause | Count | Example | Owner |
| --- | --- | ---: | --- | --- |
| Root type R | False local target; unresolved CodeQL type | 7; 6 | `TraceEvent`; `ReadRequest` | ryi defect; out of scope |
| Root type C | Missing ryi type edge | 1151 | `TurnEvent` | ryi defect |
| Root call R | Target path mismatch; synthetic caller; unresolved CodeQL static target | 6; 1357; 1480 | `rows`; `closure@`; `lookup` | ryi defect; out of scope; codeql query defect |
| Root call C | Missing ryi call edge | 3191 | `default` | ryi defect |
| sprefa type R | Cross-fixture or standard type bound locally; unresolved fixture/module | 362; 780 | `Result`; `MoveCx` | ryi defect; out of scope |
| sprefa type C | Missing ryi type edge | 415 | `Key` | ryi defect |
| sprefa call R | Cross-fixture target; synthetic caller; unresolved CodeQL target | 56; 666; 1096 | `load_config`; `closure@`; `as_id` | ryi defect; out of scope; codeql query defect |
| sprefa call C | Missing ryi call edge | 605 | `run` | ryi defect |
| TypeScript type R | Wrong generated peer; import/alias query gap | 5127; 13473 | `IBindPlanData`; `IGenProgram` | ryi defect; codeql query defect |
| TypeScript type C | Missing or wrong ryi target; alias query mismatch | 6194; 156 | `Fact`; `IHostColumnPlan` | ryi defect; codeql query defect |
| TypeScript call R | Wrong generated peer; synthetic caller; unresolved CodeQL callee | 97; 3925; 2219 | `applyArrivals`; `closure@`; `list_at_scalar_seam` | ryi defect; out of scope; codeql query defect |
| TypeScript call C | Missing ryi call edge | 3890 | `parseArgs` | ryi defect |

Gate: `PATH="$HOME/.local/bin:$PATH" CARGO_TARGET_DIR="$HOME/.cache/boop/cargo-target" CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RYI_MAX_MEM_MB=2048 cargo test --features cli --test 179_codeql_baseline -- --nocapture` from `crates/sprefa-extract`.

```text
running 1 test
test type_ladder_codeql_baseline ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 49.10s

```
