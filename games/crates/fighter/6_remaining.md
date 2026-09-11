# Remaining base movement machine

Read-only audit of the requested primordial locomotion set before the shared
fighter statechart covers it. Sources: `_1_state.rs`, `1a_chart.rs`, `_1b_ground.rs`,
`_2_advance.rs`, `_4_ground_chart.rs`, generated `5_ground_chart.md`, Pigeon
`1c_movement.rs`/`2_simulation.rs`, and read-only Melee decomp under
`kneeman-lines/4_melee_decomp`. Excludes attacks, hits, defense, grabs, ledges,
lifecycle, items and stage contacts except the landing entry fact. No transition
or edge is inferred from an animation name; every edge below cites a callback.

## Matrix (exact counts)

| Axis | Present | Missing / other | Basis |
| --- | --- | --- | --- |
| Source actions in scope | 30 named | 0 unnumbered | `ftCommon/forward.h` motion enum |
| Host `Phase` states | 12 | 2 source actions absent (`RunDirect`, `SquatRv`); 16 collapsed into a shared host | `_1_state.rs:33-46` |
| Executed chart edges | 28 | 22 remaining | `_1b_ground.rs` `decide` |
| Isolated crouch chart edges | 4 | unwired to live | `1a_chart.rs` |
| Procedural phase moves (`_2_advance.rs`) | 5 | to migrate | `_2_advance.rs:292-308,319-326` |
| Duplicate authorities | 6 | see below | |
| Pigeon catalog IDs selected live | 15 of 22 | 7 never selected (`13,15,17,18,19,20,21`) | `1b_catalog.rs`, `1c_movement.rs:51-57` |

Host states are `Idle, Walk, Dash, Run, Brake, Turn, Squat, Crouch, Landing, Jump,
Fall, AirJump`. Collapsed groups: Walk covers `WalkSlow/Middle/Fast`; Turn covers
`Turn`+`TurnRun`; Crouch covers `Squat`+`SquatWait`; Jump covers `JumpF/B`; AirJump
covers `JumpAerialF/B`; Fall covers `Fall/F/B/FallAerial/F/B`; Landing covers
`Landing`+five `LandingAir*`.

## Duplicate authorities

1. Two crouch machines: `chart::Action`/`CrouchChart` (`1a_chart.rs`) and
   `Phase::Crouch` inside `_1b_ground::decide` (`_1b_ground.rs`). The isolated chart is
   unwired; live crouch is hold-only and emits the jumpsquat pose.
2. Selection vs entry split: `_1b_ground::decide` returns a `Phase`, then
   `State::enter` in `_2_advance.rs` writes it and applies transition-specific
   impulses (`start_dash`, `start_walk`, `takeoff`).
3. Naming collision: host `Phase::Squat` is source `KneeBend` (jumpsquat) while
   source `Squat` is host `Phase::Crouch`.
4. `Phase::Turn` emits Pigeon catalog ID14 `TurnRun`; source `Turn` (ID13) is
   unselected.
5. `Phase::Landing` conflates source `Landing` and the five `LandingAir*`; Pigeon
   additionally overrides `landing_lag` from `actions[5].frames.len()` in the app.
6. Air decisions (`Jump`, `Fall`, `AirJump`, `Landing`) live only in
   `_2_advance.rs`; the generated chart proves they reject `JumpRequest` only.

## Transitions still procedural in _2_advance.rs

No `_1b_ground::decide` call, direct `State::enter`:

1. `Jump -> Fall` on `velocity[1] <= 0` (`_2_advance.rs:292-296`).
2. `AirJump -> Fall` on the same condition.
3. `Fall -> AirJump` on `jump_pressed && jumps_left > 0` (`air_step`, 319-326).
4. `Jump -> AirJump` on the same branch.
5. any airborne -> `Landing` on `position[1] <= 0 && velocity[1] < 0` (302-308).

`Squat -> Jump` is chart-selected (`_1b_ground.rs:74`) but the `Jump` write and
takeoff impulse stay in `_2_advance.rs:239-242`.

## Remaining transitions (ordered)

1. `Run -> TurnRun`, source `ftCo_Run_IASA:128` `fn_800C9D40`; state absent.
2. `TurnRun -> Run`, `ftCo_TurnRun_Anim:77` `fn_800CA644`; state absent.
3. `TurnRun -> Jump`, `ftCo_TurnRun_IASA:84` `fn_800CAF78`; state absent.
4. `Run -> RunBrake`, `ftCo_Run_IASA:130`; host RunBrake merged into `Brake`.
5. `RunBrake -> Squat`, `ftCo_RunBrake_IASA:89` `ftCo_800D5FB0`; host `Brake` has no down edge.
6. `Brake -> Idle`, `ftCo_RunBrake_Anim:78-79` `ft_8008A2BC`; present as a merged policy.
7. `Dash -> Turn` in the dash-smash window, `ftCo_Dash_IASA:113-114`
   `ftCo_Turn_Enter_Smash`; host Dash self-reverses instead.
8. `Wait -> Turn`, `ftCo_Wait_IASA:67` `ftCo_Turn_CheckInput`; host Idle has no Turn.
9. `Dash -> RunDirect`; `RunDirect` is a named state (`forward.h:311`) with an
   unresolved entry path, not an edge.
10. `Squat -> SquatWait`, `ftCo_Squat_Anim:85-86` `ftCo_800D638C`; isolated chart only.
11. `SquatWait -> SquatRv`, `ftCo_SquatWait_IASA:121`; isolated chart only.
12. `SquatWait` re-enter, `ftCo_SquatWait_CheckInput:53-54` `fn_800D62C4`; unmodeled.
13. `SquatRv -> Wait`, `ftCo_SquatRv_Anim:61-62` `ft_8008A2BC`; isolated chart only.
14. `Squat/SquatWait -> Jump`, `ftCo_Jump_CheckInput` at Squat IASA 124 and
    SquatWait IASA 118; reachable via host Crouch -> Squat.
15. Ground jump forward/back select, `ftCo_Jump_Enter:161-163` `x78`; host always forward.
16. `Jump -> Fall`, `ftCo_Jump_Anim:172` `ftCo_Fall_Enter`; procedural today.
17. `Fall -> FallF/FallB` select, `ftCo_Fall_Anim_Inner:176-187`; states absent.
18. `Fall -> AirJump`, `ftCo_800CB870` `ftCo_JumpAerial_CheckInput`; procedural today.
19. Air jump forward/back select, `ftCo_JumpAerial_Enter_Basic:173`; host always forward (ID16).
20. `JumpAerial -> FallAerial`, `ftCo_JumpAerial_Anim:276`; host falls to `Fall`.
21. Airborne -> `LandingAirN/F/B/Hi/Lw` select, `ftCo_LandingAir_EnterWithLag:23-54`; one host state.
22. Landing duration and `allow_interrupt` ordering, `ftCo_Landing_IASA:140-141`;
    `landing_lag` is used, `allow_interrupt` is always true.

## Callback phase and guard order

First matching guard wins inside one callback; the runtime may run several
callbacks per tick, so this audit does not assert one transition per tick.

| Callback | Phase | Ordered guards (inspected) | Note |
| --- | --- | --- | --- |
| `ftCo_Wait_IASA` | IASA | specials, grab, smashes, tilts, jab, defense, jump, dash, squat, turn, walk | `:46-68` |
| `ftCo_Dash_IASA` | IASA | window branch, dash-smash reverse, attacks, `fn_800CAF78`, `fn_800CA5F0` | `:84-139` |
| `ftCo_Run_IASA` | IASA | specials, attacks, timer gate, `fn_800CAF78`, `fn_800C9D40`, `RunBrake_CheckInput` | `:104-131` |
| `ftCo_RunBrake_IASA` | IASA | `fn_800CAF78`, `fn_800C9CEC`, `ftCo_800D5FB0` | `:83-90` |
| `ftCo_KneeBend_Anim` | Anim | `jump_startup_time` or anim end -> `ftCo_Jump_Enter` | `:31-45` |
| `ftCo_Landing_IASA` | IASA | `cur_anim_frame < landing_lag`, `allow_interrupt`, then action list | `:134-162` |

Unresolved helpers, not converted to edges: `ft_8008A2BC`, `ftWalkCommon_800DFEC8`,
`fn_800C9D40`, `fn_800CA5F0`, `fn_800CA644`, `fn_800CAF78`, `ftCo_800D5FB0`.

## PM3.6 status

`kneeman-lines/7_project_m_cc/[Dev Resources]/codes-3_6.txt` is raw Gecko words
labelled `Unknown Code`; no entry carries an interpreted locomotion transition.
`codes-3_61.txt` is UTF-16 and annotated, but it is 3.6.1 evidence. The repository
README bases Community Completion on dev 3.6.1. No 3.6.1 comment is substituted
for a verified 3.6 rule; PM3.6 behavior stays unresolved here.

## Three next implementation cuts

### Cut 1: air/contact chart

- Owned: new `crates/fighter/src/_1c_air.rs`; `crates/fighter/src/lib.rs`;
  `crates/fighter/src/_2_advance.rs`; new `crates/fighter/tests/4_air.rs`.
- Signature:
  ```rust
  pub struct AirFacts { pub vy: f32, pub jump_pressed: bool, pub jumps_left: u8 }
  pub enum AirEvent { Motion(AirFacts), Land }
  pub fn decide(phase: Phase, event: AirEvent) -> Option<Phase>;
  ```
- Tests: ordered guards (`Jump/AirJump -> Fall` on `vy <= 0` before `-> AirJump`;
  `Fall -> AirJump` needs `jumps_left > 0`; `Land` requires airborne and `vy < 0`);
  serialized phase suffix replay.
- Terminal: no `enter(Phase::Fall | Phase::AirJump | Phase::Landing)` stays in
  `_2_advance.rs`; the existing 360-tick Pigeon tape is byte-identical; tests pass.

### Cut 2: distinct stopping and turning states

- Owned: `crates/fighter/src/_1_state.rs`, `crates/fighter/src/_1b_ground.rs`,
  `crates/fighter/tests/2_ground.rs`.
- Signature: add `Phase::TurnRun`, `Phase::RunDirect`, `Phase::RunBrake`; edges
  `Run -> TurnRun` (reverse), `Run -> RunBrake` (no stick), `TurnRun -> Run`
  (forward), `RunBrake -> Idle` (stopped), `RunBrake -> Crouch` (down).
- Tests: ordered guard cases mirroring `ftCo_Run_IASA` and `ftCo_RunBrake_IASA`,
  plus competing-fact traces.
- Terminal: `just ground-chart` regenerates `5_ground_chart.md`/`.d2`/`.svg` with
  the new phases; the fighter freshness test accepts Markdown and D2.

### Cut 3: wire crouch lifecycle and Pigeon poses

- Owned: `crates/fighter/src/1a_chart.rs`, `crates/fighter/src/_2_advance.rs`
  crouch branch, `smash/src/fighters/pigeon/1c_movement.rs`,
  `crates/fighter/tests/1_chart.rs`.
- Signature: `CrouchChart::step(Facts) -> Action` driven from `Phase::Crouch`;
  Pigeon maps `Action` to catalog IDs 18/19/20 instead of ID3.
- Tests: live tape enters and exits crouch through all three poses; clone and
  JSON suffix replay match.
- Terminal: `Phase::Crouch` no longer holds the jumpsquat pose; IDs 18/19/20 are
  selected; the 360-tick Pigeon restore tape still passes.

Cross-target restore stays unqualified: Godot/browser observe phases but no
snapshot is restored across targets.
