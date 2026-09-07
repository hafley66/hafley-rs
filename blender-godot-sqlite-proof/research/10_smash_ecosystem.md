# Fighter-domain dependency inventory

Research date: 2026-09-07. Scope: Rust platform-fighter and fighting-game tooling, Smash asset pipelines, frame data, and replay/SQL tooling. Source inspection only for the candidates below. Their builds, fixtures, and integration behavior have not been reproduced in this lab.

## 0. Before implementing a subsystem

Search the domain vocabulary and local vendors first. Record the candidate's exact API/source, packaging, dependency coupling, license, input requirements, and remaining overlap. Inspect published crates separately from application internals and planned features. An unsuccessful search leaves an unresolved question; it does not establish that a library does not exist.

Preserve the requested engine-independent Rust simulation and SQLite boundary while evaluating candidates. A framework discovery does not authorize replacing the foundation. For a selected candidate, pin its revision and run an isolated fixture before integration. Visible experiments retain the lab's reproducible MP4 plus assertions workflow.

## 1. Capability matrix

| Subsystem | Existing implementation | Packaging and constraints |
| --- | --- | --- |
| Brawl / Project M fighter extraction | [brawllib_rs](https://github.com/rukai/brawllib_rs): MDL0 bones, CHR0 animation, script evaluation, per-frame collision data | Rust library, MIT. Local 0.29.0 manifest includes mandatory wgpu/winit dependencies. Requires source game/mod files. |
| Animated frame-data inspection | [rukaidata](https://github.com/rukai/rukaidata), using brawllib_rs | Rust tooling and website pipeline. Native GIF generation and wasm/wgpu display already exist. |
| glTF animation to hurt volumes | [canon_collision generate_hurtboxes](https://github.com/rukai/canon_collision/blob/master/generate_hurtboxes/src/main.rs) | Rust application utility. Evaluates animated joints and generates per-frame 2D circles along bones. Integration/source extraction still needs examination. |
| Fighter/stage editor and action schema | [PF_Sandbox](https://github.com/rukai/PF_Sandbox), [pf_sandbox_lib 0.3.1](https://docs.rs/pf_sandbox_lib/0.3.1/pf_sandbox_lib/all.html) | Platform-fighter application and library, GPL-3.0. README identifies macOS limitations. |
| Fighter runtime/editor implementation | [canon_collision](https://github.com/rukai/canon_collision) | MIT application/workspace. Its library manifest includes window/input dependencies; simulation isolation requires inspection. |
| Melee models and animations to Blender/GLB | [melee-blender](https://github.com/visgotti/melee-blender) | Offline Python/Blender plus mexTool/HSDLib conversion workflow. Documented Windows entry point. License unresolved in this audit. |
| Melee Blender import helpers | [melee-tools](https://github.com/shiggl/melee-tools), [HSDLib](https://github.com/Ploaj/HSDLib) | Blender tooling and C# conversion tooling. Separate format family from Brawl MDL0/CHR0. |
| Fighting-game command input, combat, state execution | [FightersParadise](https://github.com/fakoli/FightersParadise) | MIT Rust MUGEN workspace: fp-input, fp-combat, fp-vm, fp-character, fp-engine. APIs and dependency isolation require fixture validation. |
| MUGEN asset parsing | [rugen](https://github.com/reu/rugen) | Rust workspace with mugen-air, mugen-def, mugen-sff, mugen-snd. License unresolved in this audit. |
| Melee replay decoding | [peppi 2.1.2](https://docs.rs/peppi/2.1.2/peppi/), [peppi-slp](https://github.com/hohav/peppi-slp) | Rust parser and CLI. Replay telemetry and columnar data can supply test observations; exact skeletal pose availability remains unverified. |
| Melee equations, constants, state tracking | [ssbm_utils 0.4.0](https://docs.rs/ssbm_utils/0.4.0/ssbm_utils/) | Rust calc/checks/constants/enums/trackers modules. Individual formulas and fidelity need source-level tests. |
| Melee identity/action enums | [ssbm-data](https://github.com/hohav/ssbm-data) | Rust domain data. Exact coverage/version compatibility requires checking against selected replay format. |
| Replay to SQLite | [slippi-db](https://github.com/mtimkovich/slippi-db) | Rust ingestion application using Peppi. Documents SQL examples. Hot recycled simulation-buffer semantics have not been established. |
| Ultimate model/skeleton/animation formats | [ssbh_lib and ssbh_data](https://github.com/ultimate-research/ssbh_lib) | Rust binary read/write and higher-level data libraries, with JSON utilities. Different input formats from Melee/Brawl. |
| Ultimate model rendering | [ssbh_wgpu](https://github.com/ScanMountGoat/ssbh_wgpu) | wgpu renderer, including skeletal-animation code. Exact reusable CPU animation surface remains to audit. |
| Blender/Python access to Ultimate formats | [ssbh_data_py](https://github.com/ScanMountGoat/ssbh_data_py) | Python bindings to Rust data library; mesh, skeleton, animation and material read/write. |
| Ultimate material SQL research | [Smush-Material-Research](https://github.com/ScanMountGoat/Smush-Material-Research) | Documents an SQLite material database generated by companion Smush-Material-DB. Rendering-data use case; hot simulation integration untested. |

## 2. Existing local code

These directories were present when inspected:

- `/Users/chrishafley/projects/games/smash/vendor/brawllib_rs`
- `/Users/chrishafley/projects/games/smash/vendor/rukaidata`

Local brawllib_rs `src/high_level_fighter.rs` exposes:

| Symbol | Observed source line |
| --- | ---: |
| HighLevelFighter | 20 |
| HighLevelFighter::new | 37 |
| BoneTransforms | 612 |
| HighLevelSubaction | 631 |
| HighLevelFrame | 716 |
| HighLevelHurtBox | 848 |
| HighLevelHitBox | 1095 |
| ECB | 1109 |

Lines describe the inspected local checkout and may move. Local Cargo.toml reports version 0.29.0, with wgpu 28, winit 0.30, cgmath 0.18, serde, rayon, and gif dependencies.

Upstream documented entry points are `BrawlMod::new`, `load_fighters(false)`, and `HighLevelFighter::new`. Inspect `HighLevelFighter.subactions` and each subaction's `frames` for extracted frame data. This API has not been exercised against a game dump here. [Library source](https://github.com/rukai/brawllib_rs/blob/main/src/lib.rs).

The [Rukaidata pipeline writeup](https://github.com/rukai/rukaidata/blob/main/docs/writeup.md) describes costume skeletons, per-bone animations, and fighter script evaluation feeding frame data. It also documents native animated GIF generation. Script execution includes approximations and loop handling, so extracted results require comparison with the target game.

## 3. Bone-driven collision baking already implemented upstream

In canon_collision's `generate_hurtboxes/src/main.rs`, the observed sequence is:

1. `Model3D::from_gltf` loads the model.
2. `regenerate_action` visits action animation frames.
3. `animation::set_animated_joints` evaluates joints.
4. Configured bone attachments feed `generate_hurtbox`.
5. The generator projects transformed positions onto `(z, y)` and places circles along bone length.

Source contains a radius-scaling TODO and a TODO about relocating duplicate animation/hurtbox code into the library. Circle generation, frame replacement, and animation timing must be checked against the desired 3D capsule representation. The source also handles an item-hold attachment using `Hand.R`. [Implementation](https://github.com/rukai/canon_collision/blob/master/generate_hurtboxes/src/main.rs).

PF Sandbox's published schema includes `ActionDef`, `ActionFrame`, `CollisionBox`, `HitBox`, `HurtBox`, `ECB`, `LCancel`, `Shield`, and `Tech`. Presence in a schema establishes implementation vocabulary; adoption still requires inspecting behavior and licensing. [API index](https://docs.rs/pf_sandbox_lib/0.3.1/pf_sandbox_lib/all.html).

## 4. Other fighting-game runtime code

FightersParadise separates format parsing, input recognition, combat, expression execution, character state, and match execution into workspace crates. Source inspection of fp-input's manifest found fp-core, tracing, and serde dependencies. Its serialization comments distinguish static compiled commands from runtime input buffers/timers. The README describes `HitDef`, `resolve_hit`, and `resolve_clash` in fp-combat. These remain candidate APIs until compiled against pinned source. Upstream completion/test-count claims were not reproduced. [Workspace](https://github.com/fakoli/FightersParadise).

Secondary leads, with readiness limits:

- [Quad-Fighter-2](https://github.com/ValorZard/Quad-Fighter-2): Macroquad/GGRS/fixed-point physics application example. Standalone reusable combat APIs not established.
- [Castagne](https://castagneengine.com/): existing Godot fighting-game tooling; site describes forthcoming Rust work. Rust package availability/API remains unresolved.

## 5. Source and release observations

GitHub repository metadata queried on the research date. `pushed_at` is repository activity metadata, not a stability assessment. Empty GitHub releases results do not imply absence of crate releases or tags.

| Repository | Default branch | pushed_at date | License reported by GitHub | GitHub releases query |
| --- | --- | --- | --- | --- |
| rukai/brawllib_rs | main | 2025-12-18 | MIT | Empty |
| rukai/rukaidata | main | 2025-10-12 | MIT | Latest v0.1.2, published 2022-03-17 |
| rukai/PF_Sandbox | master | 2021-04-22 | GPL-3.0 | Empty |
| rukai/canon_collision | master | 2025-04-27 | MIT | Empty |
| visgotti/melee-blender | master | 2026-02-27 | Unresolved | Empty |
| fakoli/FightersParadise | main | 2026-06-18 | MIT | Empty |
| reu/rugen | master | 2026-05-31 | Unresolved | Empty |
| hohav/peppi | main | 2026-06-08 | MIT | Empty |

License entries are discovery metadata. Review actual licenses, transitive components, and asset permissions before redistribution. Existing game assets are separate inputs from tool source code.

## 6. Remaining verification

- Pin candidate source revisions before running integrations; this inventory links moving branches where shown.
- Reproduce Brawl/Project M extraction from supplied source assets and compare selected frames with in-game captures.
- Validate Melee-to-Blender conversion on macOS and Blender 5.2; upstream documented workflow includes Windows tooling.
- Inspect fp-combat/fp-character/fp-engine public APIs, dependencies, snapshot completeness, and test coverage directly.
- Test bone evaluation and capsule attachment under rollback, including interpolation, root motion, animation speed, and action transitions.
- Test cross-platform determinism independently from local restore/replay equality.
- Inspect remaining licenses and current issue trackers. A systematic issues/changelog audit is unfinished.
- Inspect additional fighter-specific libraries for input buffering, command recognition, hit resolution, ledges, and action timelines. Search coverage is incomplete.
- Exercise selected libraries against the same fixtures in the Rust core and presentation adapters, retaining MP4 evidence and machine assertions.

No newly inventoried dependency was installed or integrated by this research pass. Existing lab results remain described in `../18_results.md`.
