# 6 Remaining: shared base movement machine

Reads after `5_ground_chart.md`/`.svg`. Source audit only: no transition,
physics, animation or abstraction is implemented here, and no package stage is
promoted. Scope is the requested base machine: wait, walk, dash start, dash
dance/self-reentry, run, dash/run brake, turn/turn-run, crouch enter/hold/exit,
jumpsquat, ground jump F/B, aerial jump F/B, fall variants, ordinary/light/heavy/
aerial landing. Attacks, hits, defense, grabs, ledges, items and stage contacts
are excluded except the ground contact that enters landing.

Sources: `src/{1_state,1a_chart,1b_ground,2_advance,4_ground_chart}.rs`,
`smash/src/fighters/falcon/{1c_movement,2_simulation}.rs`, read-only
`kneeman-lines/4_melee_decomp/src/melee/ft/kinds/ftCommon/`, and
`kneeman-lines/7_project_m_cc/[Dev Resources]/codes-3_6.txt`. `codes-3_61.txt` is
3.6.1 and is never substituted; `codes-3_6.txt` is uninterpreted 3.6 patch words
(labelled Unknown Code), so every PM3.6 row below stays unresolved.

## Coverage matrix

Counts are enumerated, not sampled. `Src states` = distinct `ftCo_MS_*` in scope
(25). `Src edges` = transitions in an inspected callback whose destination is a
base state (42). `Chart` = 1b live statig rules (28 event-qualified) and the
isolated 1a crouch cycle (4). `Live` = executed by `State::advance` and the
Falcon consumer. Animation IDs are `CATALOG` indices; an ID is never evidence of
an edge. Fidelity is unqualified everywhere: no Melee/PM callback schedule is
translated, and no PM3.6 behavior is decoded.

| Family | Src states | Src edges | Chart (1b / 1a) | Live | Falcon anim (ID) | Restore |
| --- | --- | --- | --- | --- | --- | --- |
| Wait/Walk | 4 | 9 | 7 / 0 | partial | Wait1=0; WalkSlow/Mid/Fast=7/8/9 | 360-tick tape |
| Dash | 1 | 4 | 6 / 0 | yes | Dash=10 | 360-tick tape |
| Run (+RunDirect) | 2 | 3 | 6 / 0 | yes | Run=11 | 360-tick, JSON |
| Brake (RunBrake) | 1 | 3 | 2 / 0 | yes | RunBrake=12 | 360-tick tape |
| Turn/TurnRun | 2 | 5 | 3 / 0 | partial | TurnRun=14 | 360-tick tape |
| Crouch | 3 | 7 | 2 / 4 | partial | none selected; Crouch=JumpSquat 3 | 1a clone+JSON only |
| Jumpsquat | 1 | 2 | 1 / 0 | yes | JumpSquat=3 | 360-tick tape |
| Ground jump F/B | 2 | 1 | 0 / 0 | partial | JumpF=1; JumpB=15 unused | 360-tick tape |
| Aerial jump F/B | 2 | 1 | 0 / 0 | partial | JumpAerialF=16; B=21 unused | 360-tick tape |
| Fall variants | 6 | 1 | 0 / 0 | partial | Fall=4 only; FallF/B/Aerial unselected | 360-tick tape |
| Landing | 1 | 6 | 1 / 0 | partial | LandingHeavy=6, LandingAirF=5, LandingLight=17 unused | 360-tick tape |

Totals: 25 source states; 42 source edges in scope; 28 live chart rules of which
23 match a cited source edge and 5 are local policy; 4 isolated crouch rules not
wired to `Phase`; 5 procedural edges with no chart authority;
27 remaining transitions (below).

## Duplicate authorities

1. `1a_chart.rs::Action`/`Facts` (crouch cycle, re-exported as
   `game_fighter::Facts`) and `1b_ground.rs::Phase`/`Facts` are disconnected; the
   crate exposes two `Facts` types and two primary-action vocabularies.
2. Grounded selection lives in `1b_ground.rs`, while `2_advance.rs` re-decides
   the same phases in `match self.phase` and pre-dispatches `jump_starts_squat`.
3. Air (`Jump/Fall/AirJump`), takeoff, completion and ground contact transitions
   are procedural in `2_advance.rs` with no chart representation.
4. Falcon `1c_movement.rs` re-encodes phase semantics into `action` indices and
   overrides (aerial attack, recovery landing) outside `Phase`.
5. `4_ground_chart.rs::PHASES` and `1_state.rs::Phase` are two orderings that
   must stay in sync; the generated `5_ground_chart.md` is a third projection.
6. `Rules` hardcodes common callback timings (`dash_ticks=15`, `turn_ticks`,
   `landing_lag`) as local policy, not decoded source command variables.

## Remaining transitions (ordered by source callback)

Guarded edges the inspected callbacks establish whose destination is a base
state and that no live chart rule executes. `[1a]` = the isolated crouch chart
has the state but not the edge.

| # | Source callback (line) | Edge | Guard / fact | Current live state |
| --- | --- | --- | --- | --- |
| 1 | `ftCo_Wait.c:67` | Wait -> Turn | `ftCo_800C97A8` stick <= `x34` | absent |
| 2 | `ftCo_Walk.c:104` | Walk -> Squat | `ftCo_800D5FB0` crouch request | absent |
| 3 | `ftCo_Walk.c:106` | Walk -> Walk{Slow,Mid,Fast} | `ftWalkCommon_800DFEC8` tier | tier not modeled |
| 4 | `ftCo_Dash.c:42-43` | Dash -> Turn | reverse past smash threshold | collapsed to Dash self |
| 5 | `ftCo_Run.c:128` | Run -> TurnRun | `fn_800C9D40` run timer elapsed | collapsed to Turn |
| 6 | `ftCo_RunBrake.c:88` | RunBrake -> TurnRun | `cmd_vars[0] && fn_800C9CEC` | absent |
| 7 | `ftCo_RunBrake.c:89` | RunBrake -> Squat | `ftCo_800D5FB0` crouch | absent |
| 8 | `ftCo_Turn.c:95-97` | Turn -> Wait/Walk | `ft_8008A2BC` completion | Idle only |
| 9 | `ftCo_TurnRun.c:84` | TurnRun -> KneeBend | `fn_800CAF78` jump | collapsed to Turn jump |
| 10 | `ftCo_TurnRun.c:77` | TurnRun -> Run | `fn_800CA644` completion | absent |
| 11 | `ftCo_Squat.c:124` | Squat -> KneeBend | `ftCo_Jump_CheckInput` | absent `[1a]` |
| 12 | `ftCo_SquatWait.c:118` | SquatWait -> KneeBend | jump interrupt | absent `[1a]` |
| 13 | `ftCo_SquatWait.c:120` | SquatWait -> Squat | `fn_800D62C4` re-enter | absent `[1a]` |
| 14 | `ftCo_SquatWait.c:120` | SquatWait -> Dash | `ftCo_Dash_CheckInput` | absent `[1a]` |
| 15 | `ftCo_SquatRv.c:79` | SquatRv -> KneeBend | jump interrupt | absent `[1a]` |
| 16 | `ftCo_SquatRv.c:80` | SquatRv -> Walk | `ftCo_Walk_CheckInput` | absent `[1a]` |
| 17 | `ftCo_Jump.c:161-163` | KneeBend -> JumpB | stick backward vs `x78` | always JumpF |
| 18 | `ftCo_Jump.c:169-174` | JumpF/B -> Fall | `!ftAnim_IsFramesRemaining` | velocity <= 0 instead |
| 19 | `ftCo_JumpAerial.c:274-279` | JumpAerial -> FallAerial | animation end | enters Fall |
| 20 | `ftCo_JumpAerial.c:173-175` | JumpAerialF/B select | stick vs `x78` | always JumpAerialF |
| 21 | `ftCo_Fall.c:157-200` | Fall -> FallF/B | drift fraction `x444` | Fall only |
| 22 | `ftCo_Fall.c:147` | Fall -> JumpAerialF/B | `ftCo_800CB870` | procedural in `air_step` |
| 23 | `ftCo_Landing.c:156` | Landing -> KneeBend | `ftCo_Jump_CheckInput` | chart rejects jump |
| 24 | `ftCo_Landing.c:157` | Landing -> Dash | `ftCo_Dash_CheckInput` | absent |
| 25 | `ftCo_Landing.c:158-159` | Landing -> SquatWait | `ftCo_SquatWait_CheckInput` + lag | absent |
| 26 | `ftCo_Landing.c:160` | Landing -> Turn | `ftCo_Turn_CheckInput` | absent |
| 27 | `ftCo_Landing.c:161` | Landing -> Walk | `ftCo_Walk_CheckInput` | absent |

Unresolved helpers carried verbatim (no unconditional edge authored):
`ftCo_800D6824`, `ftCo_800D68C0`, `ftCo_80091A4C`, `ftCo_800DE9D8`,
`ftCo_80099794`, `ftCo_80099F9C`/`ftCo_800D638C` platform pass, `fn_800CAF78`
altitude gate, `ft_8008A2BC` generic ground resolver, `ftCo_800C5240`/`ft_800D2D0C`
item hooks. Jump and fall IASA lists are dominated by attack/air-catch/special
checks that stay out of scope. `codes-3_6.txt` supplies no readable behavior, so
all PM3.6 timing and jump/landing differences remain unresolved.

## Next three cuts

1. Ground permission closure. Files: `src/1b_ground.rs`, `src/2_advance.rs`,
   `tests/2_ground.rs`. Signature: add `Event::GroundContact` and facts
   `{ turn: bool, squat: bool, squat_hold: bool, walk_tier: u8 }`; return
   `Option<Phase>` unchanged. Tests: source-ordered cases for rows 1, 2, 4-10,
   23-27; delete the `jump_starts_squat` pre-dispatch and the duplicate
   `match self.phase` arms. Terminal: those rows are chart-executed, generated
   `5_ground_chart.md` is fresh, and no new `Phase` variant exists.
2. Live crouch cycle. Files: `src/1a_chart.rs`, `src/1_state.rs`,
   `src/2_advance.rs`, `smash/src/fighters/falcon/1c_movement.rs`,
   `tests/1_chart.rs`. Signature: `fn action_phase(a: chart::Action) -> Phase`
   plus `CrouchSlice` dispatch inside `advance`; `State` keeps one serialized
   field. Tests: rows 11-16, clone and JSON suffix replay, Falcon IDs 18/19/20
   selected during enter/hold/exit. Terminal: live crouch is three source
   actions, not hold-only, and `Phase` still has no parallel chart field.
3. Air family chart. Files: new `src/1c_air.rs` (registered in `src/lib.rs`),
   `src/2_advance.rs`, `smash/src/fighters/falcon/1c_movement.rs`,
   `tests/0_movement.rs`. Signature:
   `air::decide(phase: Phase, ev: AirEvent, f: AirFacts) -> Option<Phase>` with
   `AirFacts { animation_finished, stick_x, facing, grounded_contact }`. Tests:
   rows 17-22, JumpF/JumpB and JumpAerialF/B selection, animation-end fall,
   contact entry into landing. Terminal: air transitions are chart-executed,
   Falcon selects the signed jump and fall variants, and the 360-tick restore
   tape still reproduces bit-exact.
