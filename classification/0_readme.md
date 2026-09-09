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
