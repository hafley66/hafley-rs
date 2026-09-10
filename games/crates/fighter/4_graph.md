# Common fighter graph: source distillation

Status: source-backed design inventory, with a four-edge crouch qualification in
`src/1a_chart.rs` and `tests/1_chart.rs`. The live controller now dispatches its
grounded decisions through `src/1b_ground.rs`; air/contact decisions remain in
`src/2_advance.rs`. The crouch qualification handles one
semantic dispatch, not the source game's full per-tick callback schedule.
Scope: action exclusivity, transitions, guards, ordering and snapshot semantics.
No velocity integration, collision solver, device mapping or animation renderer.

## Local sources

Paths below are relative to the user's projects directory. These are read-only
research inputs, not new runtime dependencies.

- `kneeman-lines/4_melee_decomp/src/melee/ft/kinds/ftCommon/forward.h`:
  `ftCommon_MotionState`, the common action vocabulary.
- `kneeman-lines/4_melee_decomp/src/melee/ft/ftmotionstates.c`:
  action dispatch records separately select Anim, IASA, Phys and Coll callbacks.
- `kneeman-lines/4_melee_decomp/src/melee/ft/types.h`:
  `motion_id`, `ground_or_air` and hitlag fields coexist.
- `kneeman-lines/7_project_m_cc/[Dev Resources]/codes-3_6.txt`:
  retained 3.6 patch words, labelled Unknown Code. No interpreted transition
  claims from these words yet.
- The adjacent annotated `codes-3_61.txt` is 3.6.1 evidence. Do not silently
  substitute its comments for verified 3.6 rules. Repository README explicitly
  describes Community Completion based on development 3.6.1.
- `kneeman-lines/5_brawl_decomp`: located; common callback coverage not qualified.
- Falcon `imported/` retains PM3.6 animation/subaction data and attributes.
  This payload does not establish the complete common transition graph.

## Exclusivity and independent facts

One current primary action per fighter: Wait, Dash, AttackAirF, Guard, etc.
An attack replaces the primary action. Airborne and hitlag can remain true while
the attack is active. Preserve ground/air as a situation fact; preserve hitlag as
an independent clock-control fact. Neither requires duplicating every action.

The groups below are navigation/factoring groups derived from source names.
They are not claims that the original engine implements this hierarchy or that
every character enables every member. Statig superstates may share handlers only
when the source establishes identical guard order and interrupt conditions.

| Family | Source vocabulary | Responsibility |
| --- | --- | --- |
| Ground movement | Wait, WalkSlow/Middle/Fast, Dash, Run, RunDirect, RunBrake, Turn, TurnRun | Ordinary ground actions; turn and running turn stay distinct. |
| Crouch | Squat, SquatWait, SquatRv | Enter, hold, leave crouch are separate actions. |
| Jump and fall | KneeBend, JumpF/B, JumpAerialF/B, Fall variants | Takeoff preparation, ground jump, air jump, ordinary fall. |
| Restricted air | FallSpecial variants, LandingFallSpecial | Restricted recovery and its landing, distinct from ordinary fall. |
| Ground attacks | Attack11/12/13, Attack100Start/Loop/End, AttackDash, AttackS3/Hi3/Lw3, AttackS4/Hi4/Lw4 variants | Jab chains, dash attack, tilts and smashes with move-specific guards. |
| Aerial attacks | AttackAirN/F/B/Hi/Lw, LandingAirN/F/B/Hi/Lw | Aerial action, cancel eligibility and move-specific landing continuation. |
| Defense | GuardOn, Guard, GuardOff, GuardSetOff, GuardReflect, EscapeF/B/N/Air | Shield phases, shield hit reaction, rolls, spot dodge and air dodge. |
| Damage and recovery | Damage variants, DamageFly variants, DamageFall, Down variants, Passive variants, ShieldBreak variants, Furafura | Being hit, tumbling, knockdown, tech/getup choices and shield-break recovery. |
| Grab relationships | Catch, CatchPull, CatchWait, CatchAttack, CatchCut, Throw variants; Capture and Thrown variants | Grabber and victim each have their own exclusive action; their relationship is separate state. |
| Ledge and support | CliffCatch/Wait/Climb/Attack/Escape/Jump, Pass, Ottotto/Wait | Ledge choices, platform pass-through and edge teeter. Geometry later supplies facts. |
| Lifecycle | Dead variants, Rebirth, RebirthWait | KO and re-entry. |
| Content extensions | Common item actions, character special dispatch | Keep item and character capability selection explicit. Shared vocabulary does not grant every capability to every fighter. |

## Transition records and priority

Each executable edge needs: source action, callback phase, ordered guard,
destination, state changes and source provenance. Unresolved helpers keep their
source symbol and an unresolved marker; never convert them into unconditional
edges. A state name inventory alone is not executable behavior.

Concrete inspected examples, under Melee `ftCommon/`:

1. `ftCo_Wait.c:ftCo_Wait_IASA` checks special dispatch and other helpers, grab,
   smash attacks, tilts, jab, defense/other helpers, jump, dash, crouch helper,
   turn and walk in source order. Every RETURN_IF exits when a check succeeds.
2. `ftCo_Turn.c:ftCo_Turn_IASA` explicitly checks jump. S3 adds the missing
   live Turn jump edge. `ftCo_TurnRun_IASA` also calls `fn_800CAF78` for jumping;
   the live Turn remains a simplified running-turn policy, not both source states.
3. `ftCo_KneeBend.c:ftCo_KneeBend_IASA` contains jab-100, grab and up-smash
   checks. Jumpsquat has interrupt edges, in addition to eventual takeoff.
4. `ftCo_Run.c:ftCo_Run_IASA` has its own ordered checks and a run-local timer
   gate. Do not inherit all Wait checks wholesale.
5. `ftCo_Fall.c:ftCo_Fall_IASA_Inner` has an ordered airborne interrupt list,
   including character-specific hooks. Capability guards remain explicit.
6. `ftCo_AttackAir.c:DO_IASA` gates its interrupt checks on `allow_interrupt`.
   `ftCo_AttackAir_Anim` enters Fall when animation frames are exhausted.
7. `ftCo_Landing.c:ftCo_Landing_IASA` first checks landing duration and
   `allow_interrupt`, then evaluates its ordered action checks.

First matching guard wins within an inspected callback. The original runtime can
execute several callback phases during a tick; this inventory does not assert
one transition maximum per entire tick. Scheduling and cross-phase priority need
their own source evidence before executable translation.

## Existing library and lowering contract

### Live ground slice, S3

`ground::decide(Phase, Event) -> Option<Phase>` executes statig with borrowed
semantic facts. `Some(current)` is a real self-transition resetting dash age;
`None` keeps the phase and clock. The caller retains physics and entry impulses.
Redux still serializes the single `State.phase`; a stack-local uninitialized
machine is rehydrated before dispatch and dropped afterward. No entry/exit hooks
or persistent parallel phase are introduced. The transition hook only returns
the dispatch's changed flag. Complete World restore tests exercise the consumer.

JumpRequest precedes movement callbacks for Idle, Walk, Dash, Run, Brake, Turn
and Crouch. Squat and recovery Landing reject that ground-jump event. Air jumping
remains in the existing air handler. Motion guards preserve the prior local
policy: Dash reversal before run completion, Run reversal before braking before
crouching, Turn reversal before timeout. Numbers and formulas are unchanged.

The four-state crouch qualification still has a separate migration gate: live
Crouch remains hold-only, and Turn/TurnRun plus distinct dash/run stopping and
source command-variable timing remain unresolved. This increment adds no attack,
contact or PM3.6-equivalence claims.

Use existing macro-free statig 0.4.1, as in `../input/src/2_buffer.rs` and
`../redux/tests/2_statechart.rs`. No replacement library is proposed.

Proposed boundary, not shipped API:

```rust
fn step(chart: &mut FighterChart, event: &Event, cx: &mut Context);
// Read current action + borrowed rules/facts.
// Evaluate this callback's ordered guards.
// Return statig Handled, Super, or Transition(next).
// Record logical transition effects, without integrating physics or publishing IO.
```

Redux dispatch owns the chart update. The snapshot owns the current action,
logical action age, guard/cancel state, and any persistent chart storage. Rules,
edge provenance and content definitions are immutable borrowed context.
Each dispatch borrows facts and an effect sink only for that call. Physical input
interpretation and contact detection will later supply facts; the chart owns
permission and destination selection. Do not use a universal list of `can_*`
booleans to erase source-state-specific priorities.

Statig initialization requires care: the existing Redux qualification proves
that raw Serde restore can rerun entry actions. Clone rollback works in that
qualification. Keep irreversible work outside hooks; test both restore methods
before claiming portable save/restore for the fighter chart.

## Executable completion gates

- Every imported state and edge has a source ID, ruleset and implemented,
  unresolved, or deliberately excluded disposition.
- Ordered simultaneous-guard tests match each translated callback.
- Exactly one primary action is active; situation and clock facts coexist.
- Cancel-closed/open, completion, landing and rejected-edge traces are asserted.
- Clone and serialized suffix replay reproduce state and logical effects.
- First production cut replaces the matching live controller path; no second
  maintained gameplay implementation. Existing physics remains untouched.
- No PM3.6 parity claim until its patch/base behavior is resolved and tested.
