---
created: 2026-09-27
updated: 2026-09-27
type: bug
status: fixed
priority: high
labels: [extract, rust, deps]
---

# Rust `mod` declarations do not resolve to file edges

## Reproduction

From `crates/sprefa-extract`, `ryii --deps --root tests/fixtures/rust_module_reachability tests/fixtures/rust_module_reachability`
emits `file_unresolved(src_path="src/lib.rs", module="live", reason="node_modules_boundary")`
for the `mod live;` declaration. Rust module names are file-relative, so the
dead-files graph classifies `src/live.rs` as unreachable.

## Acceptance criteria

- [x] The extract test on `rust_module_reachability` expects a `file_edge` from `src/lib.rs` to `src/live.rs`.
- [x] Rust `mod` declarations resolve Cargo roots, nested modules, directory fallbacks, and `#[path]` relative to the declaring file; emitted edge kind is `module`.
- [x] The fixture's only module-unreachable file is `src/orphan.rs`.
- [x] Targeted extract test and workspace suite pass with `-j 2`.

## Receipt

The original table-driven `rust_module_resolution_table_covers_roots_nesting_and_path_attributes` exposed 7 failures: bin, test, example, bench, and build roots plus root and nonroot `#[path]` bases. After those fixes, the workspace-root review reproduced 5 more failures with `crates/foo/Cargo.toml` in the universe: `src/bin/ryi.rs`, `tests/all.rs`, `examples/demo.rs`, and `benches/measure.rs` resolved from the wrong module directory, and `src/a/main.rs` was incorrectly treated as a crate root. Green: the manifest-relative table passes across 14 cases, including all `crates/foo/` roots and deep `main.rs`; tree/syn module-shape parity passes 1/1; `t_185_rust_mod_file_edges` plus `t_57_rust_module_plane` pass 14/14. `ryii --deps` emits `file_edge(src/lib.rs -> src/live.rs, kind=module)` and the lab `mod` reachability comparison is agreement 1 / Ryi-only 0 / tool-only 0. Workspace: 1379 passed, 203 skipped, 1 leaky. The leaky case is `boop-acp::channel::claude::tests::streamed_activity_is_reported_to_the_stall_watchdog`, unchanged on main; main's prior workspace receipt also recorded 1 leaky. Fixed in this change.
