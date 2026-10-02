# ryi TS fix: cleave destination imports (default import dropped, same-module imports merged)

Repo hafley-rs, crate crates/sprefa-extract. Base: integrate/ryi-ts. Suspects: feature/ryi-ts-fix-cleave-reexports (ba1517df), feature/ryi-ts-refactor (D5, D6, D8: D8 keeps quote/semicolon style and grouped named imports).

## Failing
- t_155a_cleave_ts_oracle::destination_keeps_default_namespace_alias_and_type_imports (155a:180) and ::slow_preserves_default_namespace_and_type_imports (155a:88):
  destination lacks `import fs from "node:fs"`.
- t_155a_cleave_ts_oracle::overloads_move_and_export_together_and_reexports_are_not_imports (155a:138): got `import { Code, code } from "./lib";`, expected two lines
  `import { Code } from "./lib";` and `import { code } from "./lib";`. dogfood/ts/D08.sh requires grouped named imports be kept when the source had them grouped;
  155a has them separate in the source. Keep the source's grouping in both directions.

Rules:
- CODE COMPLETE ONLY. Do not run cargo, npm, node, or dogfood scripts. The coordinator runs every gate, one at a time.
- These tests pass on main b3673b84 and fail on integrate/ryi-ts. Read the test, `git show main:<path>`, and `git log main..HEAD -- <path>` to find the commit that changed behavior.
- Do not edit a test assertion or golden unless the new output is a deliberate change named by a plan row in plans/2026-10-01-ryi-ts-utility.md; if so, name the row in REPORT.md. If a plan row and an existing test contradict, write the contradiction in REPORT.md and stop.
- ryi emits facts only. No SQL analyses, no new CLI commands or flags.
- Other lanes edit the same crate in parallel. Keep diffs inside your area. No drive-by renames or formatting. No push. No Python. No `boop beep scream`. No hand-written files labelled generated.
- Commit per cause; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- On done: REPORT.md section per test: cause (file:line, commit), change, cargo test filter for the coordinator (target `all`, crate crates/sprefa-extract).
