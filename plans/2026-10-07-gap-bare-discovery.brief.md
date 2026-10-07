# Gap fix: bare-discovery

Base origin/main. User rule 2026-10-07: edit and commit only. Do NOT run cargo build, cargo check, cargo test, or any compiler. No new tests. One commit. No merge, no push. No fallback heuristics; abstain with a reason.

Bug: `cd ~/projects/hafley-rs && ryi` prints "No supported project found within the 50 ms discovery budget"; the root is a cargo workspace. Make bare ryi at a workspace root find the project and print its suggestions (src/bin/ryi/1c_no_inputs.rs). Skip ignored dirs (target/, .boop-worktrees/) in the walk rather than raising the budget.
