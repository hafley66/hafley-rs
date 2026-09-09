# I1: existing input-buffer extraction checkpoint

2026-09-09. Read-only source inspection; no runtime changes or new test execution.
Depends on `104_tasks.md`. Source revisions: sibling `hafley-rs-game-runtime`
`8646fa2b91a9827dd7e671d85ea5585f2264c507`; sibling `smashy`
`0befd3f91987a5dfd3a62ac253b4f2577d1372e5`. References below are repository-relative.

## 1. V1 implementation

`games/kneeman/src/v1/state.rs`: six lanes (Movement, Aerial, Attack, Strong,
Grab, Special); `Slot { action: Action, timer: i64, aim: Vector2 }`.
`fighter.rs`: serialized, copyable `Fighter` owns `[Slot; N_LANE]`.

Existing signatures:

```rust
fn live(&self, lane: Lane) -> Action;
fn record(&mut self, lane: Lane, action: Action, aim: Vector2, tune: &Tune);
fn clear_lane(&mut self, lane: Lane);
fn window(self, tune: &Tune) -> i64; // Action
```

`za_warudo.rs` ordinary active-frame path:

```text
derive input/context reads
  -> early return for frozen/held/stationed/hitstun paths
  -> decrement each positive lane timer
  -> refresh live movement aim from non-neutral stick
  -> record eligible edges (newest overwrites same lane)
  -> context-dependent attack mapping
  -> state eligibility reads live lanes
  -> accepted action clears its lane(s)
```

Record sets `timer = action.window(tune) + 1`: zero window still permits the
press frame. Most actions use `buffer_frames`; None/Grab use zero. Held-item
throw overrides Grab's duration separately. Timer expiry makes an action
ineligible but leaves stale payload bytes; explicit clear resets the slot.
Lanes coexist; action eligibility and cross-lane consumption are game policy.
No filesystem, network, clock or database effects in these slot operations.

Instance lifetime: one slot array per fighter, copied/serialized with fighter
state. Reads borrow input/tuning for the step; slots retain owned action/aim/timer.
Uniqueness is lane index, with at most one pending action per lane. Expiration
advances on the selected simulation path, not automatically on every global tick.

## 2. Existing behavior tests and distinct mechanisms

`games/kneeman/src/v1/replay_tests.rs`:

- `jump_plus_attack_autohops_into_an_aerial`: simultaneous jump/attack survives
  squat and reaches an aerial, with the auto-hop flag. This supplies the lane
  buffering scenario; Falcon's first adapter only has fair and does not inherit
  this test's full short-hop/damage policy.
- `a_buffered_shield_press_saves_out_of_hitstun_at_low_percent`,
  `a_high_percent_save_keeps_part_of_the_launch`, and
  `no_dodge_charge_means_no_save` exercise a SEPARATE `tech_buf` mechanism.
  `hitstun_slide` ages/rearms that countdown and consumes it at stun exit or
  ground tech. Do not claim these prove ordinary action-slot behavior.

These tests were read, not executed in this checkpoint. Exact expiry, replacement,
pause and restore tests must accompany the shared extraction. Existing tests
remain source references; a named replay test alone is not GGRS verification.

## 3. V3 reference

In `smashy/reference/og-v1/gdscript-labs/v3-game-dom-css-lab/`:

- `lab_input_runtime.gd`: samples device state, derives presses/releases, emits
  input events. P1 history is table-owned; P2 reads fighter previous-input state.
- `INTERPRETER_BOUNDARY.md.txt`: explicitly records no authored buffer yet.
- `LANGUAGE_PRIMITIVES.md.txt`: buffered intent FSM is documented only.
- `DESIGN_COMPASS.md.txt`, Input Buffer And Intents: defines intended flow
  facts -> intents -> entries -> consume routes -> effects, accepting/suspended
  buffer state and pending/consumed/expired entries.

Preserve that design requirement. Its example named "ultimate-buffer" is an
authored proposal, not verified Ultimate mechanics. Melee, PM and Ultimate need
distinct source/version-backed policies; no numeric game presets are established
by this inspection.

## Proposed I2 diff, awaiting review

| Location | Proposed change |
| --- | --- |
| `games/shared/input/` | Port existing input value/quantization; extract slot operations from V1 with source provenance and tests. |
| `simulation-core/2_simulation.rs` | Own pending input in World snapshots; preserve previous-input edge history; borrow immutable selected policy. |
| `simulation-core/1a_actions.rs` | Record early attack, consume when aerial eligibility opens, preserve source-frame flags and existing movement. |
| Falcon authored TSP and consumers | After skill-guided schema inspection, declare required policy/inspection IO once and regenerate; exact schema file diff remains part of implementation preparation. |
| Existing tests/capture | Add early-attack tape, expiry/replacement/cancellation/restore assertions and actual-state labels; use shared recording machinery when extending it. |

Proposed shared signatures (not implemented):

```rust
struct Slot<A, Aim> { action: A, timer: i64, aim: Aim }
impl<A: Copy + Default, Aim: Copy + Default> Slot<A, Aim> {
    fn age(&mut self); // decrement positive timer once when caller advances it
    fn record(&mut self, action: A, aim: Aim, window: i64);
    // replace slot; preserve window + 1 press-frame semantics, validate bounds
    fn live(&self) -> Option<A>; // timer > 0; adapter never records sentinel None
    fn clear(&mut self); // reset slot
}
```

Game-owned lane IDs, action IDs, duration selection, aim refresh and eligibility
stay outside this shared storage operation. No generic event runtime or scheduler.
The caller can suspend aging explicitly. Held/repeat policies, intent composition,
tech windows, coyote windows and item-specific exceptions remain tracked extensions.

First Falcon acceptance: press attack during jump squat; pending aerial survives
until airborne, is consumed once, and expires if eligibility is too late. Compare
zero-duration versus a named lab-duration policy, without labeling either as
source-game equivalence. Restore across pending and consumed states; corrected
GGRS states must match the independent tape. Cancellation needs an explicit lab
reset/clear condition, not unconditional clearing on every action transition.

Yield point: review this extraction and mapping before I2; I3 follows integration.
