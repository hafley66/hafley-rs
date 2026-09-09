# Checked game classifications

Author records once in [1_registry.tsp](1_registry.tsp). Native `const`, `#{}`
objects and `#[]` arrays are checked against [0_model.tsp](0_model.tsp).
No authored YAML/JSON registry. `3_registry.json` and `3_registry.d2` are generated.

From `games/`:

```sh
just status    # compile TSP, check repository references, print classifications
just tsp       # validate and regenerate JSON + D2 inventory
just map       # regenerate inventory and render the complete living roadmap
just test      # rejection tests + reference checks + generated-file freshness
```

Uses the existing `falcon-lab/contracts` compiler installation and lockfile
(@typespec/compiler 1.10.0). If absent, run `pnpm install --frozen-lockfile` there.
No new dependency installation or Rust build is required. `cargo metadata
--no-deps --offline` resolves package identities and source entrypoints. The
toolchain's lab location is migration debt in A1; update this resolver when moved.

## Authority and scope

- Keys identify actual Cargo **package** names. Module/symbol resolution is not
  implemented. Registry scope text bounds the capability being staged.
- Each first-party Cargo manifest under `games/` must be classified. Discovery
  skips dot directories, `target`, `node_modules`, and third-party `vendor`.
  Symlinked directories are not traversed. Third-party dependency auditing remains Q6.
- Existing entries check manifest/package identity, Cargo source entrypoints,
  repository-local evidence files, task IDs, and direct library/app destinations.
- Stages 0/1 may describe a future package without a manifest, visibly PROPOSED.
- Stages 2.7+ require evidence. Stage 4 requires the manifest at the declared
  destination, with no unresolved split. Tests establish structural consistency;
  they do not evaluate whether written evidence warrants stage advancement.
- Stages are the project's TC39 adaptation: 0 strawperson, 1 proposal, 2 draft,
  2.7 testing, 3 candidate, 4 finished. No automatic advancement.
- Initial stage 3 records retain the bounded shared-library evidence from 109_tasks;
  stage 2.7 records retain remaining game/compatibility qualification. No stage 4
  record currently claims promotion.
- TypeSpec skill guidance supplied native value authoring. Constant lookup uses
  the same pinned checker seam as the existing boundary generator; JSON value
  conversion uses compiler `serializeValueAsJson`. No hand-written type lowering.
- The loader imports the authoritative model and checks assignability independently
  of the authored annotation. Removing the annotation cannot bypass the schema.
- TypeSpec 1.10 accepts duplicate object keys. The registry and each entry must be
  inline object literals; repeated keys and spreads are rejected before serialization.
  Referenced record constants are explicitly unsupported to keep this guard complete.
- Task IDs come from ID/State tables in the newest numbered task ledger and two
  predecessors. Unrelated tables and combined strings such as `A2, A3` do not qualify.
- JSON, D2 and SVG freshness checks are read-only. SVG is temporarily rerendered
  with D2/ELK and compared byte-for-byte; `just map` updates the committed SVG.
  `map-watch` watches D2; TSP edits require `just map` or `just tsp` explicitly.

Change stage/scope/evidence in the same commit as the corresponding implementation.
The generated inventory appears above historical milestones; older green DONE
nodes mean scoped lab evidence, whereas numbered stage colors mean promotion state.

## Session flow: commits are the progress unit

Each commit records a bounded change, verification and remaining conditions.
A task may span commits; a stage may span commits. Commit count measures recorded
increments, never an automatic promotion threshold.

1. Read the games rules and current ledger plus its two predecessors. Run
   `just status`. Select the task ID, package and scope.
2. Send the coordinator/human the current stage, intended change, affected paths,
   checks and stopping condition before implementation.
3. Implement that increment. Report intermediate results while working. On a
   scope change or failed assumption, explain current state and yield for direction.
4. Run relevant implementation tests. Record commands, outcomes and remaining
   gaps in tracked evidence or the task ledger. Visible changes require an
   inspected MP4. Registry checks alone establish structural consistency.
5. Assess the stage exit below. Update TSP scope, paths and evidence with the
   implementation. Advance stage only when its scoped exit conditions are met.
6. Run `just map` and `just test` from `games/`. Commit only this increment's source,
   tests, lockfile changes, registry, generated outputs and tracked evidence.
7. Send commit SHA, stage before/after, checks and next bounded increment.
   Stop at the assigned terminal condition.

The commit groups implementation and evidence. Git owns the ordered change history;
TSP owns current classification. A commit cannot contain its own final SHA:
identify task/package in its message and supply its SHA in the post-commit update.
Later receipts may cite earlier commits. No second authored commit registry.

### Stage exits in this project's adaptation

| Transition | Evidence required in the advancing commit |
| --- | --- |
| 0 -> 1 | Named problem, bounded scope, destination, task and terminal condition |
| 1 -> 2 | Concrete implementation/API, lifetime/storage description and consuming path |
| 2 -> 2.7 | Executable tests/fixtures for scoped behavior, provenance and listed gaps |
| 2.7 -> 3 | Declared qualification matrix passes through real consumers, including restore/target checks required by the scope |
| 3 -> 4 | Scoped acceptance complete at declared destination, unresolved splits resolved, reproducible verification and ownership documented |

These are process gates. The checker enforces schema, references, evidence-file
presence and destination rules; it does not evaluate test results or promote code.
Explain skipped stages using existing evidence. Expanded scope requires an explicit
stage/scope reassessment. A file move alone does not change maturity.

### Actual committed increments

| Commit | Change | Maturity treatment |
| --- | --- | --- |
| `639083d` | Statechart composition, clone restore and GGRS qualification | Predates registry; evidence for bounded qualification, with Serde caveat |
| `0f9e183` | Negative regression detects restored statechart reinitialization | Predates registry; additional evidence with limitation retained |
| `1707594` | Snapshot-owned buffer consumed by Falcon, restore and SQL checks | Predates registry; integrated consumer evidence |
| `b1c2046` | Four libraries moved with tests/lockfiles and consumers | Actual registry 3 -> 3; location changes, stage retained |

### Worked promotion sequence for the current slice

Each row is a bounded commit unit; additional intermediate commits are permitted.

| Unit | Stage treatment | Required result |
| --- | --- | --- |
| Current Falcon move | App proposal 1 -> 2.7 using the existing qualified implementation | Lab/web import app-owned Falcon; core and WASM checks pass; executable still pending |
| Pending offline ingest | New shared package goes through draft/testing | Character-selectable API tested with retained Falcon and another-character fixtures; unsupported payload/behavior reported |
| Pending app executable | Assess 2.7 -> 3 for the declared slice | Existing simulation, shared capture and baked content composed; replay checks and inspected actual-state MP4 |
| Pending scoped finish | Assess 3 -> 4 | Recorded slice acceptance closed, destination ownership resolved, repeatable commands verified |

Full PM Falcon, later fighters and expanded move coverage retain separate task
conditions. A scoped stage-4 slice does not complete those tasks. Keep ingestion
outside simulation ticks throughout this sequence.

### Commit body and retrieval

Use the standard attribution footer after these factual fields:

```text
refactor(games): move Falcon policy into smash

Task: A1
Package: smash
Stage: 1 -> 2.7 (existing qualified implementation moved into app)
Property: lab and web consume app-owned Falcon state and transitions
Verified: <commands and outcomes actually observed>
Evidence: games/3_tasks.md
Remaining: executable, generic ingestion, expanded movement/attacks
```

From the repository root:

```sh
git log --oneline -- games
git log --format='%h %s%n%b' --grep='Task: A1'
git log -p -- games/classification/1_registry.tsp
git show b1c2046 -- games/classification/1_registry.tsp games/3_tasks.md
```

Count completed increments by scoped commits. Read their property, verification
and remaining conditions to determine progress. Failed/interrupted checks remain
explicit; they cannot serve as passing stage-exit evidence.
