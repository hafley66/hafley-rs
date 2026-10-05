# Brief: rules of hooks via ryii, against eslint-plugin-react-hooks

Starts after feature/ryi-v5-parity merges (needs the `function` owner column, the deferred JSX
call, and closure owners). Fast tier only (no type checking).

## Goal
Show `ryii` finds rules-of-hooks violations as a query file plus SQL, no rule code in Rust.
Reference answers: the valid/invalid cases of `eslint-plugin-react-hooks`
(facebook/react `packages/eslint-plugin-react-hooks/__tests__/ESLintRulesOfHooks-test.js`).

## Rules covered
| rule | facts |
|---|---|
| hook = call whose callee matches `^use[A-Z0-9]` | call `site` |
| caller is a component (`^[A-Z]`) or hook | flow/call owner `function` |
| not inside a loop | TS `df_loop` / `df_nest` (verify TS emits them first) |
| not under `if`, `?:`, `&&`, `||`, `??` within the owning function | scm++ ancestor check |
| not after an early `return` in the owning function | span order, SQL |
| not inside a nested function/callback | innermost owner != component |
| JSX attribute hook belongs to the enclosing function; hooks inside the deferred component call do not | deferred JSX lift |

## Work
1. Vendor the eslint test cases as fixtures under `crates/sprefa-extract/bench/` (one file per case,
   valid/invalid + expected message), pinned to a react commit.
2. One query file (scm++) + one SQL file computing violations; run by `ryii query`.
3. Comparison runs through `ryiii` as an adapter (hafley-rs CLAUDE.md: no own harness/scoring).
4. Report table: case_count, ryi_true_positive, ryi_false_positive, ryi_false_negative per rule;
   list every disagreement with file and reason. Over ~10 rows -> `scripts/bench_grid.py`.

## Limits
- Rust build profile already set. Commands under 2 min. No whole-corpus runs.
- No Rust code for the rule itself; a missing fact is reported as a gap with the rows it blocks.
- No merge, no push.
