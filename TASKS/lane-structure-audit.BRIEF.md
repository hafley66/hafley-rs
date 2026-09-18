# Lane SA: structure audit of sprefa-extract, measured with extract itself

No issue. A one-shot measurement, no code changes to the crate. You are one of two independent measurers; the other is the coordinator. Your numbers get compared to theirs, so derive everything from tool output and show the commands.

You work in `$PWD`, your own worktree. Never `cd` anywhere outside it.

## Setup

- NEVER run cargo. Another lane owns the build. The extract binary is prebuilt at `/Users/chrishafley/projects/hafley-rs/.claude/worktrees/extract-fixes/crates/sprefa-extract/target-primary/debug/extract`; call it by that absolute path.
- Two trees to measure, both `crates/sprefa-extract/src/**/*.rs`:
  - BEFORE: sha `85e59e5d` (the state before the lane series landed). Get it with `git archive -o before.tar 85e59e5d crates/sprefa-extract/src && mkdir before && tar -xf before.tar -C before`.
  - AFTER: your checkout as-is.
- `extract --family cst <file.rs>` prints one JSON line per syntax node: `{"record":"node","kind":"match_arm","span":{"start":..,"end":..},"name":..}`. `extract --schema` prints every record shape. `extract --help` for the rest.
- python3 is available for the counting.

## The question

Between BEFORE and AFTER, did the code get structurally tighter or did it leak? Concretely, per tree and per changed file:

| metric | how |
| --- | --- |
| lines | file bytes |
| fns, fn length p50 / p90 / max, fns over 60 lines, fns over 120 | `function_item` spans to line counts |
| match expressions, match arms, arms per match p90 and max | `match_expression`, `match_arm` |
| structs, enums, enum variants, traits, impl blocks | `struct_item`, `enum_item`, `enum_variant`, `trait_item`, `impl_item` |
| generics | `type_parameters` |
| closures, ifs, macro invocations | `closure_expression`, `if_expression`, `macro_invocation` |
| impl blocks per self type > 1 in one file | `impl_item` heads |
| new functions over 60 lines | by name, AFTER minus BEFORE |
| helper names duplicated across `src/lang/*_receivers.rs` and `src/lang/{go,rust,kotlin,ts}.rs` | `function_item` names shared by 2+ of those files: twin pattern (same name, same job) or drift (same name, different job)? read the bodies for up to 10 of them |

Then the judgement, as a table: for each of the six lane-touched files (`src/lang/kotlin.rs`, `kotlin_receivers.rs`, `rust.rs`, `rust_modules.rs`, `ts.rs`, `ts_receivers.rs`, plus any other file whose counts moved), one row: what grew, whether the growth is a new type/enum/trait (structure) or a longer match/fn/closure (mass), and one line of evidence.

## Deliverable

`REPORT.md` at the worktree root. Overwrite the stale one there. Tables only. Sections:

1. `## Commands` : every command you ran to produce a number, verbatim.
2. `## Totals` : BEFORE vs AFTER, one row per metric, delta column.
3. `## Changed files` : per file, the metrics above with deltas.
4. `## Long functions` : new fns over 60 lines, name, lines, file.
5. `## Duplicated helpers` : name, files, twin or drift, one line.
6. `## Judgement` : the per-file table above.
7. `## Blocked` : empty, or the exact error.

No commits. Do not push. Do not edit anything under `crates/`.

## Laws

- No em dashes. No praise. Facts and receipts.
- Every number comes from a command you list in `## Commands`. No estimates.
- No opinions beyond the judgement table.
