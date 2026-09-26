# ryi daemon split, review D

Branches: `feature/the-gang-splits-the-daemon` in both worktrees. Bases: hafley-rs `9c66d7e1`, hafley-tsp `7b8f4cc`. The last hafley-rs commit contains this report.

## State

`ryi` is a client crate with no engine dependency. `ryi-server` keeps the old direct clap path for one-shot commands and exposes the generated HTTP daemon. The two binaries use generated types from `ryi-proto`. The requested design is **not complete**: connect-or-spawn still uses a direct `Command::spawn` retry loop, stdin sentinels outside `ingest` are not streamed, and successful daemon stderr diagnostics do not reach the client. These are detailed below.

## Files

The complete changed-file roster for each repository is at the end of this report. The main groups are:

- hafley-tsp `packages/decorator-def/{lib,src}`: one `Daemon` namespace, validation-time routes, package exports and dependency. `packages/rust/src/{adapters,components,emitter}`: HTTP operation and stream metadata, daemon-specific serialization, clap/HTTP emission, client/server templates and snapshot. `packages/rust/test/fixtures/daemon_cli/ops.tsp`: non-ryi fixture. `pnpm-lock.yaml`: dependency resolution.
- hafley-rs `crates/ryi`: thin binary, generated client, help test. `crates/ryi-proto`: one shared generated model and operation module. `crates/sprefa-extract/schema/cli`: TypeSpec contract and three-output generation. `crates/sprefa-extract/src/bin/ryi*`: direct server CLI, daemon handlers, typed engine adapter and request context. `crates/hafley_scm/src/read/scip_ensure.rs` and `crates/sprefa-extract/src/{5_diff.rs,edit/*,bin/ryi/0_revision.rs}`: request-local budget and paths. `crates/sprefa-extract/tests`: binary rename references, HTTP parity and help/mode rails. Both Cargo locks and the crate justfile changed.

## TypeSpec decorators

Counts in `crates/sprefa-extract/schema/cli/ops.tsp`, comparing the base with this branch:

| Syntax | Before | After |
| --- | ---: | ---: |
| `@daemon` | 0 | 1 |
| `@route` | 14 | 0 |
| `@post` | 14 | 0 |
| `@query` | 91 | 2 |
| `@path` | 3 | 0 |
| `@bodyRoot` | 6 | 1 |
| `@valueName` | 37 | 34 |
| `@requires` | 8 | 8 |
| `@requiresAll` | 1 | 1 |
| `@conflictsWith` | 6 | 6 |
| `@positional` | 7 | 10 |
| `@requiredOneOf` | 1 | 1 |
| `...Inputs` | 0 | 5 |

The two remaining `@query` annotations are `ingest.paths` and `ingest.sqlite`. TypeSpec reports `duplicate-body` when those join its `@bodyRoot input: JsonlStream<jsonValue>` in the body. The decorator-def import precedes `@typespec/http` so its `$onValidate` routes exist before HTTP duplicate-route validation. `setRoute({ program }, op, { path: "/" + op.name, shared: false })` is called only for undecorated operations in the daemon namespace.

## Generated files

These files began as hand-written expected output during the code-only phase. All fourteen were subsequently produced by `0_gen.py` and compared byte for byte with a separate fresh staging emission. `diff -rq` reported no differences.

- `crates/ryi-proto/src/gen/daemon_auto.rs`
- `crates/ryi-proto/src/gen/ops_auto.rs`
- `crates/ryi-proto/src/gen/models/{call_edge,edit_plan,fact_summary,file_args,inputs,mod,type_edge,type_edge_kind}.rs`
- `crates/ryi/src/gen/{cli_auto,client_auto}.rs`
- `crates/sprefa-extract/src/bin/ryi/gen/{cli_auto,server_auto}.rs`

`ryi-proto/src/lib.rs` includes these files inside its own crate. Client and server depend on `ryi-proto`; neither uses a cross-crate `#[path]`.

## Daemon mechanism and API calls

| Role | Crate | Exact call |
| --- | --- | --- |
| Detach | [`daemonize` 0.5](https://docs.rs/daemonize/0.5.0/daemonize/) | `Daemonize::new().start()` |
| Single instance | [`fs4` 0.12](https://docs.rs/fs4/0.12.0/fs4/trait.FileExt.html) | `fs4::fs_std::FileExt::try_lock_exclusive(&lock)` |
| Unix HTTP | [`hyper` 1](https://docs.rs/hyper/1/hyper/client/conn/http1/fn.handshake.html), [`hyper-util` 0.1](https://docs.rs/hyper-util/0.1/hyper_util/rt/struct.TokioIo.html) | `UnixStream::connect`, `TokioIo::new`, `http1::handshake`, `sender.send_request` |
| Idle shutdown | [`tokio-util` 0.7](https://docs.rs/tokio-util/0.7/tokio_util/sync/struct.CancellationToken.html) | `CancellationToken::new`, `cancel`, `cancelled`, `axum::serve(...).with_graceful_shutdown(...)` |
| Stream input | `tokio-util` 0.7 | `ReaderStream::new` on the client; `StreamReader::new` and `FramedRead::new` on the server |
| Connect-or-spawn | no selected crate | Current code calls `Command::new(server).arg("--daemon").spawn()` and retries `UnixStream::connect`. This does **not** satisfy the library requirement. |

[`muzan::ensure_daemon_with_args`](https://docs.rs/muzan/0.1.1/muzan/) carries its own line-oriented JSON IPC, and [`daemonizable::Daemonizer::spawn_daemon`](https://docs.rs/daemonizable/0.2.0/daemonizable/struct.Daemonizer.html) uses typed pipe RPC. Neither was connected to the required Hyper socket protocol. A library-compatible connect-or-spawn mechanism remains open.

The socket and lock live under `XDG_CACHE_HOME/ryi`, or `HOME/.cache/ryi`. The server stores its own executable size and nanosecond mtime at startup; the client computes the same identity from the resolved `ryi-server` path on connect. A mismatch returns 409, cancels the old server and triggers respawn. `@daemon` supplies idle 600 seconds and handshake enabled; `RYI_IDLE_SECS` and `RYI_HANDSHAKE` override them at runtime. A response-body guard keeps a streamed request active until EOF or drop.

`with_request_root` places an absolute root in a thread-local slot for a handler invocation. The row producer captures and reapplies it on its worker thread. Path resolution occurs once in `Request::decode` against that root. Revision reads use explicit scratch paths; diff facts are rebased from scratch before comparison. The request path never changes process cwd. SCIP timeout is another scoped thread-local override. This does not move the process-wide mimalloc cap into request scope.

## Changed goldens

The help captures for `cleave`, `diff`, `fast`, `graph`, `ingest`, `move`, `query`, `region`, `rename`, `schema`, `scip`, `slow`, `trail`, and `watch` each changed `Usage: ryi ...` to `Usage: ryi-server ...` and gained documented `--fresh`. The server treats fresh as a no-op because the direct clap path is already one shot. `trail` also gained `[OPTIONS]` because the generated global option is available there. The `root.txt` capture changed `ryi` to `ryi-server`, removed the `serve` command and gained the same fresh option. The transient `extract.txt` capture was removed because `@rootArgs(FileArgs)` owns the implicit root operation, so there is no extract clap subcommand. No `serve_help` golden exists; the serve help assertion in `178_ryi_help.rs` was removed because serve is now `--daemon`.

`ryi_http_parity.tsv` changed every row because the test now compares direct one-shot output with the generated daemon HTTP route using raw JSONL and request envelopes. Its old format path used `--format jsonl` and HTTP wrappers. The before -> after row counts, shared by CLI and router, are:

| Verb | Before -> after |
| --- | --- |
| fast | 313 -> 1446 |
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

The socket fast row also changed 313 -> 1446 for the same raw-output cause. The trail case uses an isolated HOME to keep historical user runs out of its fixture. Hashes changed with those bodies and path normalization. No expected count was changed to bypass an unrelated failing test.

## Verification

The user lifted the code-only verification ban for this audit. All commands below used the two lane worktrees. Cargo used `CARGO_TARGET_DIR=/tmp/ryi-daemon-audit1-root-target` or `/tmp/ryi-daemon-audit1-server-target`.

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

A manual round trip copied sibling binaries into one temporary directory. `ryi fast` and `ryi --fresh fast` produced identical stdout for an absolute fixture path (200 lines). Touching the copied server binary forced a 409 handshake replacement and preserved stdout. With `RYI_IDLE_SECS=1`, the socket disappeared after three seconds. These checks were manual; there is no committed client-plus-server process test.

After the final client template change, another absolute-file fast run returned code 0 in both modes and `cmp` found identical 94-line stdout. An invalid rename returned code 2 in both modes, with zero stdout bytes; the daemon client wrote its JSON error row to stderr. The fresh process wrote its original plain diagnostic to stderr.

`178_ryi_help::generated_format_accepts_root_and_global_positions` still fails: it expects 31 fast rows and receives 200. The same failure was reproduced at base `9c66d7e1` in a separate temporary worktree. Its pin was left unchanged.

## Unverified expectations and open work

- The full sprefa-extract integration suite, daemon stderr/tracing parity, per-user lock race under simultaneous client starts, all verb-specific relative path cases, and memory behavior under concurrent requests remain unverified.
- Successful daemon operations lose direct `eprintln!` diagnostics because `daemonize` detaches stderr. The client forwards error bodies to stderr and keeps the final stream error row off stdout, but there is no per-request stderr trailer or side channel.
- `fast -` and `region --generated -` still read the daemon's stdin. Only `ingest` has a TypeSpec `@bodyRoot JsonlStream<jsonValue>` request stream. Ingest avoids reading an interactive TTY. Modeling these other stdin uses as streams conflicts with body-default parameters unless their metadata placement changes.
- The HTTP `{request_root,args}` envelope and `x-ryi-request` stream metadata header are generated transport conventions and are not explicit TypeSpec models.
- Relative path spellings differ in response facts between the daemon and fresh process: daemon request paths become absolute against `request_root`, while fresh keeps the old relative spelling. An absolute fixture path was used for stdout parity.
- The process-wide mimalloc limit remains shared by all daemon requests.
- The connect-or-spawn library requirement remains unmet, as described above.

## Complete changed-file roster

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

<details><summary>hafley-rs: 222 paths</summary>

- `Cargo.lock`
- `crates/hafley_scm/src/read/scip_ensure.rs`
- `crates/ryi-proto/Cargo.toml`
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
- `crates/ryi/build.rs`
- `crates/ryi/src/gen/cli_auto.rs`
- `crates/ryi/src/gen/client_auto.rs`
- `crates/ryi/src/main.rs`
- `crates/ryi/tests/0_help.rs`
- `crates/sprefa-extract/Cargo.lock`
- `crates/sprefa-extract/Cargo.toml`
- `crates/sprefa-extract/justfile`
- `crates/sprefa-extract/schema/cli/0_gen.py`
- `crates/sprefa-extract/schema/cli/domain.tsp`
- `crates/sprefa-extract/schema/cli/ops.tsp`
- `crates/sprefa-extract/src/0_query.rs`
- `crates/sprefa-extract/src/5_diff.rs`
- `crates/sprefa-extract/src/bin/ryi.rs`
- `crates/sprefa-extract/src/bin/ryi/0_revision.rs`
- `crates/sprefa-extract/src/bin/ryi/1_inputs.rs`
- `crates/sprefa-extract/src/bin/ryi/2_serve.rs`
- `crates/sprefa-extract/src/bin/ryi/gen/cli_auto.rs`
- `crates/sprefa-extract/src/bin/ryi/gen/http_auto.rs`
- `crates/sprefa-extract/src/bin/ryi/gen/server_auto.rs`
- `crates/sprefa-extract/src/bin/ryi/ops.rs`
- `crates/sprefa-extract/src/edit/_6_move.rs`
- `crates/sprefa-extract/src/edit/_6_rename.rs`
- `crates/sprefa-extract/src/edit/_7_cleave.rs`
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
- `crates/sprefa-extract/tests/1_move.rs`
- `crates/sprefa-extract/tests/1_resolve_cli.rs`
- `crates/sprefa-extract/tests/1a_prolog_refs.rs`
- `crates/sprefa-extract/tests/1b_prolog_metacall.rs`
- `crates/sprefa-extract/tests/23_flow_cli_dispatch.rs`
- `crates/sprefa-extract/tests/25_query_digest_repo_from_path.rs`
- `crates/sprefa-extract/tests/26_parallel_dispatch.rs`
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
