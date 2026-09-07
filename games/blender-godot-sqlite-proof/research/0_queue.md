# Sequential Luna research queue

User authorized this queue on 2026-09-07. Fresh gpt-5.6-luna agents,
max reasoning, strictly one research shot at a time. Research only.
The coordinator starts the next shot only after the preceding shot finishes.
No implementations, installs, commits, or changes outside this research folder.

## Shared brief

Research engine-independent Rust libraries for a reusable platform-fighter
simulation with rollback. The same Rust foundation feeds multiple presentation
engines. Exclude Tnua and Bevy-specific solutions. Godot is an editor, presentation,
and recording host. SQLite may expose bounded recycled simulation buffers.
Use official docs, source, manifests, tests and issues. Distinguish documented
guarantees, source observations, tested results and inference. Do not claim tests
were run. Assess maintenance with dated evidence, API stability, target support,
known defects and dependency constraints. Avoid subjective build-vs-buy claims.

## Ordered shots

1. A -> 1_collision.md: Parry/Rapier and engine-independent downstream collision,
   movement and physics libraries. APIs, ownership, allocation/reuse, snapshots,
   determinism. Separate queries, controllers and dynamics.
2. B -> 2_animation.md: Rust skeletal animation independent of engine; include
   ozz-animation-rs. Formats, sampling/blending/hierarchies/root motion, reuse,
   rollback state, determinism evidence, external converter requirements.
3. C -> 3_rollback.md: GGRS and verified engine-independent alternatives. Exact
   save/load/advance contract, application state, transport, desync/replay tests.
4. D -> 4_sql_buffer.md: SQLite-owned bounded rows versus virtual tables over
   Rust buffers; existing bindings/adapters, cursor lifetimes, copies, allocation,
   reuse, transactions, concurrency, rollback visibility. Secondary indexes
   versus underlying storage. No a priori exclusion of SQL from hot simulation.
5. E -> 5_hosts.md: Read A/B only. Existing Rust integration libraries for Godot
   and a second non-Bevy presentation host; bulk transforms, skeleton poses,
   assets, input, recording. Same core owns simulation timing in both hosts.
6. F -> 6_compatibility.md: Read A/B/C only. Verify shortlist versions/features,
   math types, precision, platforms, serialization, licenses and determinism;
   identify actual required conversions/adapters.
7. G -> 7_replay_audit.md: Read A/B/C/F claim tables. Challenge snapshot and
   determinism claims: controllers, animation, caches, RNG, order, callbacks.
8. H -> 8_storage_audit.md: Read D. Challenge recycled/allocation-free claims
   against actual APIs/source. Identify hidden copies and allocations.

## Per-shot contract

At most four qualified candidates for discovery shots, plus brief exclusions.
For claims: exact version/commit, URL and symbol/path, evidence category,
engine coupling, application-owned state, constraints and unknowns.
Propose one runnable experiment: inputs, assertions, measurements, and an MP4
when visible behavior applies. Report unknowns rather than extending scope.
Use apply_patch for reports. Fresh agents read this brief and only named inputs.
No shot spawns further agents. Keep reports compact enough for subsequent shots.

## Completion

Coordinator maintains 0_status.md with queued/running/completed/blocked states
and actual agent identifiers. Send root an update after each shot. Finish with
9_synthesis.md: sourced shortlist, unresolved compatibility, proposed boundary
type signatures, lifetimes/ownership, read/write/reuse rules, experiment backlog.
No automatic implementation. If interrupted, resume from saved reports/status.
