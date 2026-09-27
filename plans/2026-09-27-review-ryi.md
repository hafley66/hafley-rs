# ryi/ryii review, origin/main 30d42aff, 2026-09-27

Read-only review of ryi/ryii at origin/main 30d42aff, covering crates/ryi, crates/ryi-proto, crates/sprefa-extract and crates/hafley_scm. Nothing was built or run. Where a line says "sub-audit", the item came from a delegated reader and I did not re-verify it myself. I did re-check #7 and the file read in #11.

Paths below are relative to /Users/chrishafley/projects/hafley-rs/crates/:
- `SA` = sprefa-extract/src/bin/ryi/gen/server_auto.rs
- `OPS` = sprefa-extract/src/bin/ryi/ops.rs
- `BIN` = sprefa-extract/src/bin/ryi.rs
- `CL` = ryi/src/gen/client_auto.rs
- `DA` = ryi-proto/src/gen/daemon_auto.rs

## Findings, most severe first

1. **OPS:226, BIN:1053, hafley_scm/src/read/project.rs:1854,1887 — single-file extraction runs on a 2 MiB thread in the daemon.**
   - Defect: `produce` uses `std::thread::spawn` with the default 2 MiB stack. `extract_file` then calls `dispatch()` inline on that thread. The extract pool is built with 256 MiB stacks because deep nesting overflowed 2 MiB.
   - Failure: `ryi deeply_nested.rs` through the daemon overflows the stack. The whole daemon aborts, killing every concurrent request. In-process ryii runs the same file on the 8 MiB main thread.
   - Confidence: confirmed by reading.

2. **BIN:24-37 — one 2 GiB heap cap for the whole daemon.**
   - Defect: the `cap::Cap` limit is process-global, so every request shares one 2 GiB budget.
   - Failure: two large `ryi fast` requests at once exceed the budget. The allocation failure aborts the daemon and both requests die. As separate ryii processes each would have had 2 GiB.
   - Confidence: confirmed by reading.

3. **OPS:226-258, OPS:207, OPS:266-271, SA:69-113 — a panic truncates output and still exits 0.**
   - Defect: a panic in the producer thread drops `tx`. `Rows::next` returns None, which reads as normal completion. No exit-code trailer is sent, so the client exits 0. `one()` returns `Ok(partial)` the same way. Unwinding is the panic strategy; there is no `catch_unwind`.
   - Failure: a resolver index panic on `ryi fast .` yields a partial JSONL stream and exit 0. ryii exits 101.
   - Confidence: confirmed by reading.

4. **CL:69-101 with SA:506-509, 553-558, 586 — a stamp conflict or restart leaves the client unable to start a daemon.**
   - Defect: on CONFLICT, or when an idle/old daemon is draining, the old daemon keeps `ryi.lock` while graceful shutdown waits for in-flight requests. It has already dropped its listener. The client gets ConnectionRefused and spawns a new daemon. That daemon hits `WouldBlock` on the lock and exits silently. `started = true`, so the client never spawns again and fails after 5 s with "ryii did not become ready".
   - Failure: this happens whenever another request is in flight on the old daemon. A `ryi watch` or a long `slow` pins it indefinitely.
   - Confidence: confirmed by reading.

5. **DA:23-28, DA:79-84, SA:507 — two ryii builds on one machine kill each other's daemon.**
   - Defect: there is one socket per user (`~/.cache/ryi`), but the stamp is the length and mtime of whichever ryii the client found (sibling binary first, then PATH).
   - Failure: `target/debug/ryii` and `~/.cargo/bin/ryii` used alternately each cancel the other's daemon on every call. Combined with #4, this produces "did not become ready" errors.
   - Confidence: confirmed by reading.

6. **DA:31-35 (Request carries only request_root and args) — every request uses the environment of the client that first started the daemon.**
   - Failure: per-request environment is ignored. Examples: `SPREFA_SCIP_TIMEOUT_SECS=30 ryi slow .`, `SPREFA_SCIP_INDEX`, `RYI_MAX_MEM_MB`, `SPREFA_EXTRACT_BLOB_CACHE_MB`, `RUST_LOG`, `DL_TRAIL`, and HOME/XDG for the trail, watch and soopy state.
   - Failure: PATH is also the daemon's, so it decides whether rust-analyzer, tsc and scip-* are found.
   - Confidence: confirmed by reading.

7. **sprefa-extract/src/edit/rust_rehome.rs:887-898 and edit/ts_rehome/cross.rs:210-227 (sub-audit) — move plans are reused across requests.**
   - Defect: `relocate_plan` and `dep_plan` are built from the current move batch but cached in a process-wide `OnceLock<BTreeMap<root, &'static>>`, `Box::leak`ed and never evicted.
   - Failure: through the daemon, `ryi move a.rs b.rs` followed by `ryi move c.rs d.rs` in the same repo applies the first batch's plan to the second.
   - Confidence: confirmed by reading.

8. **edit/ts_rename.rs:391-403, rust_rehome.rs:1754-1778, ts_rehome/cross.rs:38-57, ts_rehome.rs:479-488 (sub-audit) — repo snapshots never refresh.**
   - Defect: the import graph, crate roots, packages and the oxc_resolver stat cache are cached per root and leaked.
   - Failure: after a `move` or `rename` edits the repo, the next daemon request plans against the pre-edit graph and manifests.
   - Confidence: confirmed by reading.

9. **hafley_scm/src/read/lang/go.rs:3757, 3872, 3896, 4090, 4641, 4668, key built at 4634 (sub-audit) — Go caches keyed by memory address.**
   - Defect: process-wide caches key on the address of `&PathIndex` or `&DefIndex` and are never evicted.
   - Failure: request B's index is allocated where request A's was freed (more likely after `mi_collect`). B then reads A's directory index, alias/embed/field tables and interface fanout, giving wrong Go receiver/embed edges and stray FanoutCap rows.
   - Also: `go_dir_index` holds its mutex while it computes (go.rs:3875-3888), which serializes Go resolve threads.
   - Confidence: defect confirmed by reading; address reuse plausible.

10. **go.rs:3187-3275 (`GO_FILE_FACTS.by_path`), published at project.rs:709-716 (sub-audit) — Go file facts keyed by relative path, never evicted.**
    - Failure: concurrent `ryi fast .` in repo1 and repo2 both publish `pkg/a.go`, so A resolves against B's method sets and imports.
    - Confidence: confirmed by reading.

11. **go.rs:3285-3286 and lang/ts_receivers.rs:655-656, run from project.rs:471-493 — pool workers read files relative to "/".**
    - Defect: these `io_path` reads run on extract-pool workers, where the ROOT thread-local is unset. daemonize (0.5.0 default) has set the cwd to "/".
    - Failure: on a cache miss the daemon reads `/pkg/a.go`, gets empty `GoFileFacts`, and stores them in the global map, poisoning later requests. `go_module_of` (go.rs:3091) is affected the same way: in-module imports become external.
    - Confidence: confirmed by reading; the miss trigger is plausible.

12. **sprefa-extract/src/0_graph.rs:115-133 and 0_stratify.rs:500-502, 600-601 (sub-audit) — line reads skip `io_path`.**
    - Defect: `fs::read(path)` and `fs::read(root.join(path))` are called without `io_path`.
    - Failure: through the daemon, `cd repo && ryi graph --callers f src` gives `line: null` on every row, and `ryi stratify src` gives line counts of 0. ryii fills both.
    - Confidence: confirmed by reading.

13. **0_graph.rs:675-700 with sprefa-extract/src/bin/ryi/0_revision.rs:51-55 — `graph --at` path selection.**
    - Defect: only `.` is normalised; paths are taken as repo-relative even when the caller's cwd is a subdirectory of `--root`.
    - Failure: `ryi graph --at HEAD --root . ./src --callers f` selects zero files, because `Path::starts_with("./src")` does not match `src/...`. `cd repo/crates/x && ryi graph --at HEAD --root ../.. src` selects the repo-root `src`.
    - Confidence: confirmed by reading.

14. **SA:79-84, 97-100 versus BIN:400-408 — a mid-stream error lands on stdout.**
    - Defect: after data has been sent, the daemon writes `{"error":…,"code":…}` into the body and only the exit code goes in the trailer.
    - Failure: `ryi fast .` with an error after the first rows puts the error row into the JSONL on stdout, and no message reaches stderr. ryii prints the message to stderr and nothing to stdout.
    - Confidence: confirmed by reading.

15. **CL:196-208 versus BIN:402 — closed pipe handling differs.**
    - Failure: `ryi fast . | head -1` prints "ryi: Broken pipe (os error 32)" and exits 1. `ryii fast . | head -1` exits 0 silently.
    - Confidence: confirmed by reading.

16. **Client side of `--format jsonl`: CL:103-106 and OPS:280-286 versus BIN:525-568.**
    - Defect: `ryi` sends `format` inside FileArgs, and the daemon's `file()` sets `args.format = None`.
    - Failure: `ryi --format jsonl x.ts` has no `{"complete":true,"rows":N}` trailer. `ryi --format jsonl fast …` is rejected by clap (`args_conflicts_with_subcommands`), while ryii reorders argv and accepts it.
    - Confidence: confirmed by reading.

17. **DA:52-70 (stratify has `&[]`) and sprefa-extract/src/0_stratify.rs:45 — `ryi stratify -` loses stdin.**
    - Defect: StratifyArgs has `inputs.paths` and calls `inputs::expand`, but the client does not forward stdin for "stratify".
    - Failure: the daemon reads its own stdin (/dev/null), so `git ls-files | ryi stratify -` gets an empty input list. ryii works.
    - Confidence: confirmed by reading.

18. **BIN:725 (only watch receives `cancelled`), SA:146 — Ctrl-C does not stop daemon work.**
    - Defect: for non-watch verbs the producer keeps running until its next write fails. Raw handlers (`rename`, `move`, `cleave`, `region`) run inside `spawn_blocking` and cannot be cancelled at all.
    - Failure: Ctrl-C on `ryi slow .` leaves SCIP indexers running. Ctrl-C on `ryi rename …` still applies the edits.
    - Confidence: confirmed by reading.

19. **OPS:246-254 — the mimalloc purge can delay responses.**
    - Defect: `extract_pool().broadcast(mi_collect(true))` runs before the producer thread exits. The response only completes (channel close, trailer) after every pool thread has run the broadcast, which waits behind other requests' long rayon jobs.
    - Failure: a quick request finishes late whenever another request is busy on the pool.
    - Also: `mi_collect` on the exiting producer thread (line 251) adds nothing, and the forced collect runs on every request.
    - Confidence: blocking is rayon broadcast semantics, confirmed; latency size plausible.

20. **BIN:386-395 versus 411-419 — daemon runs never reach the trail.**
    - Defect: the daemon discards the trace summary and never calls `write_trail`.
    - Failure: `ryi trail` lists only in-process ryii runs.
    - Confidence: confirmed by reading.

21. **sprefa-extract/src/edit/kotlin_rehome.rs:53,137,143, edit/_3_stage.rs:246 and _3_stage.rs:89-97 (sub-audit) — output lost in the daemon.**
    - Defect: these modules use `println!`, which bypasses `outln!`; the daemon's stdout is /dev/null.
    - Failure: `ryi move Foo.kt …` loses its "error" and "warn" lines. `--verify 'cargo check'` output also goes to /dev/null, and the verify command runs with the daemon's environment.
    - Confidence: confirmed by reading.

22. **SA:576-584 with SA:493-495 — idle exit can drop a newly arrived request.**
    - Defect: the idle task checks `active == 0` once a second. A connection accepted just before the cancel, but before the middleware increments `active`, is shut down while idle. The client also opens three separate connections (probe, handshake, request).
    - Failure: an occasional "connection closed before message completed" or ECONNREFUSED on the request connection, exit 1, with no retry.
    - Confidence: plausible.

23. **SA:39, 58-62, 104 and CL:52-56 — diagnostics travel in an HTTP header.**
    - Defect: all stderr diagnostics are base64-encoded into one header or trailer.
    - Failure: a run with many "0 facts" or warning lines exceeds hyper's header buffer (about 400 KB), and the client fails parsing, so output is lost. Separately, a HeaderValue that fails to build (`.ok()`) is silently dropped.
    - Confidence: plausible.

24. **Exit code handling on SA:27-32, SA:147/208, BIN:507-521, CL:210.**
    - A panicked `spawn_blocking` (JoinError) maps to code 1; ryii gives 101.
    - I/O errors while staging stdin into the tempfile return `bad_request`, code 2.
    - `or_exit_2` maps every non-RyiExit runtime error from graph, diff, query, move and cleave to exit 2.
    - `region` with a nonzero code prints "region exited N" through the daemon; ryii prints nothing.
    - Confidence: confirmed by reading.

25. **CL:67-101 — the handshake runs on every call.** Each call does a probe connect, a handshake and the request. Handshake `Err(_)` is swallowed and retried for up to 5 s. Confidence: confirmed by reading.

26. **SA:162-174 — `jsonl_input` has no callers.** Confidence: confirmed by grep.

27. **SA:176-454 — the stdin-staging handler is duplicated.** The same block is copied verbatim across 8 handlers (extract, fast, slow, scip, graph, query, region, ingest). Generated code, but any fix has to go into all 8. Confidence: confirmed by reading.

28. **SA:568-573, 593 — pid file.**
    - Defect: written non-atomically after bind; read only by tests (tests/support/0_daemon_guard.rs).
    - Failure: the guard's `stop()` kills nothing if the pid file is not written yet, which leaks the daemon.
    - Confidence: confirmed by reading.

29. **BIN:443, git_sha — `git rev-parse` is spawned on every in-process run** to write the trail row. Confidence: confirmed by reading.

30. **sprefa-extract/src/bin/ryi/0_sqlite.rs:534, 544 — env lookup per row.** With `streaming == false` (in-process ryii), `std::env::var_os("RYI_STREAM_FLUSH")` runs for every row: an env lock plus an allocation. On a 2M-row stream that is 2M lookups. With `streaming == true` (daemon), BufWriter is flushed after every row, which cancels its 256 KiB buffering. Confidence: confirmed by reading.

31. **0_graph.rs:95 via 0_sqlite.rs:305-307 — every graph fact is serialized three times.** Each fact goes `to_value`, then `to_vec`, then `from_slice` through the "untrusted JSON" path. The typed `bind_row` exists. Confidence: confirmed by reading.

32. **0_graph.rs:34-99 versus sprefa-extract/AGENTS.md — the documented graph store does not exist.**
    - AGENTS.md describes a `<root>/.dl/.state/graph-<key>.db` store with a corpus stamp and per-file digests. No code references `.state` or `graph-`.
    - Every `ryi graph` call re-resolves the whole corpus into an in-memory DB.
    - Confidence: confirmed by grep.

33. **hafley_scm/src/read/scip.rs:396-402 and scip_ensure.rs:1169-1178 (sub-audit) — SCIP stage and cache keyed by how the root is spelled.**
    - Failure: in-process, `ryi slow .` in two different repos shares one stage directory, and concurrent runs prune each other's stage. In the daemon, `/r/.` and `/r` give two keys for one repo, so the index is rebuilt.
    - Confidence: confirmed by reading.

34. **go.rs:1158, ts_receivers.rs:627, kotlin_receivers.rs:68, go.rs:3196 (sub-audit) — per-file fact caches have no size bound.** A daemon serving several large repos keeps every bundle until the idle exit. Confidence: confirmed by reading.

35. **sprefa-extract/src/bin/ryi/1_inputs.rs:44-46 — output path spelling depends on how the root was given.** With `--pattern` and no paths, the absolute `request_root()` is pushed, so output rows carry absolute paths; `.` gives relative ones. Confidence: confirmed by reading.

36. **hafley_scm/src/read/lang/fact.rs:67 (sub-audit) — SQLite URI built unescaped.** `format!("file:{}?mode=ro")` without escaping breaks on paths containing `?` or `#`. Confidence: confirmed by reading.

### Resolver correctness (sub-audit; confidence as listed)

37. **hafley_scm/src/lang/rust/21_tree_module_resolution_rows.rs:80-87 — inline `mod x {}` bodies are flattened into the file.**
    - `use super::*` inside `mod tests` becomes a file-level glob.
    - `mod helpers;` inside `mod tests` resolves to `foo/helpers.rs`; rustc uses `foo/tests/helpers.rs`.
    - Confirmed.

38. **hafley_scm/src/read/lang/rust_modules.rs:657-670 — mod-rs detection is wrong in two ways.**
    - Any stem `mod|lib|main|build`, at any depth, or any parent directory named `bin|tests|examples|benches` is treated as a mod-rs file.
    - Failure: `src/parser/tests/fixtures.rs` writing `mod data;` resolves wrongly. A non-root `src/build.rs` writing `mod x;` resolves to `src/x.rs`.
    - Confirmed.

39. **rust_modules.rs:1763-1772 — children of `#[path]`-loaded files use the non-mod-rs rule.** Plausible.

40. **rust_modules.rs:953-965 — `lexical` drops `..` at the root.** `#[path="../shared/x.rs"]` resolves to `shared/x.rs` inside the input set. Confirmed.

41. **rust_modules.rs:1169-1240 — Cargo dependency reading.**
    - `[target.'cfg(..)'.dependencies]` is ignored.
    - `foo = {package="bar"}` is keyed by "bar".
    - Workspace inheritance uses the nearest ancestor that has `[workspace.dependencies]`.
    - Confirmed (the rename case is plausible).

42. **rust_modules.rs:1021-1041 — `[[bin]]`/`[[test]]` `path=`, `autobins=false` and `package.build` are ignored.** Confirmed.

43. **rust_modules.rs:1561-1576 — qualified impls must be in the type's own file.** `impl Foo` in `foo/impls.rs` is never found for `crate::foo::Foo::new()`. Confirmed.

44. **rust_modules.rs:1484-1555 — a same-file impl wins by bare type name.** Calls on an imported `other::Config` bind to the local `Config::load`. Plausible.

45. **hafley_scm/src/read/lang/rust/1_type.rs:284-289 — incomplete prelude exclusion.** Only `Result|Box` are excluded (the call side also lists Option, String, Vec). A corpus `struct Option` captures every bare `Option<T>`. Confirmed.

46. **hafley_scm/src/read/lang/rust/2_call.rs:617-621 — only the literal path `Default::default` is matched.** `std::default::Default::default()` and `<Foo as Default>::default()` get no SelfType edge. Confirmed.

47. **rust_modules.rs:681-708 — `PRELUDE_TRAITS` lacks TryFrom, TryInto and FromIterator.** Confirmed.

48. **hafley_scm/src/read/lang/ts.rs:5100-5114 — TS free calls bind to unimported defs.** `fetch(url)` binds to an unimported `export function fetch` in another file. Confirmed.

49. **hafley_scm/src/read/types.rs:2120-2125 (`def_named`) — same-file TS lookup ignores scope and kind.** A free `run()` binds to the method `A.run`; two nested `helper`s both bind to the first. Confirmed.

50. **hafley_scm/src/read/lang/ts_resolve.rs:86-103, 925-930 — resolver options.** No `types`/`typings` main fields and no `types`/`require` conditions. `workspace:*` deps are unhandled. Monorepo `"types":"src/index.ts"` resolves to `dist/` and ends unresolved. Plausible.

51. **ts.rs:4760-4899 — `BUILTIN_MEMBERS` (about 100 names) now fires only on the destructured-Field branch.** It drops user methods named `parse`, `map` or `add` there. Plausible.

52. **hafley_scm/src/read/lang/kotlin_modules.rs:373-395 — the first wildcard import beats a later explicit import.** Confirmed.

53. **hafley_scm/src/read/lang/python/_2_modules.rs:90-98, 431-449 and python/_0_source.rs:2742-2771 — Python binding.**
    - `import a.b` binds the local name "a.b" instead of `a`.
    - Function-local imports are treated as module scope.
    - `import *` ignores `__all__` and the underscore rule.
    - `Foo()` binds with no import check, and the first `__init__` in the span may belong to a nested class.
    - Confirmed.

54. **go.rs:3088-3101 (`go_module_of`) — go.mod parsed by hand.**
    - Uses `strip_prefix("module ")`; a quoted module path or a trailing comment breaks it.
    - No go.work or replace support, and no cache: ancestors are re-read on every call.
    - Confirmed.

55. **sprefa-extract/src/edit/_7_cleave.rs:443-470 — serde paths found by substring scan.** A multi-line `#[serde(\n with = "…")]` is missed. Confirmed.

56. **sprefa-extract/src/edit/_1_move_cx.rs:26, 219-228 — move walk skip list.** `SKIP_DIRS` hard-codes the repo-specific `.boop-worktrees`. `--include-untracked` turns off all ignore rules, so `dist/`, `.venv/` and `vendor/` get rewritten. Confirmed.

57. **hafley_scm/src/lang/rust/10_*↔21_*, 12_*↔22_*, 9_*↔18_*, 11_df_syntax_rows.rs:295-322↔2330-2352, rust_modules.rs:125↔197 — syn and tree-sitter projections are duplicated.** The copies already differ: syn `principal_ty` (10_*:163-183) does not unwrap `Box<T>`; 22_*:569 does. Confirmed.

### Test-suite hygiene (sub-audit)

58. **Timing asserts with no load gate.**
    - tests/49_rust_resolve_scaling.rs:122-135, 146-168 (ratio < 2.5, wall < 10 s)
    - 46_resolve_scaling.rs:38-53, 51_go_package_resolve.rs:22-31, 54_ts_module_plane.rs:397-406
    - 34:97, 29:141-146, 118:203-206, 160:17,109-118, 161:91-94, 56_scip_cli_kinks.rs:34-46 (5 s around a 2 s timeout)
    - 178_cli_http_parity.rs:263-270 (2 s daemon-start poll), 182_client_daemon.rs:490-494, 561-563
    - Only tests/45 checks load average first. Confirmed.

59. **Tests that spawn cargo or external tools.**
    - Nested cargo: 182_client_daemon.rs:90-100 (`cargo build -p ryi -j 4`, falls back to `~/.cache/lanes/shared/target`); `cargo check`/`run` in 3_move_rust.rs:165,402,478,542, 5_rename_rust.rs:194,260,298,419,695, 173:120, 112:43,188. These exceed the nextest 60 s cap on a starved CPU.
    - Ungated tools: swipl (1_move.rs:155, 8_rename_prolog.rs:217), d2 (15:128), python3 (bench_normal_form.rs:166), python3+sqlite3 (179_codeql_baseline.rs:15, which reads committed CSVs; no CodeQL or JVM runs).
    - codeql is only called at 116:109, and that test is `#[ignore]`.
    - Confirmed.

60. **Shared state and golden handling.**
    - Unlocked `set_var("PATH")` in the shared `all` binary: 50_cli_crawl_defects.rs:556-560, scip_indexer_pick.rs:63-67.
    - `SPREFA_SCIP_INDEX` is guarded by a mutex local to one file: scip_freshness.rs:204-211.
    - Fixed temp-dir names such as `sprefa-extract-49-{name}` (removed first) in 49, 46, 45, 100-105, 118, 104, 129, 103.
    - Counts: 78 files build temp dirs by hand versus 18 using tempfile; 61 files define their own spawn helper.
    - tests/snapshot.rs:69-79: `UPDATE_SNAP` rewrites goldens and passes.
    - The daemon guard leaks a daemon if a test panics before the pid write.
    - crates/ryi/tests has only a `--help` test.
    - Both Cargo workspaces (root Cargo.toml:32 and sprefa-extract/Cargo.toml:374) lack `[profile.dev] opt-level = 1`; only `package."*"` = 3 is set.
    - Confirmed.

## Hand-rolled code that an existing crate covers

- **Rust module tree** (mod/`#[path]`/use/glob/re-export, super/self/crate, impl lookup) in hafley_scm/src/read/lang/rust_modules.rs and 21_tree_module_resolution_rows.rs → `ra_ap_hir_def` DefMap via `ra_ap_hir` (already a dependency). Findings 37-44 sit in this code.
- **Cargo manifests, workspace inheritance, target discovery** (rust_modules.rs:575-622, 1021-1041, 1141-1240) → `cargo_metadata` or `cargo_toml` (`Manifest::complete_from_path`). `ra_ap_project_model::CargoWorkspace` is already a dependency.
- **go.mod parsing** (go.rs:3088-3114) → `gomod-parser`, already a dependency and already used in manifests.rs:17.
- **TS same-file scope and binding** (ts_receivers.rs:85 scope chain, types.rs `def_named`, ts.rs `call_name_match`) → `oxc_semantic`, already a dependency but used only in edit/ts_rename.rs.
- **Path normalization** (rust_modules.rs:953 `lexical`, 672 `normalize_join`; go.rs:3116-3126, 3863) → `path-clean` or `normpath`. `_1_move_cx.rs:249` `relative_between` → `pathdiff`.
- **Duplicated path helpers**: `absolute`/`normalize`/`within_root` are copied across edit/_6_rename.rs, _6_move.rs and _7_cleave.rs.
- **`which` lookup** (hafley_scm scip_ensure.rs:1068) → the `which` crate.
- **git shell-outs** (BIN:440, _1_move_cx.rs:202) → `gix`, or the in-house `soopy` already used in 1_inputs.rs.
- **serde attribute scan** (_7_cleave.rs:443) → `syn` or tree-sitter attribute nodes (both already linked).
- **Daemon lifecycle** (lock file, pid file, stale socket, spawn-and-poll, stamp handshake in SA:544-596 and CL:67-101). One option for single-instance start is to take the lock before daemonizing and bind the socket as the lock. None of the crates checked (`daemonize` 0.5 and `fs4`, both already used) covers a restart or handshake protocol.
- **mimalloc option 15 as a bare literal** (BIN:383): libmimalloc-sys 0.1.49 exports no `mi_option_purge_delay` constant. 15 is correct for the vendored v3 header.
- **Python module resolution**: no published crate found (ruff's resolver is not on crates.io).
- **Already on libraries**: ts_resolve.rs uses `oxc_resolver`; 1_inputs.rs uses `ignore` and `globset`.
