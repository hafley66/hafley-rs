---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: open
priority: normal
labels: [extract]
---

# move leaves path string literals naming the old file

## Description

## Description
`ryi move --list ... --commit --verify "cargo check --tests"` moved lang/{rust,ts,kotlin}_{rehome,rename}.rs into src/edit/ and passed verify, but left path string literals naming the old files. `tests/4_rename_ts.rs:319` joins `src/lang/ts_rename.rs` onto CARGO_MANIFEST_DIR; the test panicked at runtime:

    ts_rename.rs readable: Os { code: 2, kind: NotFound, message: "No such file or directory" }

`tests/3_move_rust.rs:470` held the same kind of literal. `--text-refs` only reports these; it does not repair them, and cargo check cannot see them.

## Acceptance Criteria
- [ ] move rewrites string literals in code that spell a moved crate-relative path exactly (Rust `"src/..."` joined onto a manifest dir, TS path strings)
- [ ] comments naming the old path are reported by default in the plan
