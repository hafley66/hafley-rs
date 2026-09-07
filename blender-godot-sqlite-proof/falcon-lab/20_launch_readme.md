# Increment 3: sandbag launch and recovery under rollback

Run `bash 15_run_launch.sh`. Earlier MP4s and their run modes remain available.
The launch mode is `cargo run --locked --bin falcon-rollback -- --launch`.
Add `--verify-only` for CPU-only execution and trace output.

## Executed states

| Simulation tick | On-time sandbag state |
| --- | --- |
| 0–90 | Hovering, gravity disabled |
| 91 | Hit: damage 18, knockback scalar 67.2, hitstun 26 |
| 92–116 | Hitstun, Rapier advances flight and decrements the timer |
| 117–125 | Falling, timer expired |
| 126 onward | Landed, active Rapier contact with the floor |
| 179 | Position approximately (forward 74.82448, up 5.999408), velocity zero |

The body has half-height 6, so its center rests near y=6. The floor top is y=0.
It remains hovering until impact. Hit is visible for one simulation tick before
flight advances. Video event holds repeat captured state without advancing time.

## Existing library code used

Inspection of published [ssbm_utils 0.4.0](https://docs.rs/ssbm_utils/0.4.0/ssbm_utils/)
confirmed these public functions in `calc`: `knockback`, `resolve_sakurai_angle`,
`initial_x_velocity`, `initial_y_velocity`, and `hitstun`. Those functions execute
in `1a_sandbag.rs`; the lab does not duplicate their formulas.

Inputs come from the extracted PM knee hitbox: damage 18, angle 32 degrees,
growth 100, base knockback 24, set knockback zero. Target weight is 100 using
`Attributes::MARIO`; only name and weight are read by the knockback helper.
No stale-move or special defensive modifiers are enabled; initial percent is zero.
The helper includes hit damage internally, so it receives pre-hit percent.

[Rapier 0.35.3](https://docs.rs/rapier3d/0.35.3/rapier3d/) supplies dynamic-body
stepping, gravity, damping, floor contact, friction, and durable serialization.
The lab retains source coordinate units. Launch units/tick become units/second
by multiplying by 60. Gravity is 0.095 units/tick², converted by multiplying by
3600. Linear damping is 0.8; rotation is locked; restitution is zero; friction is
1.0. These are explicit fixture parameters.

This combines PM attack data, Melee calculation helpers, and Rapier dynamics.
The on-screen ruleset label states this combination. It does not establish
PM-accurate knockback travel, decay, hitlag, DI, techs, or collision resolution.

## Snapshot ownership and comparison

`World.bag` owns the complete Rapier PhysicsWorld, body handle, visible position
and velocity, phase, hitstun timer, knockback, and grounded status. Snapshot cloning
round-trips durable serde JSON; equality compares serialized durable state. This
includes contact/solver state, beyond visible position and velocity alone.
This allocation-heavy correctness fixture does not establish hot-buffer costs.

The same GGRS packet hold applies: A-to-B ticks 78–96 arrive on tick 97. B restores
78, replays 19 prior ticks, and advances once. Full world equality holds from tick
97 onward. The on-time peer matches direct execution throughout. Zero-delay and
repeat-run controls pass; final damage is 18 and hit count is one on each peer.
Additional snapshots taken before ticks 105 (flight) and 128 (floor contact)
are restored and replayed to tick 180. They match the uninterrupted durable world
exactly and remain independent while the live world advances.

## Output and validation

- `16_peer0.mp4`, `16_peer1.mp4`: native wgpu captures.
- `17_launch_trace.json`: complete world state plus GGRS diagnostics per tick.
- `18_launch.mp4`: side-by-side H.264 comparison.
- `19_launch_frames.png`: impact, correction, falling, landing, and final state.

Each pane has 824 encoded frames at 60 FPS. The side-by-side MP4 is 1920x540,
approximately 13.733 seconds, with half-speed playback and event holds.

HUD adds bag phase, hitstun ticks, grounded status, position, and velocity in
units/tick. Grounded status comes from active Rapier contact. Both panes use the
same fixed camera transform. GPU pixel checks follow the moving target region.
The test suite and Clippy with warnings denied pass for the lab targets.

No external network, Godot/Bevy, skinned costume mesh, or cross-machine
determinism is established by this increment.
