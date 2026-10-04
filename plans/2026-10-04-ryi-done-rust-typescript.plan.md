# ryi done for Rust and TypeScript

Scope: ryi's job per the boundary agreement (.claude/skills/2026-10-03-ryi-sprefa-boundary):
extraction + closed store + on-request fact providers + edit application. Fast tier = no type
checking; slow tier = type checking. Rust and TypeScript only (repo rule).

## Done means

For each language, every command below answers correctly on the reference corpora, within the
time budget, with a test that fails if it regresses.

Reference corpora: Rust = hafley-rs `crates/` (and `crates/hafley_scm/src` as the small one);
TypeScript = hafley-rxjs `packages/` (1,717 files) at a pinned commit.

| command | fast tier (no types) | slow tier (types) |
| --- | --- | --- |
| extract (`ryii`, `ryii fast`) | every file -> facts | - |
| `graph --from / --call-path` | syntax + module resolution | demand walk over a per-body provider |
| `graph --callers` | syntax + module resolution | checker |
| `graph --uses / --type-path` | syntax | checker |
| `rename` | binding sites from module resolution | checker sites |
| `move`, `cleave` | module resolution + specifier repair | checker verify |
| `query --query` (plain .scm) | yes | - |
| `query --scmpp` | yes | - |
| `watch` | fact deltas | - |

## Budgets (wall, release build, reference machine, warm file cache)

| command | Rust small | Rust crates | TypeScript packages |
| --- | --- | --- | --- |
| fast extract | 1 s | 5 s | 3 s |
| fast graph question | 1 s | 3 s | 2 s |
| slow graph question, cold process | 8 s | 20 s | 10 s |
| slow graph question, warm daemon | 1 s | 2 s | 1 s |
| scm++ query with one relation | 2 s | 10 s | 5 s |

## Status today (measured 2026-10-03/04)

| item | Rust | TypeScript |
| --- | --- | --- |
| fast extract | hafley_scm graph 0.50 s | 1.76 s (packages) |
| fast graph `--callers` | 0.50 s; misses `#[path]` bin modules, own-lib crate-name imports, `super::super` in `#[path]` modules (issues/ryii-dogfood-self-20261003) | 1.25 s; 9 sure + 1 name guess on `Route` |
| slow graph | demand walk 6 s on hafley_scm (5 s of it rust-analyzer load) | 0.74 s on packages/signals; 49.19 s wall / 4.39 s CPU on all packages (waiting, not computing; undiagnosed) |
| scm++ | has-ancestor crates 7.3 s; ancestor-with-field 6.9 s after ANALYZE; 3 field_expression ancestor cases about 4.7 s each (one-sided range) | not measured |
| rename / move / cleave | 4 Rust resolvers (plan 2026-10-04-rust-resolution-unify); cleave on rust-analyzer module maps | not re-measured this round |

## Work to done, in order (one agent at a time; each step merges before the next)

| step | language | work | acceptance |
| --- | --- | --- | --- |
| 1 | both | in flight: scm++ quantified captures as arrays, `rows: list`, optional relations | your cfg(test) example returns helper with its comment array and bare with [] |
| 2 | TypeScript | diagnose slow graph 49 s wall / 4 s CPU on packages (checker startup, per-file round trips, LSP waits); fix the wait | slow `--callers Route` on packages within budget, same 10 edges |
| 3 | Rust | resolution unify (plan 2026-10-04-rust-resolution-unify, after your 8 answers): one fast resolver (rust-analyzer module maps without std, or a corrected index), one slow; rename/move lose private resolvers | the 4 self-dogfood bugs closed with tests; fast graph finds 7 of 7 `checker_workspace` callers |
| 4 | Rust | write-time ancestor pairs (option C) for scm++ has-ancestor | the 3 slow field_expression cases under 0.5 s on small; rows identical |
| 5 | both | warm daemon for the slow tier: rust-analyzer host and TS checker kept across questions | warm slow question within budget |
| 6 | both | contract views (node, resolved_edge) + content-hash ids + test of sprefa's command shape | boundary agreement contact point 3 |
| 7 | both | `ryii watch` delta format (relation, ids, sign, epoch) | sprefa can consume deltas |
| 8 | both | `--scope <relation>` on rename/move/cleave/graph | scope from a relation file equals today's PATH#NAME/--entry results |
| 9 | both | budget tests: one growth or wall test per row of the budget table, run on the small corpora in CI | a regression fails a test |
| 10 | both | `ryii` can print the SQL it compiles for scm++ (bench needed it) | `ryii query --scmpp --print-sql` or equivalent |

Each step: targeted tests while working, one full suite + quality gate at the end, no corpus sweeps
beyond the step's acceptance.

## Open decisions (yours)

1. The 8 questions in plans/2026-10-04-rust-resolution-unify.plan.md.
2. Are the budget numbers above right?
3. Does the warm daemon (step 5) come before or after resolution unify (step 3)?
