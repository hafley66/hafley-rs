---
created: 2026-09-26
updated: 2026-09-26
type: bug
status: fixed
priority: normal
labels: [extract]
---

# `ryii fast` facts depend on the spelling of the same input directory

## Description

In `crates/soopy`, `ryii fast .` emits 576 `resolved_import` rows while `ryii fast $PWD` emits 653. These arguments name the same corpus, so analysis facts must be invariant under equivalent path spellings. Output paths must preserve the spelling supplied by the user.

## Acceptance Criteria

- [x] equivalent spellings of one directory produce the same facts
- [x] emitted output paths preserve the input spelling
- [x] a focused regression test covers the path spelling contract

## Tests Run

`t_183_fast_path_spelling::equivalent_directory_spellings_resolve_the_same_imports_and_keep_output_paths` passed; `ryii fast .` and `ryii fast "$PWD"` both emit 653 `resolved_import` rows in `crates/soopy`.

## Implementation Notes
