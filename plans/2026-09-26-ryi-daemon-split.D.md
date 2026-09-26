# ryi daemon split, code-only receipt

Branches: `feature/the-gang-splits-the-daemon` in both worktrees. Hafley TSP commits: `a8ddbed`, `fe1971a`, `6ae2a73`. Hafley RS code commits: `bbe725bc`, `acf887c2`, `25b0c277`. The final hafley-rs commit updates this report.

## State

Code was edited without `cargo`, `pnpm`, `tsp compile`, `node`, `just`, ryi, or CodeQL runs. `git diff --check` passed before the report commit. No compile, test, emitted-file, or runtime expectation below has been verified. The lane is incomplete at the test migration and connect-or-spawn library boundary described below.

## Files

### hafley-tsp

- `packages/decorator-def/lib/daemon.tsp`, `lib/main.tsp`, `src/daemon.ts`, `src/index.ts`, `src/lib.ts`, `src/tsp-index.ts`, `package.json`, `pnpm-lock.yaml`: Daemon namespace, state, validation-time routes, package dependency.
- `packages/rust/src/adapters/02_http-ops.ts`, `src/components/4_codegen/7_OpsTransports.tsx`, `src/emitter/00_types.ts`, `02_emit-model.tsx`, `03_emit-crate.tsx`, `07_daemon-files.tsx`: daemon metadata, spread grouping, shared serializable args, client/server emission.
- `packages/rust/src/emitter/templates/{client_auto,daemon_auto,server_auto}.rs`: Rust transport templates.
- `packages/rust/test/fixtures/daemon_cli/ops.tsp`, `src/emitter/07_daemon-files.test.tsx`: hand-written TypeSpec fixture and unverified snapshot.

### hafley-rs

- `Cargo.lock`, `crates/ryi/{Cargo.toml,build.rs,src/main.rs,tests/0_help.rs}`: thin crate, shared generated modules, hyper HTTP/1 Unix client, fresh server exec mode.
- `crates/sprefa-extract/{Cargo.toml,Cargo.lock}`, `src/bin/ryi.rs`, `src/bin/ryi/{1_inputs.rs,ops.rs}`: renamed server binary, daemon and one-shot entry, request-root context, raw row forwarding.
- `crates/sprefa-extract/schema/cli/{ops.tsp,0_gen.py}`: reduced contract and generated-file roster.
- `crates/sprefa-extract/src/bin/ryi/gen/{cli_auto.rs,ops_auto.rs,client_auto.rs,daemon_auto.rs,server_auto.rs,models/inputs.rs,models/file_args.rs}`: hand-written generated files. Both crates include this one model and argument module set. `gen/http_auto.rs` and `src/bin/ryi/2_serve.rs` were removed.
- `crates/sprefa-extract/tests/181_server_modes.rs`: new unverified mode check. The existing integration test files listed below changed `CARGO_BIN_EXE_ryi` to `CARGO_BIN_EXE_ryi-server`; their invocation bodies still use the prior direct CLI shape.

<details><summary>Existing integration test source files with binary-reference changes</summary>

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
- `crates/sprefa-extract/tests/178_ryi_help.rs`
- `crates/sprefa-extract/tests/179_codeql_baseline.rs`
- `crates/sprefa-extract/tests/180_call_ladder.rs`
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

</details>

## TypeSpec decorators

Counts in `schema/cli/ops.tsp`, read from the base and edited files:

| Syntax | Before | After |
| --- | ---: | ---: |
| `@daemon` | 0 | 1 |
| `@route` | 14 | 0 |
| `@post` | 14 | 0 |
| `@query` | 91 | 0 |
| `@path` | 3 | 0 |
| `@bodyRoot` | 6 | 1 |
| `@valueName` | 37 | 34 |
| `@requires` | 8 | 8 |
| `@requiresAll` | 1 | 1 |
| `@conflictsWith` | 6 | 6 |
| `@positional` | 7 | 7 |
| `@requiredOneOf` | 1 | 1 |
| `...Inputs` | 0 | 5 |

`@bodyRoot` remains only on the `JsonlStream<jsonValue>` ingest input. The decorator-def import precedes `@typespec/http` because its `$onValidate` assigns per-operation routes before HTTP duplicate-route validation.

## Crate and API choices

| Role | Crate / version | Calls in code |
| --- | --- | --- |
| Detach | [`daemonize` 0.5.0](https://docs.rs/daemonize/0.5.0/daemonize/) | `Daemonize::new().start()` before creating the Tokio runtime |
| Single instance | [`fs4` 0.12.0](https://docs.rs/fs4/0.12.0/fs4/trait.FileExt.html) | `FileExt::try_lock_exclusive(&lock)`; held through serve lifetime |
| Unix HTTP client | [`hyper` HTTP/1](https://docs.rs/hyper/1/hyper/client/conn/http1/fn.handshake.html), [`hyper-util` TokioIo](https://docs.rs/hyper-util/0.1/hyper_util/rt/struct.TokioIo.html) | `UnixStream::connect`, `TokioIo::new`, `http1::handshake`, `sender.send_request` |
| Idle/shutdown | [`tokio-util` CancellationToken](https://docs.rs/tokio-util/0.7/tokio_util/sync/struct.CancellationToken.html) | `CancellationToken::new`, `cancel`, `cancelled` in `with_graceful_shutdown` |
| Streamed stdin | `tokio-util` | `ReaderStream::new(tokio::io::stdin())` on the client; `StreamReader` and `FramedRead` on the server |
| Connect-or-spawn | no candidate integrated | The client currently uses `Command::spawn` and waits for the socket. [`muzan` 0.1.1](https://docs.rs/muzan/0.1.1/muzan/) `ensure_daemon_with_args` uses its own newline-delimited JSON IPC; [`daemonizable`](https://docs.rs/daemonizable/0.2.0/daemonizable/) `Daemonizer::spawn_daemon` uses its typed pipe RPC. Neither call has been connected to the Hyper socket path. This requirement remains open. |

The client reads the current `ryi-server --stamp` output for the build hash and datetime before the handshake. The server returns HTTP 409 and cancels itself on a mismatch when `handshake` is enabled. `idleSecs` and `handshake` in generated Rust are read from `@daemon`.

## Goldens and unverified expectations

No captured golden file changed. The existing `tests/fixtures/ryi_help/*.txt` captures still describe the direct CLI, including `serve`, and do not cover `--fresh` or `extract`; they require a hand-written before-to-after update when the help test moves to the thin client crate. No expected row count or pinned output was changed to bypass a case.

The following expectations are **unverified** because the ordered commands were not run:

- `packages/rust/src/emitter/07_daemon-files.test.tsx`: zero TypeSpec diagnostics; POST routes `/extract` and `/ingest`; generated file roster and `idleSecs: 37` snapshot.
- All seven hand-written generated Rust files listed above: output equality with `just gen-cli` and Rust compilation.
- Both hand-edited Cargo locks and the hand-edited pnpm lock: dependency resolution and locked builds.
- `crates/ryi/tests/0_help.rs` and `crates/sprefa-extract/tests/181_server_modes.rs`: thin help, server stamp, and mode diagnostics.
- Daemon startup, lock race, stale socket replacement, build mismatch, idle exit, Unix HTTP response streaming, stdin streaming, per-request root resolution, and `--fresh` exit codes.
- All existing sprefa-extract integration tests and their help/output captures. Their binary reference was renamed, but their old argument vectors are not valid `ryi-server --oneshot <verb> <request json>` calls. These tests are expected to fail until migrated to invoke the thin `ryi` binary and compare the new response contract.

## Open work

1. Integrate a listed connect-or-spawn library without replacing the required Hyper Unix HTTP transport. The current connect/spawn loop is manual.
2. Migrate the existing direct CLI integration suite to the new thin client and update help captures with stated causes. No golden has been changed yet.
3. Express the inherited `-` path-list stdin mode and the region generated-body stdin mode as `@bodyRoot JsonlStream<T>` request streams, or remove those modes from the public contract. The current daemon does not receive those stdin bytes for non-ingest verbs.
4. Generalize `server_auto.rs` emission beyond one stream-typed request operation. The emitter currently rejects a daemon service with zero or multiple stream inputs.
5. Verify all generated outputs and builds when code execution is allowed.
