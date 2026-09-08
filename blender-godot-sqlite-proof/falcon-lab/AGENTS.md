# Active Falcon lab direction

Start with `just status`. `101_current.json` is the replace-in-place task pointer.
Use `just tsp`, `just test [all|core|godot|workflow|web]`, `just prove`, and
`just deploy`. The default test suite includes core, workflow and native Godot
boundaries; Web tests additionally build/export and run browser acceptance.
`prove` runs all suites and produces an MP4/artifact receipt. `deploy` consumes
that exact proof and rejects changed source, recorded tool versions or assets.
No tests are automatically skipped or cached. Full logs and receipts live in
ignored `.workflow/`; read failure excerpts before opening full logs.
Read authored source first; inspect generated output when diagnosing generation
or integration errors. Existing lower-level recipes remain available.

Read `92_web_import_plan.md` before browser, deployment, behavior-import, or
photo-character work. It records user decisions and separates completed proofs
from pending work.

`https://hafley.codes/game3/` is authorized for replacement with this demo.
`https://hafley.codes/game/` and `/var/www/smash-godot/` are protected. Do not
modify them without a later explicit user instruction. Verify their artifact
hashes remain unchanged when publishing Game3.

Prefer source-backed importers for existing fighter behavior. Preserve exact
source provenance and report unsupported executable behavior. Animation data
and state IDs alone do not establish behavioral equivalence.
