# Falcon locomotion delivery

Predecessors: `3_tasks.md`, `blender-godot-sqlite-proof/falcon-lab/109_tasks.md`.
All unfinished A/Q/M and G/P tasks retain their prior disposition. This pass
executes the locomotion portion of A2 and the required G2 asset expansion.

| ID | State | Terminal condition / coordinator checkpoint |
| --- | --- | --- |
| F1 | Active | Source-backed Melee Falcon movement constants and callback rules recorded with PM animation provenance separate. Worker reports first evidence before extending research. |
| F2 | Active | Offline locomotion action catalog preserves existing IDs, decodes retained dash/run/walk/turn/jump poses, records hashes and rejects missing data. One worker commit, then review. |
| F3 | Pending | Shared snapshot-owned fighter locomotion and input history consumed by Falcon Redux. Tests cover dash/run/reversal, release traction, short/full/double jump, landing and restore. Exact-source fidelity gaps recorded. |
| F4 | Pending | Live Godot/Web inputs show locomotion poses, facing and speeds; fixed 60 Hz. Browser input assertions, replay and inspected MP4 pass. Existing knee proof retained as historical regression. |
| F5 | Pending | Commit scoped changes, run full proof, publish verified artifact to /game3/, verify production and protected /game/. |

Scope ends at playable Falcon locomotion and its reusable mechanisms. Attacks
beyond existing fair, shields, grabs, ledges, items, multiplayer and terrain remain
the carried queue. Imported data is immutable; current phase, clocks, input edges,
velocity, facing and remaining jumps are snapshot-owned. Source decoding occurs
offline. Renderers receive derived state through the existing SQLite boundary.

Two bounded Boop workers: `chore-falcon-movement-evidence` owns F1 report;
`feature-falcon-locomotion-ingest` owns F2 catalog/import. Both start from 2d4fc0c,
use Z.ai GLM 5.3 with max requested, and explicitly hail codex-587 at checkpoints.
OpenRouter GLM 5.3 Flash max is the rate-limit fallback; Luna high is the harness
failure fallback. Coordinator owns review, integration, tests and deployment.
