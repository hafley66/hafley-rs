# Gap fix: no-match

Base origin/main. User rule 2026-10-07: edit and commit only. Do NOT run cargo build, cargo check, cargo test, or any compiler. No new tests. One commit. No merge, no push. No fallback heuristics; abstain with a reason.

Bug: `ryi graph --callers 0d_flow.rs#flow_edges crates/hafley_scm` silently returns 0 edges; no file ending 0d_flow.rs declares flow_edges. When a seed matches no declaration, emit {"record":"seed_unmatched","seed":...,"reason":"no_declaration"}, and on stderr list up to 5 PATH#NAME candidates where NAME is declared. Apply to --callers/--from/--call-path/--type-path/--uses.
