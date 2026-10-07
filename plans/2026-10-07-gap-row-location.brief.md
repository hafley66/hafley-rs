# Gap fix: row-location

Base origin/main. User rule 2026-10-07: edit and commit only. Do NOT run cargo build, cargo check, cargo test, or any compiler. No new tests. One commit. No merge, no push. No fallback heuristics; abstain with a reason.

Bug A: `ryi graph --from main crates/ryi` and `--uses Kind crates/hafley_scm` rows have line:null / from_line:null. Fill 1-based line from the declaration span. Bug B: `ryi graph --flow-path FILE@START:END DIR` rows carry from_path/to_path as blake3 digests and names as "START:END". Add worktree-relative path, line, col and span source text to each flow row (digest -> path from loaded file facts). Keep existing fields.
