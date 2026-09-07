//! The step pipeline as a `Then` chain of ZST phases (plans/core-rx-refactor.md section 2).
//! `step()`'s old inline comment blocks (lib.rs:186-332) become seven `Slice` impls composed
//! by the tuple macro: order is now a type, and a misordered pipeline fails to compile
//! (the adjacent `Output = Event` constraint). Every phase is a ZST; `Step` is a ZST.
//!
//! Pure refactor: bodies moved verbatim from the lib.rs line ranges named on each phase.
//! The only transcription change is `s.fighters[p].char_id` -> `n.fighters[p].char_id` when
//! rebuilding the per-fighter `tunes` snapshot inside a phase: `char_id` is written only at
//! spawn and preserved across respawn (fighter.rs:332), so it is frame-stable and the two
//! reads are bit-identical.
//!
//! Recompute hoist (core-rx-refactor.md row 10 commit 3): `tunes` and `np` used to be
//! rebuilt as a phase-local in every phase that needed them (4x for `tunes`: Fsm, Actuate,
//! StrikeInk, Combat; 6x for `np`: Tick, Fsm, Actuate, StrikeInk, Combat, Ink). Both now
//! live on `StepCx`, computed ONCE in `step()` before the pipeline runs, from the
//! frame-start state `s` (bit-identical to any phase's own read: `char_id` and `active`
//! are untouched by every earlier phase in the same tick, so a snapshot taken before `Tick`
//! even runs equals what each phase would have recomputed for itself).
//!
//! Per-phase iteration semantics (recorded for Phase B):
//! | phase  | semantics                        | why                                        |
//! |--------|----------------------------------|--------------------------------------------|
//! | Fsm    | snapshot (simultaneous)          | `bodies` copy, foes self-`None`            |
//! | Combat | live, ordered pairs (port prio)  | `pair_mut` mutation mid-pass is the mechanic |
//! | Items  | reads + writes on shared arrays  | descriptor channel arrives in Phase C      |

use crate::v1::acts::ApplyItemActs;
use crate::v1::body::anchor::{AnchorHost, anchor_pin};
use crate::v1::body::path_surface_vel;
use crate::v1::fighters::{FighterCx, FighterSlice};
use crate::v1::item::{ink_hits_items, maybe_spawn_item, update_items};
use crate::v1::slice::{Each, EachCx, Slice};
use crate::v1::{
    Act, FxKind, InkNode, InkPath, InputFrame, Item, MAX_DRAWN, MAX_ITEMS, MAX_PLAYERS, SimState,
    Tune, Vector2, ZoneMode, ZoneRect, airborne, apply_act, attack_for, booster_blast, charge_mult,
    hitbox_center, hurtbox, ink_blast_zone, ink_hits_fighters, pair_mut, push_fx, resolve_clank,
    resolve_combat, resolve_grab, steer_from_riders,
};
use crate::v1::{items, slice, stage, station};

/// Shared per-tick context for every step phase: the frame's inputs, the match tune, the
/// per-fighter resolved `tunes` snapshot, and `np` (active fighters this tick) -- the last
/// two computed ONCE in `step()`, not rebuilt per phase (row 10 commit 3). `tunes` rides
/// as a REFERENCE, not by value: `Tune` is ~19KB (`AttackData`/`ItemConfig`/`Roster` fields),
/// so `[Tune; MAX_PLAYERS]` is ~76KB -- embedded by value, `StepCx` would get copied at
/// that size through every level of the `Then` chain's nested calls (unoptimized debug
/// builds don't collapse `#[inline(always)]` into one frame), which blew the test-thread
/// stack in practice. A reference keeps `StepCx` a handful of pointers regardless of how
/// deep the pipeline nests, and `step()` computes the array exactly once in ITS OWN frame,
/// which is the same "once per tick" guarantee either way. The ink/hurtbox snapshots stay
/// phase locals: they change WITHIN a tick (ink integrates in `Ink`), so hoisting them
/// would not be the bit-identical refactor `tunes`/`np` are.
#[derive(Copy, Clone)]
pub struct StepCx<'a> {
    pub inputs: &'a [&'a InputFrame],
    pub tune: &'a Tune,
    pub tunes: &'a [Tune; MAX_PLAYERS],
    pub np: usize,
}

/// One frame's emitted actions, produced by `Fsm` and consumed by `Actuate` (the Output channel).
pub type Acts = [Act; MAX_PLAYERS];

/// `Fsm`'s frame-start snapshots, packaged as an `EachCx` provider so the per-slot loop lives in
/// `Each::reduce`. Every field is a snapshot taken before any fighter is advanced, so slot order
/// cannot leak into a result (the "simultaneous" semantics recorded in the module table).
#[derive(Copy, Clone)]
struct FsmEachCx<'a> {
    inputs: &'a [&'a InputFrame],
    tunes: &'a [Tune; MAX_PLAYERS],
    items: &'a [Item; MAX_ITEMS],
    paths: &'a [InkPath; MAX_DRAWN],
    nodes: &'a [InkNode],
    bodies: &'a [Option<(Vector2, f32)>; MAX_PLAYERS],
    zone: Option<ZoneRect>,
    live: usize,
}

impl<'a> EachCx<FighterSlice, MAX_PLAYERS> for FsmEachCx<'a> {
    fn live(&self) -> usize {
        self.live
    }
    fn idle(&self) -> Act {
        Act::None // dormant slots (>= live); no downstream phase reads them
    }
    fn event(&self, slot: usize) -> InputFrame {
        *self.inputs[slot]
    }
    fn slot_cx(&self, slot: usize) -> FighterCx<'_> {
        let mut foes = *self.bodies;
        foes[slot] = None; // you can't footstool yourself
        FighterCx {
            items: self.items,
            paths: self.paths,
            nodes: self.nodes,
            foes,
            tune: &self.tunes[slot],
            zone: self.zone,
        }
    }
}

crate::slice! {
    /// Tick: advance the frame counter, drive the mover, maybe spawn an item, steer the ship.
    /// lib.rs:188-204 (the `tunes`/snapshot lines move into `Fsm`).
    pub Tick for SimState {
        context: StepCx<'a>, event: (), output: (),
        reduce(n, _ev, cx, _fx) {
            let inputs = cx.inputs;
            let t = cx.tune;
            n.tick = n.tick.wrapping_add(1);
            // The mover's body is a PURE function of the new tick, re-derived before anything reads
            // terrain this frame (the ink snapshot below, every Soup::collect). Never integrated, so
            // rollback re-simulation can't drift it.
            stage::fixtures::drive_mover(n);
            maybe_spawn_item(n, t);
            // helm: who's piloting the parked ship this frame (c-stick aims, attack thrusts -- only the
            // fighter SEATED at the station, `Fighter.station >= 0`; plans/lovers-ship.md "v3
            // simplification"). Runs before the FSM phase so the exhaust below sees this frame's steering.
            steer_from_riders(n, cx.np, inputs);
        }
    }
}

crate::slice! {
    /// Fsm: each fighter's FSM scans its raw input and emits a pure "next action" (Act).
    /// All FSMs run before any actuation (frame-start snapshots). lib.rs:197,209,216-242.
    pub Fsm for SimState {
        context: StepCx<'a>, event: (), output: Acts,
        reduce(n, _ev, cx, fx) {
            let inputs = cx.inputs;
            let t = cx.tune;
            // Per-fighter view of the config: physics + moveset resolve through each fighter's `char_id`
            // (its `CharSpec` roster row); the match knobs are shared. With every fighter on the same
            // row this array is `t` repeated -- the old single-`Tune` behavior. Hoisted onto `StepCx`
            // (computed once in `step()`, not rebuilt here) -- row 10 commit 3.
            let tunes = cx.tunes;
            let np = cx.np;

            // Phase 1: the input is never mutated. All FSMs run BEFORE any actuation so no fighter's
            // advance sees another's already-applied effect (preserves the old two-then-two ordering).
            let paths = n.paths; // ink is fixed for the frame (it mutates last, in update_paths); snapshot to collide
            let items = n.items; // read-only snapshot so the FSM decides pickup-vs-jab without borrowing
                                 // frame-start hurtbox snapshot (footstool targets); guarded bodies are already excluded
                                 // so the FSM never converts a jump into a footstool on an intangible foe.
            // The frame's effective FIGHTER blast rect, resolved once from this ink snapshot so every
            // fighter's KO check this tick sees the same rect. `None` is `ZoneMode::Off` (fighters never
            // KO at the edges). Items are UNAFFECTED by `zone_mode` -- they always despawn off the static
            // `out_of_bounds` frame (item.rs's despawn sites), never this rect.
            let zone: Option<ZoneRect> = match t.zone_mode {
                ZoneMode::Off => None,
                ZoneMode::Static => Some(ZoneRect::STATIC),
                ZoneMode::InkExtends => Some(ZoneRect::extended(ink_blast_zone(&paths, &n.nodes))),
            };
            let mut bodies = [None; MAX_PLAYERS];
            for p in 0..np {
                let f = &n.fighters[p];
                if f.invuln == 0 && !f.intangible {
                    bodies[p] = Some(hurtbox(f));
                }
            }
            // The per-fighter loop now lives in `Each::reduce`: run `FighterSlice` once per live slot
            // against the snapshot provider, collect the `Acts`. Simultaneous semantics -- every slot
            // sees the same frame-start `bodies`/`items`/`paths`, self slot `None`d per slot.
            let each_cx = FsmEachCx {
                inputs,
                tunes,
                items: &items,
                paths: &paths,
                nodes: &n.nodes,
                bodies: &bodies,
                zone,
                live: np,
            };
            <Each<FighterSlice, FsmEachCx<'_>, MAX_PLAYERS> as Slice>::reduce(
                &mut n.fighters,
                (),
                each_cx,
                fx,
            )
        }
    }
}

crate::slice! {
    /// Actuate: actuate in handle order (spawns bolts / drops / picks up on the shared item array).
    /// Only `Fsm` produces `Acts`, so composing `Actuate` before `Fsm` is ill-typed. lib.rs:244-246.
    pub Actuate for SimState {
        context: StepCx<'a>, event: Acts, output: (),
        reduce(n, acts, cx, _fx) {
            let tunes = cx.tunes; // hoisted onto StepCx, computed once in step() (row 10 commit 3)
            let np = cx.np;
            // Phase 2: actuate in handle order.
            for p in 0..np {
                apply_act(n, p, acts[p], &tunes[p]);
            }
        }
    }
}

crate::slice! {
    /// StrikeInk: melee hitboxes sweep the drawn strokes on their first active frame, then clank.
    /// lib.rs:247-283.
    pub StrikeInk for SimState {
        context: StepCx<'a>, event: (), output: (),
        reduce(n, _ev, cx, _fx) {
            let tunes = cx.tunes; // hoisted onto StepCx, computed once in step() (row 10 commit 3)
            let np = cx.np;
            // Melee strikes ink: each hitbox sweeps the drawn strokes ON ITS FIRST ACTIVE FRAME only —
            // ink has no per-victim re-hit grid, so the start frame is the one-hit-per-box-per-swing gate.
            // Runs before the fighter pass so this tick's fresh hitlag can't freeze `frame` at `start`
            // and re-trigger; a fighter mid-hitlag from an earlier hit is skipped outright.
            for p in 0..np {
                let f = n.fighters[p];
                if f.hitlag > 0 {
                    continue;
                }
                let Some(atk) = attack_for(&tunes[p], f.state) else {
                    continue;
                };
                for hb in atk.live_boxes() {
                    if f.frame != hb.start {
                        continue;
                    }
                    let (hc, hr) = hitbox_center(&f, hb);
                    stage::strike_ink(
                        &mut n.paths,
                        hc,
                        hr,
                        hb,
                        hb.damage * charge_mult(&f, &tunes[p]),
                        f.facing,
                        &n.nodes,
                        &tunes[p],
                    );
                }
            }
            // Clank: two live non-transcendent hitboxes meeting cancels moves before any hit resolves.
            // Within `clank_diff` % both rebound; past it only the weaker move does (the stronger one's
            // box stays live for the combat pass below). Aerials/projectiles author `transcendent`
            // boxes, so only grounded melee trades — the Melee priority rule.
            for a in 0..np {
                for b in (a + 1)..np {
                    resolve_clank(n, a, b, &tunes[a], &tunes[b]);
                }
            }
        }
    }
}

crate::slice! {
    /// Combat: pairwise combat + grabs over every ordered (attacker, victim) pair, then item
    /// strikes and the booster blast. Live ordered-pair mutation (port priority). lib.rs:284-316.
    pub Combat for SimState {
        context: StepCx<'a>, event: (), output: (),
        reduce(n, _ev, cx, _fx) {
            let inputs = cx.inputs;
            let t = cx.tune;
            let tunes = cx.tunes; // hoisted onto StepCx, computed once in step() (row 10 commit 3)
            let np = cx.np;
            // Phase 3: pairwise combat + grabs over every ordered (attacker, victim) pair. The victim's
            // stick this frame feeds trajectory DI, so each call passes the defender's aim. `pair_mut`
            // borrows the two distinct fighters at once (generalizes the old hand split).
            for a in 0..np {
                for b in 0..np {
                    if a == b {
                        continue;
                    }
                    let aim_b = Vector2::new(inputs[b].dir, inputs[b].aim_y);
                    let (fa, fb) = pair_mut(&mut n.fighters, a, b);
                    resolve_combat(fa, b, fb, aim_b, &tunes[a], &tunes[b]); // a attacks b (victim b DIs)
                }
            }
            // Items are strikeable bodies (Strikeable, plans/trait-math.md): after fighter-vs-fighter
            // combat, live hitboxes chip item hp + knock items back; a 0-hp item despawns.
            items::hurt::item_strikes(n, np, tunes);
            for a in 0..np {
                for b in 0..np {
                    if a == b {
                        continue;
                    }
                    // a grabs b: the grab kit + pummel + throw are the grabber's character. A Falcon up-B
                    // command grab returns the explosion position on its boom frame; paint the fireball there.
                    let boom = {
                        let (fa, fb) = pair_mut(&mut n.fighters, a, b);
                        resolve_grab(fa, fb, a as i8, b as i8, inputs[a], inputs[b], &tunes[a])
                    };
                    if let Some(pos) = boom {
                        push_fx(n, FxKind::Fire, pos); // dedicated cheesy fireball draw
                    }
                }
            }
            booster_blast(n, t); // the ship's engine exhaust is a knockback volume
        }
    }
}

crate::slice! {
    /// Items: move bolts, follow held guns, resolve bolt hits. lib.rs:318.
    pub Items for SimState {
        context: StepCx<'a>, event: (), output: (),
        reduce(n, _ev, cx, fx) {
            let t = cx.tune;
            crate::v1::fighters::tick_badge_meters(n); // Phase C1: meter maintenance, fighter-owned now
            // Phase C2: update_items emits fighter effects as an ItemActs batch instead of writing
            // fighters inline; ApplyItemActs (in acts.rs) is the ONE place they land.
            let item_acts = update_items(n, t);
            <ApplyItemActs as Slice>::reduce(n, item_acts, t, fx);
            crate::v1::terrain_cells::detach_depleted(n);
        }
    }
}

crate::slice! {
    /// Ink: integrate traveling ink, billiard impulses, repin mounts, ink hazards, prune, lay/extend.
    /// lib.rs:319-330.
    // parity(v1-ink-phase-order): every tick integrates ink, resolves ordered billiards, repins riders, applies item and fighter hazards, prunes, then authors and decays paths
    pub Ink for SimState {
        context: StepCx<'a>, event: (), output: (),
        reduce(n, _ev, cx, _fx) {
            let inputs = cx.inputs;
            let t = cx.tune;
            let np = cx.np; // hoisted onto StepCx, computed once in step() (row 10 commit 3)
            let snap = n.paths; // frame-start ink snapshot: traveling paths stack onto STILL ink as-of-now
            for i in 0..stage::MAX_DRAWN {
                let mut p = n.paths[i];
                // `&snap` (frame-start heads) gates OTHERS by frame-start `traveling()`, so a piece that
                // settles earlier this loop is still skipped as a stack target -- geometry reads off the
                // live pool are byte-identical to the snapshot there (`world_pt` is invariant across the
                // settle `bake_rotation`), so passing the live `n.nodes` reproduces the old full-copy snap.
                stage::integrate_ink(&mut p, &snap, i, &mut n.nodes, &mut n.free, t);
                n.paths[i] = p;
            }
            stage::resolve_ink_billiard(n, t); // traveling↔ink impulse: momentum, spin, the un-lock chain
            station::repin_mounted(n); // hull pos/rot final: mounted consoles follow the flying ship
            repin_ink_riders(n, &snap); // grounded/clinging riders re-pin to the same true delta (row 4)
            ink_hits_items(n, t); // flying ink pops bombs, ricochets bolts, shoves ground items
            ink_hits_fighters(n, np, t); // a flying piece is a live hazard — its maker included
            stage::prune_outside(n); // still ink that settled outside the blast zone dies
            stage::update_paths(n, inputs, t); // lay/extend/finalize drawn ink, decay old nodes
        }
    }
}

/// Rider-follow re-pin (plans/ship-containment.md row 4, direction A; now the first migrated
/// caller of the `anchor` primitive, plans/architecture-debt.md #1). `Fsm` (several phases
/// back now) carried each grounded/clinging rider by the ridden path's velocity as of the
/// FRAME-START snapshot -- one linear add, applied once. Whatever `integrate_ink` +
/// `resolve_ink_billiard` do to that same path for the rest of THIS tick (the hull's own
/// zero-g gravity solve, a billiard impulse, the hard bounce off the invincible blast frame)
/// is not reflected in that add; `repin_mounted` got this exact fix for the mounted CONSOLE
/// (station.rs's doc), riders never did. `body::anchor_pin` does the correction: `host.pos_now
/// - host.pos_snapshot` is how far the ridden path ACTUALLY moved this tick end-to-end (`snap`
/// being the same pre-integration snapshot this phase already took for the `integrate_ink`
/// loop above); `stale_carry` (`path_surface_vel(&snap[slot])`, read off that SAME snapshot)
/// is what the FSM already applied.
///
/// The two ride mechanics apply that stale carry differently, so `carry_applied` (what
/// `anchor_pin` subtracts back out) is NOT symmetric across x/y:
/// - Wall cling (`za_warudo.rs`'s `n.pos += wall_ride`) adds the full stale carry to BOTH axes
///   directly, nothing overwrites it -- `carry_applied` is the full `stale_carry`.
/// - Grounded-on-ink (`za_warudo.rs`'s ground branch) only ever ADDS the x carry; y is instead
///   RE-DERIVED every tick from `ink_floor_y_near`'s stale (frame-start) geometry -- the ride
///   carry never actually lands in y, it only widens the branch's own continuity gate
///   (direction B). Since the hull never rotates (`spin_scale` 0, `rot` stays 0), that
///   stale-geometry y is exactly "the surface's true CURRENT y at this x, minus this tick's own
///   translation" -- so y's full catch-up is the real delta's y alone; feeding `stale_carry.y`
///   into `carry_applied` (never applied in the first place) would overcorrect. `carry_applied`
///   is `(stale_carry.x, 0.0)`: x still nets `real_delta.x - stale_carry.x` same as the wall
///   case, y passes through untouched.
///
/// `anchor_pin`'s stale-ref scrub is what skips a rider whose ridden path died THIS tick
/// (`prune_outside`/expiry, `host.active` false: `InkPath::EMPTY`'s `pos` is the origin, not a
/// position worth snapping to) or is a KINEMATIC baked fixture (the mover: `mass == 0`, so
/// `host.traveling` (`snap[slot].traveling()`) reads false even while `vel` is nonzero) --
/// `drive_mover` writes its `pos` DIRECTLY from `SimState.tick` in the `Tick` phase, before the
/// FSM even runs, so the FSM's carry already used THIS tick's true final position; there is no
/// staleness to correct, and the real delta would read a spurious ZERO there (`integrate_ink`
/// never touches a non-`traveling` path, so `n.paths[slot].pos == snap[slot].pos` always) against
/// a genuinely nonzero `stale_carry`, which would cancel out the mover's legitimate carry instead
/// of leaving it alone. `snap[slot].traveling()` is the same `mass > 0 && vel != 0` gate
/// `integrate_ink` itself uses, so this pass only ever touches paths that phase can actually move.
///
/// Wall riding gates on `airborne(state) && wall_touch > 0`, not the bare `wall_ink` field:
/// `wall_ink` is never reset on landing (fighter.rs's own doc calls it "meaningful while
/// wall_touch > 0; stale otherwise"), so a grounded fighter can carry a stale wall slot index
/// from an earlier airborne touch. Without the `airborne` gate this pass would apply BOTH the
/// ground-ink correction (the branch actually riding this tick) and a leftover wall correction
/// against whatever that stale index still points at -- double-application. `ground_ink >= 0`
/// is checked first and is always exclusive with the wall branch below (a fighter is grounded
/// XOR clinging, never both), so only one correction ever lands.
// parity(v1-ink-moving-rider-carry): grounded and clinging riders are corrected by the ridden path's true post-bounce transform delta without double applying stale carry
fn repin_ink_riders(n: &mut SimState, snap: &[InkPath; MAX_DRAWN]) {
    for p in 0..(n.active as usize) {
        let f = n.fighters[p]; // Fighter is Copy: reads below never alias the write at the end
        let grounded = f.on_ink();
        let slot = if grounded {
            f.ground_ink as usize
        } else if airborne(f.state) && f.wall_touch > 0 && f.wall_ink >= 0 {
            f.wall_ink as usize
        } else {
            continue;
        };
        let stale_carry = path_surface_vel(&snap[slot]);
        // Grounded specials pin against the old hull without applying its horizontal carry.
        let carry_applied = if grounded && f.grounded() && crate::v1::is_special(f.state) {
            Vector2::ZERO
        } else if grounded {
            Vector2::new(stale_carry.x, 0.0)
        } else {
            stale_carry
        };
        let host = AnchorHost {
            pos_snapshot: snap[slot].pos,
            pos_now: n.paths[slot].pos,
            active: n.paths[slot].active(),
            traveling: snap[slot].traveling(),
        };
        n.fighters[p].pos = anchor_pin(f.pos, host, carry_applied);
    }
}

/// The whole step, order-as-type. Composing these in any other order fails to compile
/// (the adjacent `Output = Event` constraint the tuple macro preserves).
pub type Step = (Tick, Fsm, Actuate, StrikeInk, Combat, Items, Ink);
const _: () = assert!(core::mem::size_of::<Step>() == 0);
