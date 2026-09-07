# Adversarial rollback and replay audit

No tests were run. This report turns the existing guarantees and unknowns into
failure probes.

| Risk | State that must be saved or reconstructed | adversarial assertion |
| --- | --- | --- |
| Controller | character pose/velocity, grounded/slope flags if gameplay uses them, controller config, contact ordering, one-way/platform state | Restore frame F, perturb host presentation, replay F..N, and require identical movement/contact hashes. Reinsert colliders in a different order and require either documented equality or a recorded incompatibility. |
| Rapier dynamics/caches | all durable body/collider/island/broad/narrow/joint/CCD structures; app hook/event state | Serialize the documented wrapper, omit only scratch pipelines, rebuild them, and compare N steps. Flip each included field class once and require checksum divergence. |
| Animation | clip ID, integer phase, loop/direction, blend weights/timers, root accumulator, IK state, event cursor | Sample forward, restore backward, cross loop boundaries, reverse direction, and resimulate. Require exact local/model matrices and exactly-once confirmed events. Compare fresh versus restored `SamplingContext`. |
| RNG | algorithm, complete internal state, stream ID, draw count | Insert a presentation-only random draw and require unchanged simulation. Delete one simulation draw and require immediate checksum divergence. |
| Ordering | entity IDs, iteration order, callback/event order, spawn/despawn sequencing | Randomize container capacity and host traversal order while holding canonical simulation order fixed; hashes must match. Deliberately shuffle canonical order and require detection. |
| Rollback side effects | confirmed-frame cursor, speculative event ledger, audio/particle dedupe IDs | Roll back across attack/hit/death frames repeatedly. Simulation events may regenerate; each confirmed presentation event is emitted once. |
| Snapshot aliasing | every GGRS save slot and recycled buffer generation | Mutate current state after every save and assert all saved checksums/bytes remain unchanged. Hold maximum prediction-window saves while recycling presentation/SQL slots. |
| Cross-host leakage | host clocks, transforms, input repeat/focus events, GPU/Godot handles | Replay identical normalized inputs in Godot and winit/wgpu with different render rates; per-tick core snapshots remain byte-identical. |

GGRS `SaveGameState`/`LoadGameState`/`AdvanceFrame` ordering and
`SyncTestSession` provide the harness described in
[3_rollback.md](3_rollback.md). Rapier's documented determinism conditions in
[1_collision.md](1_collision.md) mean cross-machine success cannot be inferred
from a same-process replay. Ozz's cross-platform statement is upstream-reported
and requires the architecture matrix in [6_compatibility.md](6_compatibility.md).

## Proposed test matrix

- 10,000 headless ticks with fixed inputs and seed; checkpoint every tick;
  restore every checkpoint at least once; compare canonical bytes and checksum.
- Two GGRS peers with delay, loss, duplication, and reordering; inject one byte
  corruption and require a desync event at a known frame.
- x86_64 and ARM64 release builds with identical dependency/compiler pins;
  compare state hashes, collision events, root motion, and model matrices.
- Godot at 30/60/144 redraw Hz and winit/wgpu at an independent cadence;
  compare normalized inputs and core hashes. Record MP4 only for visible output.
- Allocation counter after warmup around save/load/advance, collision step,
  animation sample, SQL publish/query, and both uploads.

All entries are **proposed**. No replay, allocation audit, cross-host run, or
recording was executed.
