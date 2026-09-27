# ryi daemon split, review D

Branches: `feature/the-gang-splits-the-daemon` in both worktrees. Bases: hafley-rs `9c66d7e1`, hafley-tsp `7b8f4cc`. The last hafley-rs commit contains this report.

## State

`ryi` is always the generated daemon client. It connects to the per-user socket or spawns `ryii --daemon`; there is no exec fallback or `--daemon-client` flag. `ryii <verb>` runs the flat in-process CLI, and `ryii --daemon` serves requests. The two binaries share generated types and one clap model from `ryi-proto`; the server sets its displayed clap name to `ryii`. Stdin selected by `-` or `/dev/stdin` travels as an unmodified `application/octet-stream` request body. The requested design is **not complete**: connect-or-spawn still uses a direct `Command::spawn` retry loop, and some engine diagnostics remain outside the request-scoped stderr transport. These are detailed below.

## Files

The complete changed-file roster for each repository is at the end of this report. The main groups are:

- hafley-tsp `packages/decorator-def/{lib,src}`: one `Daemon` namespace, validation-time routes, package exports and dependency. `packages/rust/src/{adapters,components,emitter}`: HTTP operation and stream metadata, daemon-specific serialization, clap/HTTP emission, client/server templates and snapshot. `packages/rust/test/fixtures/daemon_cli/ops.tsp`: non-ryi fixture. `pnpm-lock.yaml`: dependency resolution.
- hafley-rs `crates/ryi`: generated always-daemon client and help test. `crates/ryi-proto`: one shared generated model, operation and clap module. `crates/sprefa-extract/schema/cli`: TypeSpec contract with `serverBin: "ryii"` and three-output generation. `crates/sprefa-extract/src/bin/ryi*`: direct `ryii` CLI, daemon handlers, typed engine adapter and request context. `crates/hafley-observe`: a non-closing Chrome trace flush call for daemon request completion. `crates/hafley_scm/src/read`: path-aware extraction cache, request-scoped filesystem root, module manifest reads and SCIP budget. `crates/sprefa-extract/src/{5_diff.rs,edit/*,bin/ryi/0_revision.rs}`: request-local paths. `crates/sprefa-extract/tests`: binary rename references, HTTP parity, PID-backed daemon cleanup, round trip and help/mode rails. Both Cargo locks and the crate justfile changed.

## TypeSpec decorators

Counts in `crates/sprefa-extract/schema/cli/ops.tsp`, comparing the base with this branch:

| Syntax | Before | After |
| --- | ---: | ---: |
| `@daemon` | 0 | 1 |
| `@route` | 14 | 0 |
| `@post` | 14 | 0 |
| `@query` | 91 | 2 |
| `@path` | 3 | 0 |
| `@bodyRoot` | 6 | 8 |
| `@valueName` | 37 | 34 |
| `@requires` | 8 | 8 |
| `@requiresAll` | 1 | 1 |
| `@conflictsWith` | 6 | 6 |
| `@positional` | 7 | 10 |
| `@requiredOneOf` | 1 | 1 |
| `...Inputs` | 0 | 5 |

The two remaining `@query` annotations are `ingest.paths` and `ingest.sqlite`. `RawByteStream` is `HttpStream<bytes, "application/octet-stream", bytes>`; it is the `@bodyRoot` request input on extract, fast, slow, scip, graph, query, region and ingest. The daemon validator marks those operations' other implicit-body arguments with HTTP `@bodyIgnore`, since the generated transport carries their serialized arguments in `x-ryi-request`. This prevents HTTP `duplicate-body` diagnostics without adding query annotations to every flag. The decorator-def import precedes `@typespec/http` so its `$onValidate` routes exist before HTTP duplicate-route validation. `setRoute({ program }, op, { path: "/" + op.name, shared: false })` is called only for undecorated operations in the daemon namespace.

The optional `serverBin` field stays inside the existing `@daemon` decorator. `Ryi` sets it to `ryii`; the emitter's generic default remains `${bin}-server`. `daemon_auto::SERVER_BIN` supplies the client's sibling/PATH lookup and the server's clap display name.

## Generated files

These files began as hand-written expected output during the code-only phase. The shared clap module now lives in `ryi-proto`; `0_gen.py` emits each generated file once. A separate fresh staging emission is compared byte for byte with the committed output.

- `crates/ryi-proto/src/gen/cli_auto.rs`
- `crates/ryi-proto/src/gen/daemon_auto.rs`
- `crates/ryi-proto/src/gen/ops_auto.rs`
- `crates/ryi-proto/src/gen/models/{call_edge,edit_plan,fact_summary,file_args,inputs,mod,type_edge,type_edge_kind}.rs`
- `crates/ryi/src/gen/client_auto.rs`
- `crates/sprefa-extract/src/bin/ryi/gen/server_auto.rs`

`ryi-proto/src/lib.rs` includes these files inside its own crate. Client and server depend on `ryi-proto`; neither uses a cross-crate `#[path]`.

The raw-stdin revision regenerated `ryi-proto/src/gen/daemon_auto.rs`, `ryi/src/gen/client_auto.rs`, and `sprefa-extract/src/bin/ryi/gen/server_auto.rs` from the TypeSpec emitter. It added no hand-written generated file.

The relative-cwd revision regenerated those same three files. None was edited by hand. The daemon fixture inline snapshot in hafley-tsp was hand-updated from the changed template and passed the later `npx vitest run` check. The request-I/O audit regenerated only `crates/sprefa-extract/src/bin/ryi/gen/server_auto.rs` from the updated hafley-tsp template; it was not hand edited.

## Daemon mechanism and API calls

| Role | Crate | Exact call |
| --- | --- | --- |
| Detach | [`daemonize` 0.5](https://docs.rs/daemonize/0.5.0/daemonize/) | `Daemonize::new().start()` |
| Single instance | [`fs4` 0.12](https://docs.rs/fs4/0.12.0/fs4/trait.FileExt.html) | `fs4::fs_std::FileExt::try_lock_exclusive(&lock)` |
| Unix HTTP | [`hyper` 1](https://docs.rs/hyper/1/hyper/client/conn/http1/fn.handshake.html), [`hyper-util` 0.1](https://docs.rs/hyper-util/0.1/hyper_util/rt/struct.TokioIo.html) | `UnixStream::connect`, `TokioIo::new`, `http1::handshake`, `sender.send_request` |
| Idle shutdown | [`tokio-util` 0.7](https://docs.rs/tokio-util/0.7/tokio_util/sync/struct.CancellationToken.html) | `CancellationToken::new`, `cancel`, `cancelled`, `axum::serve(...).with_graceful_shutdown(...)` |
| Stream input | `tokio-util` 0.7 | `ReaderStream::new` on the client; `StreamReader::new` and `FramedRead::new` on the server |
| Diagnostic transport | `http-body-util` 0.1 | `StreamBody::new` and `BodyExt::into_stream` preserve HTTP trailer frames; Hyper `Frame::trailers` carries request-scoped stderr |
| Connect-or-spawn | no selected crate | Current code calls `Command::new(ryii).arg("--daemon").spawn()` and retries `UnixStream::connect`. This does **not** satisfy the library requirement. |

For raw stdin, generated client code detects `-` or `/dev/stdin` only in path fields that are positional or default to `-`. It sends the request envelope in the base64 `x-ryi-request` header and pipes stdin through `ReaderStream` into an octet-stream HTTP body. The server copies body chunks through `StreamReader` into a `NamedTempFile`, then passes an `Arc` for that file through request-scoped context to the engine worker. The file persists through streamed response production. `Inputs.paths`, region's generated body and ingest read that scoped file; direct `ryii` keeps its own stdin reader. Ingest no longer parses and reserializes each request line in the daemon adapter. The body remains raw bytes over HTTP and on disk; each operation starts after its complete stdin body arrives. Later targeted `182_client_daemon` assertions passed for the fast path list, region generated text, and ingest raw bytes; the final e2e script also passed the relative fast path list.

[`muzan::ensure_daemon_with_args`](https://docs.rs/muzan/0.1.1/muzan/daemon/fn.ensure_daemon_with_args.html) connects to its own line-oriented JSON IPC under `$XDG_STATE_HOME/{app}/daemon.sock` and [spawns the current executable](https://docs.rs/muzan/0.1.1/src/muzan/daemon.rs.html). [`daemonizable::Daemonizer::spawn_daemon`](https://docs.rs/daemonizable/0.0.1/daemonizable/struct.Daemonizer.html) re-executes the current executable and returns a typed pipe RPC client. The lane requires the separate `ryii` binary and one Hyper socket under `$XDG_CACHE_HOME/ryi`. Neither library's connect-or-spawn API directly takes that executable and socket. A choice to extend the candidate list or permit a second IPC socket remains open; the current `Command::spawn` loop does not satisfy the named-library requirement.

The lock and PID file live under `XDG_CACHE_HOME/ryi`, or `HOME/.cache/ryi`. The socket normally lives there too. When that path exceeds 100 bytes, a SHA-256-derived short socket directory under the system temporary directory is used with mode 0700. The server stores its own executable size and nanosecond mtime at startup; the client computes the same identity from the resolved `ryii` path on connect. A mismatch returns 409, cancels the old server and triggers respawn. Tests own each daemon with a guard that sends `POST /__shutdown`, then waits on its recorded PID and escalates through SIGTERM and SIGKILL. `@daemon` supplies idle 600 seconds and handshake enabled; the generated service-name prefix yields `RYI_IDLE_SECS` and `RYI_HANDSHAKE` overrides. A response-body guard keeps a streamed request active until EOF or drop.

The daemon installs `sprefa_extract::trace::install()` once after detaching. The generated middleware creates one `daemon_request` span per HTTP request, with `request_id`, operation verb and recorded `request_root`. `RequestGuard::drop` calls `hafley_observe::flush_trace()`, which calls `tracing_chrome::FlushGuard::flush()` without closing the timeline; daemon exit calls `finish_trace()`. The process test sends two `fast` requests through one daemon and finds two distinct request IDs with the expected verb and root in the Chrome trace. Finite daemon operations send 64 KiB chunks across the bounded channel; watch retains per-event flushes so an open watch delivers its first event promptly. The generated server emits 64 KiB HTTP frames, and the client writes each frame directly into a 64 KiB buffered stdout. Direct `--format jsonl` keeps row boundaries for its completion count. Late stream errors keep the final JSON error row and carry the exit code in an HTTP trailer.

`with_request_context` places an absolute root and a diagnostic sink in thread-local slots for a handler invocation. The row producer captures and reapplies them on its worker thread. A drop guard restores prior values even on unwind. `Request::new` now serializes the path arguments without changing their spelling, while `Request::decode` validates the absolute request root. `sprefa_extract::with_io_root` scopes filesystem lookup to that root; `with_diagnostic_sink` captures the slow checker's library diagnostic in the same request buffer. Input discovery returns relative display paths, and the fast reader carries the root into its Rayon file reads and SCM fallback reads. TypeScript import resolution and Rust Cargo manifest reads use the rooted filesystem path while their fact paths retain the caller's spelling. Revision reads use explicit scratch paths; diff facts are rebased from scratch before comparison. The request path never changes process cwd. SCIP timeout is another scoped thread-local override. Extraction cache keys include the source path because TypeScript closure names embed that path even when file bytes match. This does not move the process-wide mimalloc cap into request scope.

## Changed goldens

The raw-stdin revision changed no hafley-rs golden. It changed the hafley-tsp daemon fixture from JSONL input to `RawByteStream` and updated its inline snapshot for the generated stdin path fields. The relative-cwd revision changed that snapshot again: `Request::new` and `Request::decode` no longer call `resolve_paths`, and the generated path-field table and `resolve_one` function are gone. This is a transport behavior change, not a fixture-count adjustment. The fixture now marks `extract.args` with HTTP `@bodyIgnore` so it can coexist with the raw `@bodyRoot` stream. `npx vitest run` passed, including that inline snapshot. No hafley-rs golden changed in the e2e followup. The initial HTTP parity test failure came from an ignored `.dl` cache written into the source `type_ladder` fixture by the new e2e script; moving the script's slow and query cases to a scratch copy restored the pinned `diff` hash without changing the golden.

The help captures for `cleave`, `diff`, `fast`, `graph`, `ingest`, `move`, `query`, `region`, `rename`, `schema`, `scip`, `slow`, `trail`, and `watch` changed `Usage: ryi-server ... -> Usage: ryii ...`; `root.txt` changed both usage lines the same way. Every capture lost the `--daemon-client` line because the client is now always a daemon client and the direct server has no transport flag. `schema` changed `Usage: ryi-server schema [OPTIONS] -> Usage: ryii schema`, and `trail` changed `Usage: ryi-server trail [OPTIONS] [N] -> Usage: ryii trail [N]`; after removing that flag, only help remains in their option sections, so their help-line padding returned to the pre-flag spacing. The expected captures were written by comparison with the pre-flag base and have not been run under the no-tests rule. The root capture still omits the deleted `serve` command. Its build-stamp line stays at the prior captured value because the test normalizes it. The transient `extract.txt` capture remains removed because `@rootArgs(FileArgs)` owns the implicit root operation.

`ryi_http_parity.tsv` changed every row because the test now compares direct one-shot output with the generated daemon HTTP route using raw JSONL and request envelopes. Its old format path used `--format jsonl` and HTTP wrappers. The before -> after row counts, shared by CLI and router, are:

| Verb | Before -> after |
| --- | --- |
| fast | 313 -> 312 |
| slow | 367 -> 366 |
| scip | 221 -> 220 |
| graph | 14 -> 13 |
| cleave | 1 -> 4 |
| move | 1 -> 15 |
| rename | 1 -> 115 |
| query | 4 -> 3 |
| region | 1 -> 1 |
| watch | 404 -> 403 |
| diff | 2 -> 1 |
| ingest | 1 -> 24 |
| schema | 1 -> 436 |
| trail | 1 -> 1 |

The previous 313-row fast capture included one completion row. The new 312-row raw output has the same fast record kinds; the interim 1446-row capture was a regression caused by sending phase-1 facts through the fast serializer and was removed. The socket fast row also changed 313 -> 312. Slow, scip, graph, query and watch each lose their completion row. Cleave, move, rename, ingest and schema now expose their raw lines instead of one wrapped string. Diff loses its wrapper/completion shape. The trail case uses an isolated HOME to keep historical user runs out of its fixture. Hashes changed with those bodies and path normalization.

After merging main `0d686c37`, the `diff` parity hash changed `208dc82ebd26 -> 750777a29f8d` for both CLI and router, with the row count fixed at 1. That main commit appended five lines to `tests/fixtures/type_ladder/regen.sh`: a CodeQL comment, `out=$(mktemp -d)`, a `RYI_CODEQL_OUT=$out scripts/ryi-vs-codeql.sh ...` invocation, a `cp` of `type.csv` and `call.csv`, and `rm -rf "$out"`. The executable fixture blob changed from Git object `52b7a3ca` to `0f09de92`. `178_cli_http_parity.rs:25-47,154-157` copies and commits every fixture file with fixed author and committer dates before `diff HEAD HEAD`; `5_diff.rs:404-408` writes that commit SHA into both `from` and `to` in the `diff_run` row. The extra script bytes therefore change the normalized row hash in both transports. This is the only parity golden edit after that merge.

On `tests/fixtures/type_ladder/src`, the corrected fast path emits 317 rows with the merge-base record-kind counts: `free_name` 90, `occurrence` 64, `symbol` 64, `resolved_type_edge` 47, `local` 33, `resolved_import` 17, and `resolved_edge` 2. The interim output additionally emitted `node` 574, `edge` 492, `sig` 22, `specifier` 17, `param` 16, and `file` 6. Those extra kinds are absent after restoring `diet_scip_jsonl` for fast stdout.

## Verification

The request-I/O audit searched `std::fs::`, `File::open`, `Command::new`, `WalkBuilder`, `WalkDir`, and `current_dir(` under `crates/sprefa-extract/src` and `crates/hafley_scm/src/read`; `rg` returned 158 lines. The verb-reachable relative probes now pass through `io_path` or an absolute root captured before worker dispatch: direct file and ingest reads; SCIP source-set and staging walks, index files, marker/cache paths and subprocess cwd; checker inputs and manifest/dependency reads; graph revision, diff output, watch root/receipts, region inputs, edit lists/stages and SQLite output. Temporary staging paths and the daemon's cache/socket files were already absolute. The `git rev-parse` trail command uses `ops::request_root()`, the indexer uses its absolute staged cwd, and edit verification uses its rooted cwd. Fact paths keep the caller's spelling. No golden or fixture count was changed in this audit.

The later TypeSpec `duplicate-body` report came from stale compiled decorator output in the primary hafley-tsp checkout: `packages/decorator-def/src/daemon.ts:38-46` marks non-stream parameters with HTTP `@bodyIgnore`, but its old `dist/src/daemon.js` lacked that code. The same checkout also lacked `packages/rust/dist/emitter/06_on-emit.js` when first probed. `schema/cli/0_gen.py` now runs `pnpm --filter '@hafley66/alloy-rs...' build`, which builds decorator-def before the Rust emitter. With `HAFLEY_TSP` set to primary hafley-tsp main `be72d59`, the real `ops.tsp` then compiled and 12 of 13 emitted files matched the committed files. The remaining `server_auto.rs` difference is the still-unmerged hafley-tsp feature commit `2c71030`, which sends early operation errors through the CLI stderr header. With `HAFLEY_TSP` set to the hafley-tsp feature worktree after merging main, all 13 emitted files matched byte for byte. The expanded daemon fixture covers `...Inputs` beside a raw `@bodyRoot` stream; targeted Vitest passed 1/1. No generated file or golden changed in this revision. Byte equality against hafley-tsp **main** remains pending its merge of `2c71030`.

The generated server now treats an operation failure before its first data chunk as an empty error HTTP body plus the CLI's exact diagnostic bytes and exit code in headers. Later failures keep the final JSON error row and exit trailer. The client still copies response data frames without examining rows. `182_client_daemon` now checks the missing-path stderr bytes as well as stdout and exit code. The e2e script compares stderr for every parity case. That stricter script initially found the direct slow checker note and graph summary absent from the daemon client; the slow library diagnostic and graph summary now use the request-scoped diagnostic buffer. The same routing covers the watch polling notice. The changed hafley-tsp template built with TypeScript and direct Rollup, and `pnpm exec vitest run src/emitter/07_daemon-files.test.tsx` passed 1/1. TypeSpec 1.10.0 compiled the real `ops.tsp` contract through `0_gen.py`.

Final request-I/O verification used `CARGO_TARGET_DIR=$HOME/.cache/lanes/shared/target` and two build jobs: `cargo build --release --features cli --bin ryii -j 2` passed. `cargo nextest run --features cli --test all -E 'test(/^t_182_client_daemon/)' -j 2` passed **2/2**, with 1,110 tests skipped; it included the new missing-path stderr assertion. `scripts/ryi-e2e.sh $HOME/.cache/lanes/shared/target/release` exited **0** after the strict stderr comparison: **10/10** direct/client stdout, stderr and exit parity cases plus **4/4** lifecycle checks. The script shutdown guard removed its daemons. No full suite or CodeQL command ran in this audit.

For the e2e followup, `cargo build --release -p ryi -j 2` and `cargo build --release --features cli --bin ryii -j 2` completed with `CARGO_TARGET_DIR=$HOME/.cache/lanes/shared/target`. The first `scripts/ryi-e2e.sh <release dir>` run passed fast direct/daemon parity, including `fast -` with relative stdin paths, but failed `slow .`, relative `query`, and the missing-path exit code. After rooting the slow index I/O and query file read and mapping the missing input to exit 2, the script exited 0: 10 byte/exit parity rows and 4 lifecycle rows passed. Its slow case initially left an ignored `.dl` under the source fixture; that generated cache was removed, and the script now runs slow and query from a scratch copy. The parity golden was unchanged. `CARGO_TARGET_DIR=$HOME/.cache/lanes/shared/target cargo nextest run --features cli --test all -E 'test(/^t_18[12]_|^t_178_cli/)' -j 2` passed **8/8**, with 1,104 tests skipped; the selected targets include `178_cli_http_parity`, `181_server_modes`, `181_ts_ladder`, and `182_client_daemon`. The expanded 182 case passed `fast -` with `src/lib.rs` on stdin from `crates/soopy`, plus missing-path, slow and query relative-cwd comparisons. In hafley-tsp, `npx vitest run` in `packages/rust` passed **21 files, 160 tests**, with 4 skipped. No CodeQL command or full Rust suite ran.

For the relative-cwd revision, `pnpm --dir packages/rust build` completed TypeScript and then failed because the worktree lacks the `rollup` command symlink. Running its installed Rollup binary directly completed successfully. `HAFLEY_TSP=... python3 crates/sprefa-extract/schema/cli/0_gen.py` compiled the real TypeSpec contract and regenerated client, protocol and server files. The first targeted Nextest build used the prior dist bundle and failed on stale `request.decode(verb)` calls; after direct Rollup and regeneration, compilation passed. The new `fast .` assertion initially failed with 237 daemon versus 576 direct `resolved_import` rows and 1,783 versus 1,506 `resolved_edge` rows. Rooting the Rust Cargo manifest reads corrected it. A focused `t_182_client_daemon::direct_server_and_daemon_client_replacement_and_idle_exit` rerun passed 1/1. The final authorized command, `CARGO_TARGET_DIR=$HOME/.cache/lanes/shared/target cargo nextest run --features cli --test all -E 'test(/^t_182_client_daemon::|^t_178_cli_http_parity::/)' -j 4`, passed **6/6**: four HTTP parity tests and two daemon process tests, with 1,106 tests skipped. No other test target ran for this revision.

For the `ryi`/`ryii` revision, no test binary was executed, per the current user rule. The following compile-only checks passed: `pnpm exec tsc -p tsconfig.json` in decorator-def, `pnpm exec tsc -p tsconfig.build.json` and direct Rollup in packages/rust, and `HAFLEY_TSP=... python3 crates/sprefa-extract/schema/cli/0_gen.py`. Cargo used the persistent `CARGO_TARGET_DIR=$HOME/.cache/lanes/shared/target`: `cargo test --no-run -j 4 -p ryi -p ryi-proto` and `cargo test --no-run -j 4 --features cli --test all` in sprefa-extract both compiled. For the raw-stdin revision, TypeScript, Rollup, the real TypeSpec contract and the daemon fixture compile; `cargo check -j 4 -p ryi -p ryi-proto`, `cargo check -j 4 --features cli --bin ryii`, and `cargo check -j 4 --features cli --tests` passed. An initial `cargo test --no-run -j 4 --features cli --test all` found a stale formatted-ingest call; the direct formatted path now uses the same raw ingestion adapter, and the later check passed. No test binary, daemon round trip, or performance script ran for the raw-stdin revision. Its generated snapshot and expected stdout, stderr and exit-code comparisons remain unverified.

The table and run notes below record earlier revisions. They do not verify the current binary names or always-daemon client behavior. Those earlier commands used the two lane worktrees and, before the persistent-target rule, `CARGO_TARGET_DIR=/tmp/ryi-daemon-audit1-root-target` or `/tmp/ryi-daemon-audit1-server-target`.

| Command | Result |
| --- | --- |
| `pnpm exec tsc -p tsconfig.build.json` in decorator-def and rust | pass |
| direct Rollup executable with `-c rollup.config.mjs` in rust | pass; `pnpm run build` could not find its Rollup shim |
| `pnpm exec vitest run src/emitter/07_daemon-files.test.tsx` | 1 passed |
| `HAFLEY_TSP=... python3 crates/sprefa-extract/schema/cli/0_gen.py /tmp/ryi-generation-final` | TypeSpec 1.10.0 compile passed |
| `diff -rq` staged server, client, proto against checked-in `gen` | no differences |
| `cargo check -p ryi-proto -p ryi` | pass |
| `cargo check --features cli --bin ryi-server --tests` | pass, existing unused-method warnings |
| `cargo test --features cli --test 178_cli_http_parity` | 4 passed |
| `cargo test --features cli --test 55_diff_verb` | 6 passed |
| `cargo test --features cli --test 168_graph_revision` | 1 passed |
| `cargo test --features cli --test 181_server_modes` | 1 passed |
| `cargo test -p ryi --test 0_help` | 1 passed |
| `cargo test --features cli --test 178_ryi_help generated_clap_help_matches_captured_main` | passed |
| `cargo test --features cli --test 178_ryi_help generated_format_accepts_root_and_global_positions` | passed, pinned fast 31 and root 35 |
| `cargo test --features cli --test 111_cli_identity` | 4 passed |
| `cargo test --features cli --test 166_cleave_rust` | 11 passed |
| `cargo test --features cli --test 50_cli_crawl_defects` | 13 passed |
| `cargo test --features cli --test 182_client_daemon` | 1 passed: fresh equality, executable replacement, idle exit |
| `cargo test --features cli --test 27_blob_cache` | 5 passed; identical TypeScript bytes under two paths retain distinct closure names |
| `cargo test --features cli --test golden_parity -- --test-threads=1` | 11 passed after path-aware cache key |
| `178_generated_contract` target in the full suite with `HAFLEY_TSP` set | passed, fresh three-root generation matched committed files |
| `cargo test -p ryi -p ryi-proto` | 1 passed, 0 failed |
| final `cargo check -p ryi -p ryi-proto` | passed |
| final `cargo test --no-fail-fast --features cli` in `crates/sprefa-extract` | **225 targets passed; 1,081 tests passed, 0 failed, 18 ignored** |
| `cargo test -p ryi -p ryi-proto` after client path resolution | 2 passed, 0 failed |
| `cargo test --no-fail-fast --features cli` after client path resolution | **225 targets passed; 1,081 tests passed, 0 failed, 18 ignored** |
| `pnpm exec tsc -p tsconfig.build.json` and `pnpm exec vitest run src/emitter/07_daemon-files.test.tsx` after trailer emission | passed; 1 emitter test passed |
| `cargo check --features cli --bin ryi-server --tests` after trailer emission | passed |
| `cargo test --no-fail-fast --features cli` after trailer emission | **225 targets passed; 1,082 tests passed, 0 failed, 18 ignored** |
| `cargo test --features cli --bin ryi-server request_diagnostics_stay_in_the_request_buffer` after unwind guard | 1 passed |
| `cargo test --features cli --test 178_cli_http_parity --test 182_client_daemon` after unwind guard | 5 passed |
| `cargo test --features cli --test 182_client_daemon` after simultaneous-start test | 1 passed; two synchronized clients returned the same fast bytes through one cache socket |
| `pnpm exec tsc -p tsconfig.build.json` and direct Rollup after opt-in/trailer/trace templates | passed |
| `pnpm exec vitest run src/emitter/07_daemon-files.test.tsx` after short socket snapshot | 1 passed |
| fresh TypeSpec emission to `/tmp/ryi-generated-final-check` and `diff -rq` for server/client/proto roots | compile passed; all three generated roots byte-identical |
| `cargo test --features cli --test 178_ryi_help --test 178_cli_http_parity --test 182_client_daemon -- --test-threads=1` after opt-in and byte transport | 8 passed: help 2, HTTP parity 4, daemon process 2 |
| `cargo test -p ryi -p ryi-proto` after opt-in | 3 passed: client help 1, protocol 2 |
| `cargo test --features cli --bin ryi-server --test 178_ryi_help --test 178_cli_http_parity --test 182_client_daemon -- --test-threads=1` after channel chunking | 15 passed: server unit 7, HTTP parity 4, help 2, daemon process 2 |
| `cargo test --no-fail-fast --features cli -- --skip codeql --test-threads=1` after channel chunking | **225/225 targets passed; 1,083 tests passed, 0 failed, 18 ignored, 1 CodeQL test filtered** |
| `cargo build --release -p ryi` and `cargo build --release --features cli --bin ryi-server` | both passed with two build jobs |
| release `fast` on absolute `crates/hafley_scm`, direct and opt-in daemon client, five timed runs each | median direct 1.119 s real, 0.126 s sys; median warm client 1.161 s real, 0.078 s client sys |
| release direct and opt-in client output hash on that same absolute path | both 119,199 rows, 35,780,959 bytes, SHA-256 `0320cabc53dac0f244fca3a4f6c06956b341b5339cffd2f5fe0175a9467bac84`; measured daemon PID removed |

The first full `cargo test --no-fail-fast --features cli` run had 224 targets: 221 passed, 3 failed; 1076 tests passed, 4 failed, 18 ignored. Failures were `111_cli_identity` (separate crate build timestamps), `166_cleave_rust` (trail query still named `ryi`), and two `50_cli_crawl_defects` tests (generic SCIP decode error mapped to exit 2). Each cause was corrected and all three targets passed separately. The second full run had 225 targets: 224 passed, 1 failed; 1080 tests passed, 1 failed, 18 ignored. Its sole failure was the new HTTP error test, which revealed that a late query error needed the existing `or_exit_2` mapping. That mapping is restored and the four HTTP parity tests pass separately. The third full run also had 224/225 targets and 1080 passed, 1 failed, 18 ignored; its CodeQL script returned nonzero while another CodeQL job was active. The isolated CodeQL target later passed with `LC_ALL=C` (1/1). The fourth full run again had 224/225 targets and 1080 passed, 1 failed, 18 ignored; CodeQL passed, while the timing assertion in `63_go_inferred` measured 0.123s for 400 entries versus 0.042s for 200, over its 2.5x bound. That target passed 9/9 on its isolated rerun.

After the opt-in and 64 KiB transport changes, the first full suite ran with `-- --skip codeql --test-threads=2`: **225 targets, 223 passed, 2 failed; 1,080 tests passed, 2 failed, 18 ignored, 1 filtered**. The `ryi-server` unit test still expected a short row to send immediately; it now checks the flush boundary. `178_cli_http_parity` connected before the daemon had written its PID file; its start helper now waits for both socket connectivity and the PID file. No CodeQL test executed.

The next serial full-suite attempt was terminated externally with exit 143 while running `56_scip_cli_kinks`, with no reported test failure. The completed serial rerun passed 225/225 targets and left no `ryi-server` process running.

The coordinator's earlier release warm-client baseline for a relative `hafley_scm` invocation was 2.41 s real, 0.65 s user, 1.77 s sys. This lane's post-chunking five-run median on an absolute `hafley_scm` path is 1.161 s real, 0.027 s client user, 0.078 s client sys. Direct on that same absolute path measured 1.119 s real, 7.579 s accumulated child user across worker threads, and 0.126 s sys. Warm client wall time was 3.8% above direct. The coordinator's relative-path baseline and this absolute-path run differ in path spelling and fixture revision, so the timing delta is directional; the direct and client commands in this run emitted byte-identical output. Both timed and hash-check daemons were shut down and their PID files removed.

A serial full-suite run had 224/225 targets pass and 1,080 tests pass, with `golden_parity::ported_facets_match_v5` failing on five TypeScript closure names. An isolated serial run reproduced it. The extraction cache keyed only by content, language and mask, and an earlier test warmed it with the same bytes under a basename path. The extracted closure names contain the source path. Adding the path to `CacheKey` corrected the production output and made the serial parity target pass 11/11. The final full suite then passed 225/225 targets. The cache test now asserts two parses and distinct closure output for byte-identical TypeScript files at two paths; this changes its prior cross-path sharing expectation because that sharing returned the wrong path in output.

A manual round trip at the earlier default-daemon revision copied sibling binaries into one temporary directory. `ryi fast` and `ryi --fresh fast` then produced identical stdout for an absolute fixture path. Touching the copied server binary forced a 409 handshake replacement and preserved stdout. With `RYI_IDLE_SECS=1`, the socket disappeared after three seconds. The current `182_client_daemon.rs` passed its assertions for direct `ryii fast .` versus daemon `ryi fast .` byte equality from `crates/soopy`, absolute file equality, executable replacement, idle exit, zero-fact stderr equality, a synchronized two-client first start, request trace spans, raw stdin, and PID-backed shutdown including a long XDG cache path.

At an earlier revision, an invalid rename returned code 2 in both modes with zero stdout bytes, but the daemon client wrote a JSON error row to stderr while direct mode wrote plain text. The current generated server sends plain text for errors before the first output chunk; exact invalid-rename stderr has not been rerun.

`178_ryi_help::generated_format_accepts_root_and_global_positions` now passes its pinned 31 fast rows. The earlier 200-row result came from the phase-1 fast regression. Base `9c66d7e1` itself does not compile because its generated clap module refers to `ServeArgs`; the coordinator verified 2/2 help tests pass at merge-base `96292970`.

## Unverified expectations and open work

The earlier review prohibited all test execution. The later coordinator instruction authorized only `182_client_daemon` and `178_cli_http_parity`, which passed as recorded above. `tests/all.rs` declares `support/0_daemon_guard.rs`, `181_server_modes.rs`, and `182_client_daemon.rs`. `182_client_daemon.rs` compares the daemon's late-error stdout and exit code with the in-process `ryii --format jsonl` path. `ryi.rs:552-561` emits the final JSON error row through its stdout writer; `ryi.rs:352-356` writes plain CLI errors to stderr. The generated daemon client forwards response data bytes to stdout and takes the exit code from the trailer. The process assertion passed without per-line parsing in the client.

`179_codeql_baseline.rs` now uses `env!("CARGO_BIN_EXE_ryii")` because `sprefa-extract/Cargo.toml:314-317` declares `ryii` as its only binary. The prior request to drop that test edit applied before this explicit binary rename; keeping the old compile-time variable would prevent `tests/all.rs` from compiling. The consolidated test target compiled with `--no-run`. No CodeQL command or test was executed in this revision.

- Full stderr/tracing parity, lock behavior beyond the tested two-client startup, all verb-specific relative path cases, and memory behavior under concurrent requests remain unverified.
- The server captures its file disclosure and SCIP location messages in a request-local buffer. Stream responses send them in `x-ryi-stderr` HTTP trailers, and raw responses send them in the same header. The client writes those bytes to stderr; the daemon round-trip test checks the zero-fact disclosure against fresh mode. Diagnostics emitted directly from other engine modules, including the optional `RYI_SQLITE_PHASES` timing line, still go to the detached process stderr.
- The raw-stdin process assertions in `182_client_daemon.rs` compare `ryii` and `ryi` for a `fast -` path list, a `region --generated -` text body, and `ingest /dev/stdin` bytes from the TSI fixture. They passed in the targeted Nextest run. The fixture's `RawByteStream` and updated generated snapshot also passed Vitest.
- The HTTP `{request_root,args}` envelope and `x-ryi-request` stream metadata header are generated transport conventions and are not explicit TypeSpec models.
- `ryii fast .` and `ryi fast .` from `crates/soopy` emitted byte-identical stdout in the targeted process test. The request carries the absolute cwd and the literal `.` path argument. The final e2e script also verified relative `slow .`, `graph --callers`, and `query ... src` stdout, stderr, and exit code. Other verb-specific relative paths remain unverified. A separate engine discrepancy, outside this daemon lane, remains: the coordinator measured 576 `resolved_import` rows for `ryii fast .` and 653 for `ryii fast $PWD` in `crates/soopy`. Both are direct in-process invocations; this revision does not change that behavior.
- The process-wide mimalloc limit remains shared by all daemon requests.
- The connect-or-spawn library requirement remains unmet, as described above.

## Complete changed-file roster

The roster lists 20 hafley-tsp lane paths and 254 hafley-rs lane paths. The hafley-rs branch also merged main `0d686c37`, `6af0e0ad`, `f24fe957` and `a672d4a1`; inherited paths are excluded from the lane roster. One inherited fixture script changes the diff parity revision hash as described above.

<details><summary>hafley-tsp: 20 paths</summary>

- `packages/decorator-def/lib/daemon.tsp`
- `packages/decorator-def/lib/main.tsp`
- `packages/decorator-def/package.json`
- `packages/decorator-def/src/daemon.ts`
- `packages/decorator-def/src/index.ts`
- `packages/decorator-def/src/lib.ts`
- `packages/decorator-def/src/tsp-index.ts`
- `packages/rust/src/adapters/00_typespec-to-neutral.ts`
- `packages/rust/src/adapters/02_http-ops.ts`
- `packages/rust/src/components/4_codegen/7_OpsTransports.tsx`
- `packages/rust/src/emitter/00_types.ts`
- `packages/rust/src/emitter/02_emit-model.tsx`
- `packages/rust/src/emitter/03_emit-crate.tsx`
- `packages/rust/src/emitter/07_daemon-files.test.tsx`
- `packages/rust/src/emitter/07_daemon-files.tsx`
- `packages/rust/src/emitter/templates/client_auto.rs`
- `packages/rust/src/emitter/templates/daemon_auto.rs`
- `packages/rust/src/emitter/templates/server_auto.rs`
- `packages/rust/test/fixtures/daemon_cli/ops.tsp`
- `pnpm-lock.yaml`

</details>

<details><summary>hafley-rs: 254 paths</summary>

- `Cargo.lock`
- `crates/hafley-observe/src/3_chrome.rs`
- `crates/hafley-observe/src/lib.rs`
- `crates/hafley_scm/src/read/0_request_root.rs`
- `crates/hafley_scm/src/read/1_reach.rs`
- `crates/hafley_scm/src/read/2_slow.rs`
- `crates/hafley_scm/src/read/cache.rs`
- `crates/hafley_scm/src/read/deps.rs`
- `crates/hafley_scm/src/read/dispatch.rs`
- `crates/hafley_scm/src/read/lang/7_scm_rows.rs`
- `crates/hafley_scm/src/read/lang/go.rs`
- `crates/hafley_scm/src/read/lang/rust_modules.rs`
- `crates/hafley_scm/src/read/lang/ts_receivers.rs`
- `crates/hafley_scm/src/read/lang/ts_resolve.rs`
- `crates/hafley_scm/src/read/manifests.rs`
- `crates/hafley_scm/src/read/mod.rs`
- `crates/hafley_scm/src/read/project.rs`
- `crates/hafley_scm/src/read/scip.rs`
- `crates/hafley_scm/src/read/scip_decode.rs`
- `crates/hafley_scm/src/read/scip_ensure.rs`
- `crates/hafley_scm/src/read/scip_v5_rels.rs`
- `crates/ryi-proto/Cargo.toml`
- `crates/ryi-proto/build.rs`
- `crates/ryi-proto/src/gen/cli_auto.rs`
- `crates/ryi-proto/src/gen/daemon_auto.rs`
- `crates/ryi-proto/src/gen/models/call_edge.rs`
- `crates/ryi-proto/src/gen/models/edit_plan.rs`
- `crates/ryi-proto/src/gen/models/fact_summary.rs`
- `crates/ryi-proto/src/gen/models/file_args.rs`
- `crates/ryi-proto/src/gen/models/inputs.rs`
- `crates/ryi-proto/src/gen/models/mod.rs`
- `crates/ryi-proto/src/gen/models/type_edge.rs`
- `crates/ryi-proto/src/gen/models/type_edge_kind.rs`
- `crates/ryi-proto/src/gen/ops_auto.rs`
- `crates/ryi-proto/src/lib.rs`
- `crates/ryi/Cargo.toml`
- `crates/ryi/src/gen/client_auto.rs`
- `crates/ryi/src/main.rs`
- `crates/ryi/tests/0_help.rs`
- `crates/sprefa-extract/Cargo.lock`
- `crates/sprefa-extract/Cargo.toml`
- `crates/sprefa-extract/justfile`
- `crates/sprefa-extract/schema/cli/0_gen.py`
- `crates/sprefa-extract/schema/cli/domain.tsp`
- `crates/sprefa-extract/schema/cli/ops.tsp`
- `crates/sprefa-extract/scripts/ryi-e2e.sh`
- `crates/sprefa-extract/src/0_graph.rs`
- `crates/sprefa-extract/src/0_query.rs`
- `crates/sprefa-extract/src/3_region_writer.rs`
- `crates/sprefa-extract/src/4_watch.rs`
- `crates/sprefa-extract/src/5_diff.rs`
- `crates/sprefa-extract/src/bin/ryi.rs`
- `crates/sprefa-extract/src/bin/ryi/0_revision.rs`
- `crates/sprefa-extract/src/bin/ryi/0_sqlite.rs`
- `crates/sprefa-extract/src/bin/ryi/1_inputs.rs`
- `crates/sprefa-extract/src/bin/ryi/2_serve.rs`
- `crates/sprefa-extract/src/bin/ryi/gen/cli_auto.rs`
- `crates/sprefa-extract/src/bin/ryi/gen/http_auto.rs`
- `crates/sprefa-extract/src/bin/ryi/gen/server_auto.rs`
- `crates/sprefa-extract/src/bin/ryi/ops.rs`
- `crates/sprefa-extract/src/edit/_1_move_cx.rs`
- `crates/sprefa-extract/src/edit/_1_rename_cx.rs`
- `crates/sprefa-extract/src/edit/_3_stage.rs`
- `crates/sprefa-extract/src/edit/_6_move.rs`
- `crates/sprefa-extract/src/edit/_6_rename.rs`
- `crates/sprefa-extract/src/edit/_7_cleave.rs`
- `crates/sprefa-extract/src/edit/ts_rehome.rs`
- `crates/sprefa-extract/tests/0_sqlite.rs`
- `crates/sprefa-extract/tests/100_tsi_intersection.rs`
- `crates/sprefa-extract/tests/101_ts_semantic_tsi.rs`
- `crates/sprefa-extract/tests/102_rust_semantic_tsi.rs`
- `crates/sprefa-extract/tests/103_trail.rs`
- `crates/sprefa-extract/tests/104_tier_decline_diagnostic.rs`
- `crates/sprefa-extract/tests/105_resolve_syntax_tsi.rs`
- `crates/sprefa-extract/tests/106_rust_syntax_graph.rs`
- `crates/sprefa-extract/tests/107_rust_checker_features.rs`
- `crates/sprefa-extract/tests/108_rust_checker_walk_by_file.rs`
- `crates/sprefa-extract/tests/109_rust_checker_site_cost.rs`
- `crates/sprefa-extract/tests/110_tsi_name.rs`
- `crates/sprefa-extract/tests/111_cli_identity.rs`
- `crates/sprefa-extract/tests/111_ts_syntax_graph.rs`
- `crates/sprefa-extract/tests/112_go_syntax_graph.rs`
- `crates/sprefa-extract/tests/113_ts_module_edges.rs`
- `crates/sprefa-extract/tests/114_go_graph_grind.rs`
- `crates/sprefa-extract/tests/115_rust_type_grind.rs`
- `crates/sprefa-extract/tests/117_python_syntax_graph.rs`
- `crates/sprefa-extract/tests/118_python_call_grind.rs`
- `crates/sprefa-extract/tests/119_kotlin_syntax_graph.rs`
- `crates/sprefa-extract/tests/126_python_modules.rs`
- `crates/sprefa-extract/tests/127_kotlin_modules.rs`
- `crates/sprefa-extract/tests/128_scip_relationship_conforms.rs`
- `crates/sprefa-extract/tests/129_go_checker_tier.rs`
- `crates/sprefa-extract/tests/129_scip_external_coverage.rs`
- `crates/sprefa-extract/tests/130_rust_spelled_receiver.rs`
- `crates/sprefa-extract/tests/131_kotlin_module_resolve.rs`
- `crates/sprefa-extract/tests/133_go_binding_legs.rs`
- `crates/sprefa-extract/tests/134_ts_binding_legs.rs`
- `crates/sprefa-extract/tests/135_untyped_receiver_rust.rs`
- `crates/sprefa-extract/tests/136_untyped_receiver_ts.rs`
- `crates/sprefa-extract/tests/137_kotlin_receiver_legs.rs`
- `crates/sprefa-extract/tests/138_untyped_receiver_kotlin.rs`
- `crates/sprefa-extract/tests/141_unresolved_contract.rs`
- `crates/sprefa-extract/tests/142_lines_flag.rs`
- `crates/sprefa-extract/tests/143_default_call_plane.rs`
- `crates/sprefa-extract/tests/145a_query_scm_dogfood.rs`
- `crates/sprefa-extract/tests/146_rename_stop_lines.rs`
- `crates/sprefa-extract/tests/147_rename_path_union.rs`
- `crates/sprefa-extract/tests/148_rename_abstain_ts.rs`
- `crates/sprefa-extract/tests/149_graph_callers_ts.rs`
- `crates/sprefa-extract/tests/150_fast_scm_kotlin.rs`
- `crates/sprefa-extract/tests/155_cleave_ts.rs`
- `crates/sprefa-extract/tests/156_cleave_play.rs`
- `crates/sprefa-extract/tests/157_fast_scm_rows.rs`
- `crates/sprefa-extract/tests/158_fast_scm_kotlin.rs`
- `crates/sprefa-extract/tests/159_fast_scm_judge.rs`
- `crates/sprefa-extract/tests/160_fast_scm_ts.rs`
- `crates/sprefa-extract/tests/161_fast_scm_ratchet.rs`
- `crates/sprefa-extract/tests/162_graph_views_sql.rs`
- `crates/sprefa-extract/tests/163_graph_from_ts.rs`
- `crates/sprefa-extract/tests/164_graph_uses_ts.rs`
- `crates/sprefa-extract/tests/165_graph_kotlin.rs`
- `crates/sprefa-extract/tests/166_cleave_rust.rs`
- `crates/sprefa-extract/tests/166_graph_tsi_evidence.rs`
- `crates/sprefa-extract/tests/167_graph_paths.rs`
- `crates/sprefa-extract/tests/168_graph_revision.rs`
- `crates/sprefa-extract/tests/169_graph_uses_rust.rs`
- `crates/sprefa-extract/tests/170_ratchet_sites_rust.rs`
- `crates/sprefa-extract/tests/172_graph_slow.rs`
- `crates/sprefa-extract/tests/173_move_cross_crate.rs`
- `crates/sprefa-extract/tests/174_type_ladder.rs`
- `crates/sprefa-extract/tests/175_cleave_ladder.rs`
- `crates/sprefa-extract/tests/176_rename_ladder.rs`
- `crates/sprefa-extract/tests/177_crate_scope.rs`
- `crates/sprefa-extract/tests/178_cli_http_parity.rs`
- `crates/sprefa-extract/tests/178_generated_contract.rs`
- `crates/sprefa-extract/tests/178_ryi_help.rs`
- `crates/sprefa-extract/tests/179_codeql_baseline.rs`
- `crates/sprefa-extract/tests/180_call_ladder.rs`
- `crates/sprefa-extract/tests/181_server_modes.rs`
- `crates/sprefa-extract/tests/181_ts_ladder.rs`
- `crates/sprefa-extract/tests/182_client_daemon.rs`
- `crates/sprefa-extract/tests/all.rs`
- `crates/sprefa-extract/tests/support/0_daemon_guard.rs`
- `crates/sprefa-extract/tests/1_move.rs`
- `crates/sprefa-extract/tests/1_resolve_cli.rs`
- `crates/sprefa-extract/tests/1a_prolog_refs.rs`
- `crates/sprefa-extract/tests/1b_prolog_metacall.rs`
- `crates/sprefa-extract/tests/23_flow_cli_dispatch.rs`
- `crates/sprefa-extract/tests/25_query_digest_repo_from_path.rs`
- `crates/sprefa-extract/tests/26_parallel_dispatch.rs`
- `crates/sprefa-extract/tests/27_blob_cache.rs`
- `crates/sprefa-extract/tests/28_package_edges.rs`
- `crates/sprefa-extract/tests/29_data_family.rs`
- `crates/sprefa-extract/tests/2_df_aux_cli.rs`
- `crates/sprefa-extract/tests/2_move_refs.rs`
- `crates/sprefa-extract/tests/31_owned_region.rs`
- `crates/sprefa-extract/tests/31_tracing.rs`
- `crates/sprefa-extract/tests/3_move_rust.rs`
- `crates/sprefa-extract/tests/41_move_ts.rs`
- `crates/sprefa-extract/tests/42_move_list.rs`
- `crates/sprefa-extract/tests/45_emit_throughput.rs`
- `crates/sprefa-extract/tests/46_resolve_scaling.rs`
- `crates/sprefa-extract/tests/47_resolve_door_cli.rs`
- `crates/sprefa-extract/tests/48_kotlin_operator_calls.rs`
- `crates/sprefa-extract/tests/49_rust_resolve_scaling.rs`
- `crates/sprefa-extract/tests/4_capability_parity.rs`
- `crates/sprefa-extract/tests/4_move_kotlin.rs`
- `crates/sprefa-extract/tests/4_rename_ts.rs`
- `crates/sprefa-extract/tests/50_cli_crawl_defects.rs`
- `crates/sprefa-extract/tests/51_go_package_resolve.rs`
- `crates/sprefa-extract/tests/52_rust_crawl_kinks.rs`
- `crates/sprefa-extract/tests/53_ts_crawl_kinks.rs`
- `crates/sprefa-extract/tests/54_ts_module_plane.rs`
- `crates/sprefa-extract/tests/55_diff_verb.rs`
- `crates/sprefa-extract/tests/55_go_type_plane.rs`
- `crates/sprefa-extract/tests/56_scip_cli_kinks.rs`
- `crates/sprefa-extract/tests/57_rust_module_plane.rs`
- `crates/sprefa-extract/tests/59_rust_scip_macros.rs`
- `crates/sprefa-extract/tests/5_rename_rust.rs`
- `crates/sprefa-extract/tests/5_scip_facts_cli.rs`
- `crates/sprefa-extract/tests/60_rust_corpus_scope.rs`
- `crates/sprefa-extract/tests/62_go_module_plane.rs`
- `crates/sprefa-extract/tests/63_go_inferred.rs`
- `crates/sprefa-extract/tests/64_go_closure_mirror.rs`
- `crates/sprefa-extract/tests/65_ts_member_calls.rs`
- `crates/sprefa-extract/tests/66_go_iface_fanout.rs`
- `crates/sprefa-extract/tests/67_go_multihop.rs`
- `crates/sprefa-extract/tests/68_go_type_refs.rs`
- `crates/sprefa-extract/tests/68_rust_receivers.rs`
- `crates/sprefa-extract/tests/69_go_promoted.rs`
- `crates/sprefa-extract/tests/69_ts_closure_mirror.rs`
- `crates/sprefa-extract/tests/6_document_formats.rs`
- `crates/sprefa-extract/tests/6_kind_vocab.rs`
- `crates/sprefa-extract/tests/6_occurrence_text_cli.rs`
- `crates/sprefa-extract/tests/70_ts_init_receivers.rs`
- `crates/sprefa-extract/tests/71_go_residual.rs`
- `crates/sprefa-extract/tests/71_rust_paths.rs`
- `crates/sprefa-extract/tests/71_ts_namespace_members.rs`
- `crates/sprefa-extract/tests/72_go_bound_qualify.rs`
- `crates/sprefa-extract/tests/72_rust_traits.rs`
- `crates/sprefa-extract/tests/72_ts_iface_receiver.rs`
- `crates/sprefa-extract/tests/73_go_range_elem.rs`
- `crates/sprefa-extract/tests/73_ts_destructured_receiver.rs`
- `crates/sprefa-extract/tests/74_go_field_promote.rs`
- `crates/sprefa-extract/tests/74_scip_relationship_family.rs`
- `crates/sprefa-extract/tests/74_ts_property_arrow.rs`
- `crates/sprefa-extract/tests/75_rust_trait_blob.rs`
- `crates/sprefa-extract/tests/76_rust_variant_names.rs`
- `crates/sprefa-extract/tests/77_rust_collapsed_span.rs`
- `crates/sprefa-extract/tests/78_rust_checker.rs`
- `crates/sprefa-extract/tests/79_rust_generic_args.rs`
- `crates/sprefa-extract/tests/79_rust_qualified_type.rs`
- `crates/sprefa-extract/tests/79_rust_type_alias.rs`
- `crates/sprefa-extract/tests/79_rust_variant_payload.rs`
- `crates/sprefa-extract/tests/7_diet_deps_cli.rs`
- `crates/sprefa-extract/tests/7_rename_kotlin.rs`
- `crates/sprefa-extract/tests/80_py_args.rs`
- `crates/sprefa-extract/tests/80_py_assignments.rs`
- `crates/sprefa-extract/tests/80_py_decorators.rs`
- `crates/sprefa-extract/tests/80_py_direct_calls.rs`
- `crates/sprefa-extract/tests/80_py_exceptions.rs`
- `crates/sprefa-extract/tests/80_py_module_caller.rs`
- `crates/sprefa-extract/tests/80_rust_impl_owner.rs`
- `crates/sprefa-extract/tests/8_rename_prolog.rs`
- `crates/sprefa-extract/tests/8_scip_families_cli.rs`
- `crates/sprefa-extract/tests/91_origin_column.rs`
- `crates/sprefa-extract/tests/92_ts_checker.rs`
- `crates/sprefa-extract/tests/93_rust_checker_wiring.rs`
- `crates/sprefa-extract/tests/94_rust_checker_types.rs`
- `crates/sprefa-extract/tests/96_witness_wire.rs`
- `crates/sprefa-extract/tests/97_ingest.rs`
- `crates/sprefa-extract/tests/98_resolve_witness.rs`
- `crates/sprefa-extract/tests/99_syntax_tsi_rows.rs`
- `crates/sprefa-extract/tests/9_large_file_bounds.rs`
- `crates/sprefa-extract/tests/9_query_cli.rs`
- `crates/sprefa-extract/tests/9_size_skip.rs`
- `crates/sprefa-extract/tests/9a_query_blob_door.rs`
- `crates/sprefa-extract/tests/fixtures/ryi_help/cleave.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/diff.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/fast.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/graph.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/ingest.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/move.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/query.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/region.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/rename.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/root.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/schema.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/scip.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/slow.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/trail.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_help/watch.txt`
- `crates/sprefa-extract/tests/fixtures/ryi_http_parity.tsv`
- `plans/2026-09-26-ryi-daemon-split.D.md`

</details>
