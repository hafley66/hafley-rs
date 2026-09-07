# PM Falcon knee, native Rust/wgpu fixture

Run from any directory with:

```sh
bash /Users/chrishafley/projects/hafley-rs/games/blender-godot-sqlite-proof/falcon-lab/3_run.sh
```

Requires Rust, ffmpeg/ffprobe, and a wgpu-compatible GPU. First build needs crate
downloads. Restricted shells may need native GPU permission on macOS.
For CPU-only verification, run `cargo test --locked` in this directory.

## Recorded behavior

`5_falcon_knee.mp4` shows 180 simulated ticks at 60 Hz, then repeats the same trace
at half speed: 540 encoded frames, 960x540, H.264, nine seconds total.

- Ticks 0–59: PM `Wait1` idle poses.
- Ticks 60–77: first 18 PM `JumpF` poses.
- Ticks 78–117: all 40 PM `AttackAirF` poses.
- Ticks 118–179: idle poses after fixture travel returns to the floor.
- Tick 91, knee animation frame 14: first overlap adds 18 damage to the target.
- Contact persists across 15 ticks; the single attack applies damage once.

The airborne target stays fixed. Orange is the active attack volume and hit
feedback. Purple geometry uses the PM hurtbox capsules transformed by their
extracted per-frame bone matrices. Capsule mesh generation comes from Parry;
font glyphs come from font8x8. The custom capture host rasterizes their wire edges
with wgpu and pipes actual GPU readback to ffmpeg.

## Inputs and dependency boundary

`../fixtures/falcon` contains the downloaded Rukai Data PM 3.6 pages and source
hashes. Base64/bincode payloads decode completely with published brawllib_rs
0.29.0: idle 61 frames, jump 36 frames, knee 40 frames. Cargo.lock pins registry
dependencies, including bincode 2.0.1, Parry 0.30.2, and capture wgpu 29.0.4.
Brawllib's own renderer dependencies are transitive, but this executable uses
brawllib's data types and its own wgpu capture host. No Godot or Bevy is invoked.

This renders collision volumes, not the downloaded costume mesh. The DAE is not
used in this fixture. Root travel is explicitly scripted, and the jump is
interrupted for the knee after 18 ticks. These choices do not establish exact PM
movement or action-transition rules.

Collision uses discrete 3D attack-sphere versus stationary cuboid queries, filtered
by enabled/aerial attack flags. Damage comes from the intersecting hitbox record.
Swept collision, PM priority rules, staling, knockback, hitlag, hitstun, input,
networking, GGRS, and SQLite integration are outside this fixture.

## Assertions and evidence

The fixture test checks complete decoding, action names/frame counts, exact
repeat-run trace equality, first-hit tick/frame/id/damage, one damage application,
sustained contact, zero pre-hit damage, and a far-away-target negative control.
GPU capture asserts colored Falcon and target pixels within geometry regions on
every encoded frame. Frame montage `6_frames.png` is inspected separately.

`4_trace.json` records each simulation tick, source action/frame, root position,
overlap state, hit event, and accumulated damage. `decoded/` preserves decoded
action JSON from initial payload inspection; runtime reads the hashed HTML inputs.

The download's reference GIF is upstream output. The MP4 in this directory is
captured from this lab's executed Rust simulation trace.
