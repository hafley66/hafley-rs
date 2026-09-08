# Game development incubation workspace

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
