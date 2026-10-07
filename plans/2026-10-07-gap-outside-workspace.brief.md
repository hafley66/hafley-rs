# Gap fix: outside-workspace

Base origin/main. User rule 2026-10-07: edit and commit only. Do NOT run cargo build, cargo check, cargo test, or any compiler. No new tests. One commit. No merge, no push. No fallback heuristics; abstain with a reason.

Bug: `ryi graph --callers flow_edges crates/hafley_scm` emits file_unresolved for crates/hafley_scm/src/read/lang/kotlin_scm_generated.rs with reason=outside_workspace; the file is inside the crate. Give the true reason (e.g. not_in_module_tree when no mod declaration reaches it). Also the header "2 edges: 1 +, 0 ~, 0 -" counts the file_unresolved row; count edge rows only.
