---
created: 2026-09-26
updated: 2026-09-26
type: bug
reporter: codex
status: open
priority: normal
---

# ryi rename batch leaves soopy re-export bindings stale

## Description

On a copied `crates/soopy` corpus, run `ryi rename --list batch.tsv --root <copy>/crates/soopy --state <outside-state> --commit` from the copied crate directory. `batch.tsv`:

```text
src/_1_pattern.rs	Pattern	GlobPattern
src/_0_types.rs	RepositoryId	RepoIdentity
```

The command exits 0. `cargo check --all-targets` in the copied crate exits 101:

```text
error[E0432]: unresolved import `crate::Pattern` at src/_14_multi_repo_refresh.rs:21
error[E0432]: unresolved import `crate::RepositoryId` at src/_7b_source_actions.rs:10
error[E0432]: unresolved import `_1_pattern::Pattern` at src/lib.rs:38
```

The declarations and many direct uses were renamed. The root re-export `pub use _1_pattern::Pattern;` and two imports through that re-export kept their old spelling. The plan needs to follow root re-export bindings and their consumers across rows.
