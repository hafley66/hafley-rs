# Games: shared foundations, labs, and Smash

## Approved destinations and living roadmap

The main game application belongs in `smash/`, one app crate with no nested
crates. All first-party reusable game crates belong in `crates/`. Existing
`shared/` and lab packages are migration sources; move them with tests, provenance,
lockfile/dependency updates and consumer verification. Never duplicate them to
populate the destination. Third-party submodules retain their upstream layout.

## Direct file trees and colocation

Use explicit, direct paths. Each directory segment must identify a distinct
ownership or domain boundary. Do not repeat intermediate concepts or mirror the
same domain tree under source, content, assets, data, or generated roots.

Keep a fighter's authored behavior, configuration, imported/generated data and
local tests together in one fighter directory. For example,
`smash/src/fighters/falcon/` owns Falcon; do not also create
`smash/content/fighters/falcon/`. A local `generated/` directory is permitted when
it separates machine-owned output from authored files without duplicating the
fighter hierarchy. Apply the same colocation rule to stages, items and vehicles.

Create folders when code or data actually occupies them. Do not scaffold empty
category trees, redundant wrapper modules, or directory levels that merely repeat
their parent. Number files within their local module's dependency/reading order;
do not encode the full parent path again in filenames.

Falcon-specific behavior belongs in the app's fighter directory. Shared crates
must remain character-independent. Offline ingestion selects a character and
writes into that character's existing home using a common package format; do not
create a second maintained implementation or parallel per-character output tree.

Lab is a maturity stage. Classify lab code by promotion destination independently
of proof status:

- `library` -> `crates/`: reusable implementation, adapters, tooling and their tests.
- `app` -> `smash/`: game-specific composition, behavior/content selection, scenes
  and integration scenarios.
- `mixed`: name the library and app portions and their respective destinations.

All lab code is eligible for later lifting. Do not assume experimental code is
disposable or that everything validated must become a library. Preserve fixtures,
recordings and provenance with links from promoted consumers. A passing lab may
still be pending promotion. Record both destination and maturity in tasks and map
nodes; use text for destination and the existing colors for maturity. Destination
classification alone does not authorize a move or change a queued task's status.

Crate classifications are authored in `classification/1_registry.tsp`, checked
against `classification/0_model.tsp`. Use TC39-style stages 0/1/2/2.7/3/4 independent
of destination. Run `just status` for checked references, `just map` to regenerate,
and `just test` for registry tests/freshness. Update stage, scope and evidence in
the implementation commit. Every first-party Cargo package in games must be
classified; future packages are explicit proposals. Module-level resolution is
not implemented. Generated JSON/D2 are outputs, never separate authored registries.

Maintain `1_roadmap.d2` and its rendered SVG with architecture changes. Add new
structural milestone groups above older groups, preserving stable node IDs and
historical provenance. Group experiments beneath the capability they qualify.
Use explicit status labels plus the existing classes; a source audit, a passing
lab and a shipped game capability are distinct states. Evidence and limits live
in `2_roadmap.md`. Run `just map` and `just test-map` from this directory.

Read the newest numbered `*_tasks.md` plus N=2 predecessors when available.
Carry unfinished IDs forward with explicit disposition; never erase unfinished
work by starting another task file. The domain ledger starts at `3_tasks.md` and
links the earlier Falcon ledger. Every task needs a terminal condition and a
coordinator checkpoint. Send a progress message before implementation and when
an intermediate result or changed approach needs review. Subagents must send
these messages to their parent while working; yield for direction on scope
changes, missing authority or ambiguous behavior. Do not wait for final completion.

Distill v1 props/items/Lovers ship and v3 engine/destruction/input behavior from
their actual sources. v2 language experiments remain provenance. Rust v4 and
retired document/DOM architecture remain abandoned. Do not revive old apps.

Queue domain-library labs in the roadmap before inventing overlapping machinery.
Start with ssbm_utils fidelity and bone-driven gameplay geometry. Record exact
version/API/license, target ruleset, fixture, assertions, native/WASM scope and
integration destination. Existing use of a formula does not establish PM fidelity.
Rust, GDScript and JS/TS tools may qualify; other-language offline tools need an
explicit scoped reason. Library discovery is open-ended; each individual lab is
bounded and must report its result rather than expand itself indefinitely.

Rollback state includes every mutable cause of future gameplay, including input
history, action/animation clocks, item ownership, modifiers, destruction, RNG and
required physics state. Immutable content is shared and identified by version/hash.
Derived caches may be rebuilt only when replay equivalence is demonstrated.
GPU resources and presentation-only animation remain outside snapshots. Publish
presentation/SQLite state after catch-up, rather than requiring publication for
every replayed tick. Reducers coordinate state; asset blobs need no Redux actions.

This directory holds game-development labs: Rust experiments, research, fixtures,
asset tooling, engine adapters, and reproducible recordings.

Incubate here. Once a component is validated and a production destination is
chosen, move its source, tests, required fixtures, dependency pins, and provenance
into that proper crate or project. Record the destination in the lab and update
consumers. Avoid maintaining duplicate implementations after extraction. Ask for
the destination if it is unspecified; do not invent a production architecture.

Keep experimental Cargo packages and lockfiles isolated from the parent workspace
until promotion is explicitly requested. Retain source assets and evidence; ignore
build caches and regenerable intermediate captures in Git.

Commonize shell automation within each lab. In the Falcon lab, use its justfile
for workflow entry points and source `0_shell.sh` after setting `lab_dir` for
Cargo policy, temporary artifact directories, encoding, and media inspection.
Keep experiment-specific assertions and capture parameters in their scripts.

If a Rust build is interrupted by SIGTERM, clean the affected lab's validated
Cargo target directory and retry a clean build before reporting verification
blocked. Keep cleanup scoped to that lab's generated artifacts and use at most
two compile jobs. If the clean rebuild is also interrupted, report that result
instead of repeatedly cleaning and rebuilding.

Before implementing a subsystem, inspect local vendors and domain-specific
libraries for overlap. Record exact APIs, dependencies, licenses, and gaps.
Start fighter-related work with
`blender-godot-sqlite-proof/research/10_smash_ecosystem.md`.

Visible experiments require reproducible H.264 MP4 output from the implementation
under test plus deterministic assertions. Inspect frames and distinguish executed
results from proposals. Preserve the engine-independent Rust simulation and the
user's SQLite boundary. State remaining untested integrations explicitly.

Keep visual proofs incremental and preserve earlier MP4s. On-screen labels must
report actual state: action/pose, tick, inputs, prediction/confirmation, damage,
and rollback activity where applicable. Label slowed playback and presentation
holds separately from simulation time. Derive labels from executed trace data.

## Rust tracing standard

Use `tracing` for structured Rust diagnostics and spans at execution/effect
boundaries: simulation steps, snapshot save/load, rollback replay, publication,
SQL reads, engine transfer, and other measured work. Record scalar identifiers
and counts such as tick, peer, input bits, generation, rows, and replay count.
Use `skip_all` on instrumented functions and explicitly select fields. Never
Debug-format worlds, assets, geometry, or snapshots into telemetry.

Reusable libraries emit spans/events and never install a global subscriber.
Executables and engine adapters own subscriber/filter/output setup, respect an
existing host subscriber, and propagate tracing context into spawned workers.
Keep per-tick diagnostics at debug/trace, with runtime filtering; do not require
per-tick logging during ordinary captures or benchmarks. Keep span guards scoped
to synchronous work and out of sleeps, waits, and GPU synchronization unless
the span explicitly measures that wait. Never hold an entered guard across await.

Existing machine-consumed stdout protocols may retain their markers until their
consumers migrate. New diagnostics use structured tracing rather than ad hoc
prints. Test emitted fields and span relationships. Performance claims must
separate tracing-enabled overhead from the workload; tracing alone does not
measure allocation counts, allocated bytes, RSS, or memory bandwidth.
