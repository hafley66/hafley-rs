# ryi gaps found by use (one line each: command -> what went wrong)

- 2026-10-07 `ryi` (bare, at hafley-rs root) -> "No supported project found within the 50 ms discovery budget"; root is a cargo workspace.
- 2026-10-07 `ryi graph --callers flow_edges crates/hafley_scm` -> file_unresolved kotlin_scm_generated.rs reason=outside_workspace, module=crates/hafley_scm/Cargo.toml (file is inside that workspace).
- 2026-10-07 same -> header "2 edges" counts the file_unresolved row as an edge.
- 2026-10-07 `ryi graph --callers 0d_flow.rs#flow_edges ...` -> silent 0 edges; no declaration matches that PATH#NAME, should say so.
- 2026-10-07 `ryi graph --from main crates/ryi`, `--uses` -> every row has line:null.
- 2026-10-07 `ryi graph --flow-path` -> rows name blake3 digests and byte ranges only; no path, line, or source text.
- 2026-10-07 `ryi rename FILE#OLD NEW crates/x` -> "unexpected argument"; rename takes no PATH scope while graph does.
