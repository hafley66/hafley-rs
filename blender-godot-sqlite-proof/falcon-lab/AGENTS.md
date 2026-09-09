# Active Falcon lab direction

## Yield current state

During work, yield a message to the parent/coordinator or human explaining current
state: what just finished, what is happening, and what comes next. Do this along
the way to the goal, without waiting for completion. Send enough concrete detail
for the parent to catch a wrong approach and interrupt or redirect the child
before more work accumulates. The parent must read these updates and intervene
when the child drifts from the request.

Start with `just status`. `101_current.json` is the replace-in-place task pointer.
Read the active ledger named by its `tasks` field and the previous N numbered
`*_tasks.md` ledgers, following the task-history rule below. Update active task
status/receipts after each tested increment.
Work stays in this lab and `games/shared`; sealed applications are read-only references.
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

## Rolling task history: one lab folder

Keep task ledgers here as `<D>_tasks.md`, using the lab's author-driven numeric
ordering. Do not create a separate tasks folder or one file per task.

- On resume, read the active ledger plus N preceding task ledgers, oldest first.
  N is `task_lookback` in `101_current.json`; default to 3 when absent. This is a
  configurable working default. Read all available predecessors if fewer exist.
  Order by numeric/author prefix, not modification time or plain string sorting.
- Read those ledgers completely. Follow older references only when an unresolved
  dependency or evidence question requires them. Report missing referenced files.
- Continue updating the active ledger during its milestone. Create the next
  numbered ledger when starting another milestone or replacing the queue; do not
  create a ledger on every turn. Choose its prefix after its dependencies.
- A successor links its predecessor and carries unresolved tasks forward by ID
  and reference. Record completed/deferred/superseded dispositions and their
  evidence. Never silently drop work or reopen passed proofs because context faded.
- Preserve prior ledgers as history. Put changed decisions in the successor with
  an explicit supersession reference rather than rewriting historical outcomes.
- Update `101_current.json` to select the successor. Keep task details in the
  ledgers and the pointer compact. Commit ledger/pointer changes with the work.

## Prefactoring by default

User direction: design and implement reusable game/lab machinery in its shared
home from the first consumer. Do not wait for a second game or a later cleanup.

- Inspect existing libraries first. Extend/reuse those APIs before introducing
  equivalents. Shared Rust, generation, storage and capture code belongs under
  `games/shared` or an existing appropriate reusable package in `hafley-rs`.
- Before implementing a feature, identify its shared mechanism and game-owned
  policy/data. Record the concrete signature, ownership and consumer. Shared code
  must not import Falcon modules or embed its paths, action IDs or expectations.
- Falcon consumes the shared implementation in the same change. Game schemas,
  assets, tuning, scenario inputs and independent expected results stay game-owned.
- On the next change to existing lab-local reusable machinery, extract and wire
  it first, preserving its tests and receipts. Track outstanding extractions in
  the active ledger; do not silently leave a second maintained implementation.
- Cover shared behavior independently and exercise the real Falcon consumer.
  Generator tooling also needs a small non-Falcon schema fixture. Keep ownership,
  bounds, failure behavior and reproducible commands explicit.
- Apply this to TSP emitter/adaptor generation, SQLite ring/query/publication,
  recording/encoding/verification, workflow receipts and deployment drivers.
  Dependencies point from game adapters to shared machinery.
- Scope the shared API to the concrete feature and known reuse requirements.
  Additional abstraction layers require a specific responsibility and call site.
- This policy does not authorize modifying sealed applications, broad workspace
  migrations or deployment. Keep shared work inside the active games domain.
