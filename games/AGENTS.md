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
