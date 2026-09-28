## API matrix

All paths below are under `~/.cache/lanes/claude-375/eval/` unless marked as the active worktree. RSS is peak resident memory of the command process tree sampled every 20 ms. JSON samples are truncated to one line or one source line. Telemetry is disabled with `DO_NOT_TRACK=1`, `SEMGREP_SEND_METRICS=off`, and `GRIT_TELEMETRY_DISABLED=true`.

### ast-grep 0.45.3

Docs: [CLI run](https://ast-grep.github.io/reference/cli/run), [CLI scan](https://ast-grep.github.io/reference/cli/scan), [CLI outline](https://ast-grep.github.io/reference/cli/outline), [tooling overview](https://ast-grep.github.io/guide/tooling-overview). Binary: `ast-grep --help` and each command's `--help`, captured in `eval/api_inventory.json`. CLI flags are listed as printed by the installed binary; no MCP or HTTP API is advertised by the CLI docs. `lsp` uses stdio LSP. Rows for utility endpoints without a source-query result surface are listed with `N/A` pending their command tests.

| tool | endpoint | endpoint_kind | binary_inventory | scenario_runs | result_samples |
|---|---|---|---|---:|---|
| ast-grep 0.45.3 | run | query and rewrite | captured | 3 | JSON and text |
| ast-grep 0.45.3 | scan | rule query and rewrite | captured | 3 | JSON |
| ast-grep 0.45.3 | outline | source outline | captured | 3 | JSON |
| ast-grep 0.45.3 | test | rule test runner | captured | 0 | N/A |
| ast-grep 0.45.3 | new | scaffolding | captured | 0 | N/A |
| ast-grep 0.45.3 | lsp | stdio LSP | captured | 0 | N/A |
| ast-grep 0.45.3 | completions | shell completion | captured | 0 | N/A |
| Semgrep OSS 1.178.0 | scan | query and autofix | partial | 20 | JSON |
| Semgrep OSS 1.178.0 | mcp | MCP | names pending tools/list | 0 | N/A |
| Grit 0.1.1 | apply | query and rewrite | partial | 0 | N/A |
| Comby 1.8.1 | match | structural text query | partial | 0 | N/A |
| Comby 1.8.1 | rewrite | structural text rewrite | partial | 0 | N/A |
| Tree-sitter CLI 0.26.9 | query | tree-sitter query | captured | 5 | stdout captures |
| CodeQL CLI 2.26.4 | database and query commands | database query | 89 command help entries | 0 | N/A |
| ryi | query | tree-sitter query | captured | 0 | N/A |
| ryii | query | tree-sitter query and SQLite | captured | 20 plus reference passes | JSONL |

The binary flag inventory is in `eval/api_inventory.json`; it does not enumerate every docs-only API route. This inventory lists the relevant query endpoints. `new`, LSP, and completion endpoints are not query endpoints. Comby HTTP server routes require a separate source build; attempting `-server` returned `unknown flag -server`. The API matrix remains incomplete for three-scenario endpoint calls, samples, and matching `ryii` calls.
### Other tools

Endpoint inventory is not a completed scenario matrix. Grit, Comby, CodeQL, Semgrep MCP, Tree-sitter query endpoints, and `ryii` require real-call rows before this API matrix is complete. Scaffolding, completion, and LSP endpoints are not query endpoints.

| Tool | Installed/API status |
|---|---|
| Semgrep OSS | `1.178.0` installed under `eval/semgrep/venv`; binary inventory collection needs ANSI-stripped subcommand traversal. `semgrep mcp` is present; names and parameter schema pending `tools/list`. |
| Grit | CLI binary `0.1.0-alpha.1743007075` present under the npm cache. The documented helper install returned `Error: error decoding response body` with detail `invalid type: null, expected a string at line 1 column 1543`; the Rust CLI binary itself launches. Query scenarios pending. |
| Comby | `1.8.1` staged from Homebrew bottles under `eval/comby/cellar`; `pcre` and `libev` are also staged there. Wrapper `eval/comby/bin/comby` uses only those local libraries. `-server` returned `unknown flag -server`; docs say the Comby HTTP server requires a separate source build. The documented routes are `/match`, `/rewrite`, `/substitute`. |
| Tree-sitter CLI | `0.26.9` available. Local Rust and TypeScript grammars generated and built. |
| CodeQL CLI | `2.26.4` available; `resolve languages` lists Rust and JavaScript extractors. Recursive binary help inventory collected 89 command entries. Query/database scenarios pending. |
| `ryi` / `ryii` | CLI help and `query --help` collected. Both expose the same 17 command entries. `ryii query` returned JSON rows with path, line, end line, and capture name for the cases above. |

## Structural query cases

Scope after the 2026-09-28 steer: Rust and TypeScript only. Rust query corpora are `tokio`, `hafley-rs`, `hafley_scm`, `codegraph-src`, and `graphify-src`; TypeScript corpora are `vite`, `hafley-rs`, `codegraph-src`, and `graphify-src`. Python, Go, and Kotlin work is out of scope. Case 1 is Rust-specific. Case 6 is the TypeScript decorator adaptation because Python is out of scope. Rust case data are complete only for ast-grep and Semgrep cases 1–4, plus Tree-sitter case 1. TypeScript results are limited to the decorator enumeration below. Grit, Comby, CodeQL, and rename comparisons have no completed structural rows.

`ryii` is the reference query engine. For case 1, the reference was `scratch-aff/rust_test_any.scm`, with its attribute capture renamed from `@path` to `@attr` so the result `path` field remained the source filename. Ast-grep findings were compared as a multiset of `(source path, function name)` pairs against `ryii` output, not only by raw count.

| tool | repo | case | targets | results | file_precision | file_recall | site_precision | site_recall | wall_seconds | peak_rss_mb | rc |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ast-grep 0.45.3 | tokio | 1 | 1876 | 1876 | N/A | N/A | 1 | 1 | 0.106 | 16.92 | 0 |
| ast-grep 0.45.3 | tokio | 2 | 15643 | 15643 | N/A | N/A | 0.0671 | 0.0671 | 0.2978 | 61.97 | 0 |
| ast-grep 0.45.3 | tokio | 3 | 1058 | 1058 | N/A | N/A | 1 | 1 | 0.1924 | 13.72 | 0 |
| ast-grep 0.45.3 | tokio | 4 | 6559 | 6559 | N/A | N/A | 1 | 1 | 0.1792 | 29.11 | 0 |
| ast-grep 0.45.3 | hafley-rs | 1 | 2945 | 2945 | N/A | N/A | 1 | 1 | 0.327 | 27.83 | 0 |
| ast-grep 0.45.3 | hafley-rs | 2 | 31475 | 31475 | N/A | N/A | 0.1508 | 0.1508 | 1.0338 | 44.07 | 0 |
| ast-grep 0.45.3 | hafley-rs | 3 | 1528 | 1528 | N/A | N/A | 1 | 1 | 0.3863 | 18.06 | 0 |
| ast-grep 0.45.3 | hafley-rs | 4 | 11130 | 11130 | N/A | N/A | 0.9999 | 0.9999 | 0.3929 | 26.86 | 0 |
| ast-grep 0.45.3 | hafley_scm | 1 | 58 | 58 | N/A | N/A | 1 | 1 | 0.091 | 18.27 | 0 |
| ast-grep 0.45.3 | hafley_scm | 2 | 652 | 652 | N/A | N/A | 0.112 | 0.112 | 0.0854 | 23.92 | 0 |
| ast-grep 0.45.3 | hafley_scm | 3 | 20 | 20 | N/A | N/A | 1 | 1 | 0.0834 | 19.13 | 0 |
| ast-grep 0.45.3 | hafley_scm | 4 | 1945 | 1945 | N/A | N/A | 1 | 1 | 0.0847 | 21.94 | 0 |
| ast-grep 0.45.3 | codegraph-src | 1 | 22 | 22 | N/A | N/A | 1 | 1 | 0.0355 | 23.39 | 0 |
| ast-grep 0.45.3 | codegraph-src | 2 | 53 | 53 | N/A | N/A | 0 | 0 | 0.0382 | 20.81 | 0 |
| ast-grep 0.45.3 | codegraph-src | 3 | 174 | 174 | N/A | N/A | 1 | 1 | 0.0367 | 22.71 | 0 |
| ast-grep 0.45.3 | codegraph-src | 4 | 833 | 833 | N/A | N/A | 1 | 1 | 0.0468 | 21.91 | 0 |
| ast-grep 0.45.3 | graphify-src | 1 | 0 | 0 | N/A | N/A | N/A | N/A | 0.0169 | 5.53 | 0 |
| ast-grep 0.45.3 | graphify-src | 2 | 0 | 0 | N/A | N/A | N/A | N/A | 0.0148 | 5.14 | 0 |
| ast-grep 0.45.3 | graphify-src | 3 | 0 | 0 | N/A | N/A | N/A | N/A | 0.0121 | 4.27 | 0 |
| ast-grep 0.45.3 | graphify-src | 4 | 6 | 6 | N/A | N/A | 1 | 1 | 0.0139 | 4.52 | 0 |
### Semgrep corrected Rust pass

The preliminary 40x difference included Semgrep default ignores. `--no-git-ignore` disables Git ignore discovery only; Semgrep continued applying `.semgrepignore` and built-in default ignore patterns. Corpus roots contain no `.semgrepignore` files. `--x-ignore-semgrepignore-files` bypassed those exclusions for the full file set; the binary prints that `--x-*` flags are internal. All Rust files were below the default 1,000,000-byte target limit. Retried with `--max-target-bytes 0` and `--timeout 60`; no timeout or parse errors occurred in cases 1–4. Case 1 compares `(path, function)` multisets. Case 2 compares unique `(path, line, call text)` sites inside a test-attributed function. Case 3 compares unique containing function items after Semgrep returned one match per `unwrap` call. Case 4 compares unique `(path, line, call text)` sites. File metrics use intersecting source path sets.

| tool | case | repo | targets | results | file_precision | file_recall | site_precision | site_recall | wall_seconds | peak_rss_mb | rc | errors |
|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Semgrep OSS 1.178.0 | 1 | tokio | 1876 | 1962 | 0.9338 | 0.8535 | 0.8644 | 0.9041 | 9.41 | 369.5 | 0 | 0 |
| Semgrep OSS 1.178.0 | 2 | tokio | 1501 | 1697 | 0.913 | 0.9231 | 0.8379 | 0.9467 | 15.67 | 274.9 | 0 | 0 |
| Semgrep OSS 1.178.0 | 3 | tokio | 1058 | 2261 | 0.964 | 1.0 | 0.9568 | 0.9839 | 15.71 | 251.3 | 0 | 0 |
| Semgrep OSS 1.178.0 | 4 | tokio | 34940 | 45356 | 0.9793 | 1.0 | 0.7644 | 0.9916 | 77.1 | 735.1 | 0 | 0 |
| Semgrep OSS 1.178.0 | 1 | hafley-rs | 2945 | 1788 | 0.9887 | 0.7207 | 0.9838 | 0.5973 | 15.91 | 477.2 | 0 | 0 |
| Semgrep OSS 1.178.0 | 2 | hafley-rs | 4083 | 1796 | 0.9625 | 0.6311 | 0.9833 | 0.4325 | 28.42 | 326.9 | 0 | 0 |
| Semgrep OSS 1.178.0 | 3 | hafley-rs | 1528 | 5062 | 0.9723 | 1.0 | 0.9604 | 0.9987 | 43.32 | 340.4 | 0 | 0 |
| Semgrep OSS 1.178.0 | 4 | hafley-rs | 124060 | 142844 | 0.9819 | 1.0 | 0.8615 | 0.9914 | 392.82 | 1693.0 | 0 | 0 |
| Semgrep OSS 1.178.0 | 1 | hafley_scm | 58 | 41 | 1.0 | 0.56 | 1.0 | 0.7069 | 9.51 | 278.3 | 0 | 0 |
| Semgrep OSS 1.178.0 | 2 | hafley_scm | 46 | 45 | 1.0 | 0.75 | 1.0 | 0.9783 | 6.52 | 322.3 | 0 | 0 |
| Semgrep OSS 1.178.0 | 3 | hafley_scm | 20 | 53 | 1.0 | 1.0 | 1.0 | 1.0 | 5.53 | 322.4 | 0 | 0 |
| Semgrep OSS 1.178.0 | 4 | hafley_scm | 25125 | 26048 | 0.9845 | 1.0 | 0.9547 | 0.9891 | 104.2 | 425.9 | 0 | 0 |
| Semgrep OSS 1.178.0 | 1 | codegraph-src | 22 | 0 | 0.0 | 0.0 | 0.0 | 0.0 | 5.23 | 236.0 | 0 | 0 |
| Semgrep OSS 1.178.0 | 2 | codegraph-src | 0 | 0 | N/A | N/A | N/A | N/A | 3.07 | 286.2 | 0 | 0 |
| Semgrep OSS 1.178.0 | 3 | codegraph-src | 174 | 234 | 1.0 | 1.0 | 0.9158 | 1.0 | 3.49 | 305.1 | 0 | 0 |
| Semgrep OSS 1.178.0 | 4 | codegraph-src | 12081 | 12832 | 1.0 | 1.0 | 0.9181 | 0.9738 | 44.93 | 338.2 | 0 | 0 |
| Semgrep OSS 1.178.0 | 1 | graphify-src | 0 | 0 | N/A | N/A | N/A | N/A | 2.23 | 114.2 | 0 | 0 |
| Semgrep OSS 1.178.0 | 2 | graphify-src | 0 | 0 | N/A | N/A | N/A | N/A | 1.52 | 112.0 | 0 | 0 |
| Semgrep OSS 1.178.0 | 3 | graphify-src | 0 | 0 | N/A | N/A | N/A | N/A | 1.32 | 112.7 | 0 | 0 |
| Semgrep OSS 1.178.0 | 4 | graphify-src | 10 | 11 | 1.0 | 1.0 | 0.9091 | 1.0 | 2.21 | 114.8 | 0 | 0 |

Case 1 used `case1-final.yml`: eight patterns for single/qualified test attribute paths, with and without attribute arguments, and the four Rust function forms (`fn`, `async fn`, `pub fn`, `pub async fn`). `metavariable-regex` was anchored to the attribute path so `#[cfg(test)]` did not pass. Case 2 used `case2-final.yml` with `pattern: $OBJ.unwrap(...)` under each attribute/function form and the same anchored path regex. Case 3 used `case3.yml`; Semgrep returned one match per `unwrap` call and the comparison collapsed calls to containing function items. Case 4 used `case4.yml`; Semgrep returned each call expression as a separate JSON finding. It did not return one grouped match with a function-name capture and a list of every call.

The original `eval/bench/run.py` sampler could not start because its environment lacks `psutil` (`ModuleNotFoundError: No module named 'psutil'`). Measurements above use macOS `/usr/bin/time -l`, with command stdout saved as Semgrep JSON. Semgrep telemetry settings were `DO_NOT_TRACK=1`, `SEMGREP_SEND_METRICS=off`, and `OTEL_SDK_DISABLED=true`.

Tree-sitter Rust and TypeScript grammars were generated and built under `eval/tree-sitter/`. TypeScript generation first failed with `Cannot find module 'tree-sitter-javascript/grammar'`; local `tree-sitter-javascript` v0.23.1 was exposed through `NODE_PATH`, after which generation and build succeeded. Tree-sitter case 1 was run on the Rust corpora above. The TypeScript six-case pass and Tree-sitter Rust cases 2–5 remain unmeasured. No grammar files were added to the repository.


### Tree-sitter Rust case 1

| tool | repo | case | targets | results | file_precision | file_recall | site_precision | site_recall | wall_seconds | peak_rss_mb | rc |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| tree-sitter CLI 0.26.9 | tokio | 1 | 1876 | 1876 | N/A | N/A | N/A | N/A | 2.3172 | N/A | 0 |
| tree-sitter CLI 0.26.9 | hafley-rs | 1 | 2945 | 2945 | N/A | N/A | N/A | N/A | 5.8662 | N/A | 0 |
| tree-sitter CLI 0.26.9 | hafley_scm | 1 | 58 | 58 | N/A | N/A | N/A | N/A | 1.6578 | N/A | 0 |
| tree-sitter CLI 0.26.9 | codegraph-src | 1 | 22 | 22 | N/A | N/A | N/A | N/A | 0.832 | N/A | 0 |
| tree-sitter CLI 0.26.9 | graphify-src | 1 | 0 | 0 | N/A | N/A | N/A | N/A | 0.7231 | N/A | 0 |

Counts match the case 1 `ryii` reference. File metrics and peak RSS were not captured. `ryii` returned rc 2 on hafley-rs when its unfiltered Rust corpus included invalid UTF-8; the filtered case 1 comparison used `*.rs` paths.
### TypeScript decorator probe

The `ryii` query was `(decorator) @decorator` with `--pattern '*.ts' --pattern '*.tsx' --lang typescript`; each sample was checked from captured source text. Per-command wall time and peak RSS were not captured.

| tool | repo | case | targets | results | file_precision | file_recall | site_precision | site_recall | wall_seconds | peak_rss_mb | rc |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ryii | vite | 6 | 0 | 4 | N/A | N/A | 0 | N/A | N/A | N/A | 0 |
| ryii | hafley-rs | 6 | 0 | 2 | N/A | N/A | 0 | N/A | N/A | N/A | 2 |
| ryii | codegraph-src | 6 | 0 | 3 | N/A | N/A | 0 | N/A | N/A | N/A | 0 |
| ryii | graphify-src | 6 | 0 | 2 | N/A | N/A | 0 | N/A | N/A | N/A | 0 |

All 11 decorator captures were manually inspected; none has an identifier matching `(?i)test|fixture`. The hafley-rs query exited 2 after encountering invalid UTF-8. Per-command wall time and peak RSS were not captured.

## Rename and reference evaluation

Pending for the narrowed Rust and TypeScript scope. SCIP target sampling, per-target references, edit-site precision and recall, build or type checks, and diff-boundary checks were not run. Rename support was not measured.
