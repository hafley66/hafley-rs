# Skeletal animation research, shot B

As of 2026-09-07. Source, manifest, documentation, and issue review. No tests
or experiments were run in this workspace. The proposed experiment was not run.
No visible experiment was run, so no MP4 exists.

Evidence labels:

- **[DG] documented guarantee:** package or API documentation.
- **[SO] source observation:** manifest, source path, README, release record, or
  package contents.
- **[TR] upstream-reported result:** maintainer or issue report; not reproduced.
- **[IN] inference:** consequence of an API boundary or ownership model.
- **[UK] unknown:** requires a pinned build, source audit, or experiment.

## Qualified candidates

| Candidate pin | Runtime and formats | Coupling, state, and reuse |
| --- | --- | --- |
| [ozz-animation-rs 0.11.0](https://github.com/SlimeYummy/ozz-animation-rs/tree/ece37e90f2df7ba2747f4886f420a377897bb52c), tag commit ece37e90f2df7ba2747f4886f420a377897bb52c; current master ea9f1d92cbe7b3d5179208335cd25088be7e0f2d | Sampling, hierarchy expansion, layer/partial/additive blending, motion blending/root motion, IK, skinning, tracks, archive serialization. Runtime consumes .ozz; C++ ozz tools convert source assets. | No renderer or engine dependency in the runtime manifest. Jobs use caller buffers and contexts. Application owns time, clip/blend state, root-motion application, IK targets, event cursors, and rollback state. README determinism claim and CI results are [TR]. |
| [skeletal_animation 0.47.0](https://docs.rs/skeletal_animation/0.47.0/skeletal_animation/), source HEAD c8938b127fe2a89657a23544b270ad5ddba2a0c0; release commit [UK] | Collada skeletons/clips, JSON controller definitions, blend trees, transitions, matrix or dual-quaternion output. | Manifest directly depends on gfx, gfx_debug_draw, piston-gfx_texture, and collada. Controller receives caller time/output but retains private state and is !Send/!Sync. Global-pose output can be reused; skinning helper allocates a Vec. No snapshot or determinism contract found [UK]. |
| [animgraph 0.2.0-dev](https://github.com/animgraph/animgraph/tree/f2f8fb5a224a5dcdc7bed8aceeaf87c9426548ae), source HEAD f2f8fb5a224a5dcdc7bed8aceeaf87c9426548ae; published docs 0.1.0; release commit [UK] | Animation clips, skeleton/bone groups, blend trees, hierarchical state machines, transitions, events, graph interpreter. Importer, mesh skinning, and root-motion channel [UK]. | Current manifest has no renderer or ECS dependency. Application owns assets, timing, pose conversion, mesh upload, and host integration. Runtime snapshot fields, allocation behavior, and cross-host determinism [UK]. |
| [symbios-avatar 0.3.1](https://github.com/TheJanusStream/symbios-avatar/tree/4e1ed3c5a27f82307ec40e90d1a308c37295954d), source HEAD 4e1ed3c5a27f82307ec40e90d1a308c37295954d; release tag/commit [UK] | Procedural avatar rig, fixed-rate pose clips, custom glTF animation sampling/retargeting, two-bone IK, FABRIK, look-at, springs, and mesh deformation. PoseClip stores root motion per frame. | Core has no Bevy dependency; a separate Bevy adapter exists. Pose application writes into caller pose storage. deform_linear returns a Vec. Rollback state and bitwise animation determinism [UK]. Cached docs expose mixed versions, so source/docs pairing [UK]. |

## ozz-animation-rs

The release [manifest](https://raw.githubusercontent.com/SlimeYummy/ozz-animation-rs/ece37e90f2df7ba2747f4886f420a377897bb52c/Cargo.toml)
reports Rust 2021, Rust 1.88, MPL-2.0, default rkyv/serde features, and
nightly toolchain use for portable SIMD [SO]. The current master head is
[ea9f1d9](https://github.com/SlimeYummy/ozz-animation-rs/commit/ea9f1d92cbe7b3d5179208335cd25088be7e0f2d);
post-tag changes [UK].

API symbols and source paths:

- [SamplingJob](https://docs.rs/ozz-animation-rs/0.11.0/ozz_animation_rs/sampling_job/struct.SamplingJob.html)
  and [src/sampling_job.rs](https://github.com/SlimeYummy/ozz-animation-rs/blob/ece37e90f2df7ba2747f4886f420a377897bb52c/src/sampling_job.rs)
  sample compressed animation at a clamped ratio. SamplingContext stores
  decompressed keyframes and forward-sampling cache values. Backward sampling
  works and is documented as less optimized. Input and output buffers are not
  owned by the job [DG].
- [src/local_to_model_job.rs](https://github.com/SlimeYummy/ozz-animation-rs/blob/ece37e90f2df7ba2747f4886f420a377897bb52c/src/local_to_model_job.rs)
  expands local SoA transforms through Skeleton hierarchy into caller matrices.
  BlendingJob and MotionBlendingJob in
  [src/blending_job.rs](https://github.com/SlimeYummy/ozz-animation-rs/blob/ece37e90f2df7ba2747f4886f420a377897bb52c/src/blending_job.rs)
  and [src/motion_blending_job.rs](https://github.com/SlimeYummy/ozz-animation-rs/blob/ece37e90f2df7ba2747f4886f420a377897bb52c/src/motion_blending_job.rs)
  cover layer and root-motion blending [DG].
- IKTwoBoneJob, IKAimJob, and SkinningJob are public runtime stages.
  TrackTriggeringJob detects float-track threshold crossings and returns a
  lazy iterator. Event handling remains application-owned [DG].
- SoaTransform is repr(C) with SIMD translation, rotation, and scale. The
  SamplingJobRef alias uses &Animation, &mut [SoaTransform], and
  &mut SamplingContext [DG].

Resource loading and archive reads allocate. The README example allocates one
sample array sized by skeleton.num_soa_joints() and one model-matrix array
sized by skeleton.num_joints(), then reuses them. OzzBuf and OzzMutBuf define
the buffer boundary [SO]. A zero-allocation guarantee for every blend, IK, or
skinning path is [UK].

Rollback must include the application animation id, fixed tick/phase, direction,
loop mode, blend weights/timings, root-motion accumulator, IK targets/weights,
and event cursor [IN]. The crate exposes archive/serialization types, including
archived animation, skeleton, and sampling-context types [SO]. A complete
runtime snapshot API is absent from the reviewed docs [UK]. SamplingContext is
described as decompression/cache state, so rebuilding it from restored resource
and tick inputs is a candidate procedure [IN].

The README calls the runtime cross-platform deterministic and suitable for
lock-step networking. It reports Windows, Ubuntu, and macOS x64 CI plus x64 and
Arm64 Docker tests [TR]. The tests are listed in the upstream
[test directory](https://github.com/SlimeYummy/ozz-animation-rs/tree/ece37e90f2df7ba2747f4886f420a377897bb52c/tests);
none were reproduced here. The implementation uses f32 and SSE2/NEON/WASM.
A formal bitwise cross-architecture contract [UK]. Open
[issue #126](https://github.com/SlimeYummy/ozz-animation-rs/issues/126) reports
a user's distorted GPU skinning output and requests a canonical example [TR];
the integration behavior [UK].

The Rust runtime does not include offline conversion. The official C++
[ozz-animation project](https://github.com/guillaumeblanc/ozz-animation) lists
gltf, FBX, Collada, OBJ, 3ds, and DXF conversion to .ozz runtime structures.
Its [release history](https://github.com/guillaumeblanc/ozz-animation/releases)
identifies gltf2ozz and fbx2ozz. Blender export settings, rest-pose and
coordinate conventions, mesh inverse-bind data, and converter/runtime pairing
require asset tests [UK].

## skeletal_animation

The package [manifest](https://raw.githubusercontent.com/PistonDevelopers/skeletal_animation/c8938b127fe2a89657a23544b270ad5ddba2a0c0/Cargo.toml)
at source HEAD lists gfx 0.18.1, gfx_debug_draw 0.33.0,
piston-gfx_texture 0.44.0, collada 0.15.0, vecmath, quaternion,
dual_quaternion, and interpolation [SO]. Its release commit is [UK].

The API surface is [Skeleton](https://docs.rs/skeletal_animation/0.47.0/skeletal_animation/skeleton/struct.Skeleton.html),
[AnimationController](https://docs.rs/skeletal_animation/0.47.0/skeletal_animation/controller/struct.AnimationController.html),
[AssetManager](https://docs.rs/skeletal_animation/0.47.0/skeletal_animation/manager/struct.AssetManager.html),
and [SkinnedRenderer](https://docs.rs/skeletal_animation/0.47.0/skeletal_animation/skinned_renderer/struct.SkinnedRenderer.html).
Skeleton imports Collada and writes global poses into caller output. The
controller accepts a definition, skeleton, and clip map, advances with
update(delta_time), and writes output with get_output_pose [DG]. The renderer
directly consumes gfx resources, textures, command buffers, render targets, and
matrices [DG]. This is a Piston/gfx presentation boundary [SO].

Application state includes elapsed time, parameters, asset ownership, output
pose/global-pose arrays, and renderer upload [IN]. Private controller state,
transition state, parameter values, and timing have no documented
serialization/snapshot API [UK]. AnimationController is !Send/!Sync [DG].
calculate_global_poses and get_output_pose accept reusable output slices;
SkinnedRenderer::calculate_skinning_transforms returns a fresh Vec and documents
a TODO to avoid that allocation [DG]. Determinism and rollback coverage [UK].

Collada is the documented source format. JSON describes clips, difference clips,
controllers, blend trees, and state machines. glTF/FBX conversion and a
presentation-independent skinning path [UK].

## animgraph

The current [manifest](https://raw.githubusercontent.com/animgraph/animgraph/f2f8fb5a224a5dcdc7bed8aceeaf87c9426548ae/Cargo.toml)
reports 0.2.0-dev, while [published docs](https://docs.rs/animgraph/0.1.0/animgraph/)
are 0.1.0. The README labels the project early development with incomplete
documentation [SO]. The current source release tag/commit [UK].

Symbols include AnimationClip, Skeleton, BoneGroup, BoneWeight, BlendTree,
Graph, Interpreter, InterpreterContext, GraphTime, GraphTransitionState, and
[DefaultRunContext](https://docs.rs/animgraph/0.1.0/animgraph/struct.DefaultRunContext.html).
The run context exposes new, clear, run, run_and_append, and run_without_blend;
documented fields include events, layer builder, blend tree, and delta time [DG].
README features include hierarchical state machines, transitions, blend trees,
parameters, events, custom nodes/resources, and serialized graph definitions
[DG].

The manifest contains serde/serde_json, glam, anyhow, thiserror, and optional
uuid, with no renderer or ECS dependency [SO]. The application supplies asset
loading, host timing, pose conversion, mesh deformation, and upload [IN]. Graph
definitions and compiled definitions can be serialized. Runtime transition
cursors, blend caches, event queues, and snapshot/restore behavior [UK].
Allocation behavior, root-motion output, importer support, and cross-host
determinism [UK]. glTF, FBX, and Collada conversion remain external or
application-owned [UK].

## symbios-avatar

The source [manifest](https://raw.githubusercontent.com/TheJanusStream/symbios-avatar/4e1ed3c5a27f82307ec40e90d1a308c37295954d/Cargo.toml)
reports 0.3.1, edition 2024, MIT, and dependencies including glam, noise, rand,
rand_pcg, serde, serde_json, and symbios-texture [SO]. The release tag/commit
[UK]. Cached docs pages expose mixed versions, including 0.1.0 and a 0.4.0
retargeting page, so matching docs to source [UK].

The [anim module](https://docs.rs/symbios-avatar/0.3.1/symbios_avatar/anim/)
contains Pose, PoseClip, Clip, two-bone IK, FABRIK, look-at, walking, and
springs. [PoseClip](https://docs.rs/symbios-avatar/0.3.1/symbios_avatar/anim/pose_clip/struct.PoseClip.html)
stores name, rate, frame count, looping, flat joint tracks, and per-frame root
motion. apply writes represented joints into a supplied pose. [Rig](https://docs.rs/symbios-avatar/0.3.1/symbios_avatar/rig/)
uses parent-before-child joint order. The [gltf module](https://docs.rs/symbios-avatar/0.3.1/symbios_avatar/gltf/)
reads skinned animation data. [retarget::clip](https://docs.rs/symbios-avatar/0.3.1/symbios_avatar/retarget/fn.clip.html)
bakes a source animation at a selected rate into a PoseClip [DG].

Pose owns local positions/rotations, and PoseClip::apply mutates it [DG].
Application owns GPU upload and host integration [IN]. deform_linear returns a
Vec [DG]. Root motion is explicit and can be accumulated or discarded during
rollback. Snapshot support for walk/spring state, IK targets, retargeting
caches, or procedural random/noise state [UK]. Documentation reports
deterministic avatar geometry from quantized records and seeded streams; that
evidence covers generated geometry, not cross-host animation playback [DG] [IN].
The custom glTF reader and fixed-rate retargeting path are documented. General
scene import, FBX import, material import, and DCC conversion [UK].

## Discovery and exclusions

- Bevy animation APIs and bevy_animation_graph were excluded by the brief:
  [AnimationGraph](https://docs.rs/bevy/latest/bevy/animation/graph/struct.AnimationGraph.html)
  and [bevy_animation_graph](https://github.com/mbrea-c/bevy_animation_graph).
- [soorat](https://github.com/MacCracken/soorat) contains animation inside a
  wgpu/AGNOS/Kiran rendering stack.
- [scml](https://github.com/spebern/scml) covers SCML 2D sprite animation.
- [skeletal-animation-system](https://github.com/chinedufn/skeletal-animation-system)
  is a JavaScript/npm system.
- [rust-animation](https://github.com/joone/rust-animation) is a wgpu scene
  graph experiment.
- [AnyMotion 0.1.1](https://docs.rs/crate/anymotion/0.1.1) has mandatory ECS and
  renderer dependencies, first-skin and linear-interpolation limits, a failed
  docs build report, and a documented blending stub. Its performance claims
  were not reproduced [TR]. It is excluded from the qualified set.
- The C++ [ozz-animation toolset](https://github.com/guillaumeblanc/ozz-animation)
  is retained as ozz conversion evidence, not as a Rust candidate.

## Cross-candidate findings

- Sampling/hierarchy: ozz provides SamplingContext and LocalToModelJob. Piston
  provides Skeleton::calculate_global_poses. Animgraph pose evaluation is
  revision-sensitive [UK]. Symbios provides fixed-rate PoseClip tracks and
  parent-before-child rigs.
- Blending/root motion: ozz provides BlendingJob and MotionBlendingJob. Piston
  and animgraph provide blend trees/controllers; separate root-motion output
  [UK]. Symbios stores root motion per baked frame.
- Application state: all four leave host timing and presentation upload to the
  application. Piston and animgraph retain controller/interpreter state; ozz
  exposes lower-level explicit job inputs; Symbios retains pose/procedural
  package state [IN].
- Allocation/reuse: ozz, Piston, and Symbios expose caller-owned pose buffers
  in selected paths. Piston skinning and Symbios linear deformation return Vec.
  Animgraph allocation behavior and ozz complete hot-loop allocation behavior
  [UK].
- Rollback: none documents a whole simulation snapshot containing application
  timing, blend/controller state, root accumulator, event cursor, IK, and
  procedural state. ozz's explicit job inputs and archived resource types leave
  the rollback boundary smallest by API shape [IN], with exact behavior [UK].
- Determinism: ozz has the only explicit lock-step/cross-platform statement and
  upstream platform reports [TR]. Cross-architecture bitwise equality [UK].

## Proposed runnable experiment

Target ozz-animation-rs 0.11.0 at commit
ece37e90f2df7ba2747f4886f420a377897bb52c, using the upstream playback inputs
[resource/playback/skeleton.ozz](https://github.com/SlimeYummy/ozz-animation-rs/blob/ece37e90f2df7ba2747f4886f420a377897bb52c/resource/playback/skeleton.ozz)
and [resource/playback/animation.ozz](https://github.com/SlimeYummy/ozz-animation-rs/blob/ece37e90f2df7ba2747f4886f420a377897bb52c/resource/playback/animation.ozz).
Pin the Git dependency by that commit and run:

    cargo +nightly test --release --test animation_probe -- --nocapture

Allocate once: SamplingContext::new(animation.num_tracks()), a
Vec<SoaTransform> sized to skeleton.num_soa_joints(), and a Vec<Mat4>
sized to skeleton.num_joints(). Reuse one SamplingJobRc and the output buffer
for ticks 0 through 9,999. Set ratio to (tick % 240) as f32 / 239.0; run
SamplingJobRc and LocalToModelJobRef.

Assertions:

1. Both files load and both output lengths match the skeleton counts.
2. Every job returns Ok(()) for all 10,000 ticks.
3. Save local/model outputs at tick 120, advance to tick 239, rerun tick 120,
   and require exact equality with the saved outputs.
4. Repeat the sequence with a fresh SamplingContext and compare checkpoints for
   exact equality.

Measurements: after 1,000 warmup ticks, record total time and nanoseconds per
tick for 10,000 ticks, output byte sizes, and allocation/deallocation counter
deltas from a test-local global allocator. The allocation delta is a measured
value because the reviewed API supplies no zero-allocation guarantee [UK].

This checks sampling, hierarchy expansion, buffer reuse, backward context use,
and same-process repeatability. Converter behavior, GPU skinning conventions,
Godot FFI, cross-architecture identity, and SQLite integration [UK]. The
experiment was not run.

## Preserved unknowns

- ozz bitwise equality across x64, ARM64, WASM, and host builds [UK].
- Minimal rollback record for ozz blend layers, motion blending, track cursors,
  IK, and SamplingContext rebuild [UK].
- Converter/runtime pairing, Blender export settings, rest-pose convention,
  coordinate conversion, and inverse-bind layout [UK].
- skeletal_animation controller serialization, deterministic advance, and
  renderer-independent reuse [UK].
- animgraph current-main runtime fields, allocation behavior, root-motion
  semantics, importer support, and snapshot [UK].
- symbios-avatar source/docs version pairing plus spring, walk, random/noise,
  and retargeting rollback state [UK].
- Whole-simulation snapshots including gameplay state, timing, event queues,
  pose state, and asset identity are absent from the reviewed contracts [UK].
