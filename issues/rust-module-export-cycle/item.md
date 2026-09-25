---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: fixed
priority: normal
labels: [extract]
closed: 2026-09-25
---

# rust module plane: re-export cycle guard lost through use-bound heads (stack overflow)

## Description

## Description

`ryi fast` over crates/.../brotli-decompressor-5.0.3/src (23 files) aborts with `thread 'extract-7' has overflowed its stack` (release, 256 MiB worker stacks too). lldb shows unbounded mutual recursion in `RustModuleIndex`:

```
export_table -> resolve_in_module -> resolve_qualified -> home_file -> bound_home
  -> resolve_qualified -> export_table -> ...
```

`bound_home` (rust_modules.rs) starts a fresh `stack` for its `resolve_qualified` call, and `export_table` / `star_contributions` start a fresh `seen`, so the re-export cycle guard (`stack` holds the open export tables) is lost whenever resolution passes through a `use`-bound head. Glob re-export webs (`pub use super::*`, crate-root `pub use` of sibling modules) then recurse forever; a table built inside such a loop is never `complete`, so it is never cached and gets rebuilt on every ask, which is also why macro-free files like crossterm style/stylize.rs took ~3s.

## Acceptance Criteria
- [x] one `stack` threads through home_file / bound_home / star_contributions
- [x] a regression test with a re-export cycle through a use-bound head terminates
- [x] `ryi fast` over the 3000-file registry corpus completes
