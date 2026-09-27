# the-gang-cli: CLI surface and path handling

Rules: TASKS/the-gang-queue.RULES.md (read it first).

Queue, in order:
1. path spelling (no issue file yet; create issues/fast-path-spelling/item.md):
   in crates/soopy, `ryii fast .` emits 576 resolved_import rows and
   `ryii fast $PWD` emits 653. The same corpus must give the same facts for
   any spelling of the same path; output paths stay as the user spelled them.
2. hafley_scm test `9_type_candidate_rows` fails on main: fix it.
3. ryi-default-info-stderr, directory-arg-blames-resolve, sqlite-resolve-contract,
   cli-teach-errors, ryi-cli-cleanup.
4. compiler warnings in sprefa-extract and hafley_scm (`cargo build --features cli`
   warning list -> 0): delete dead code, never `#[allow]`.
