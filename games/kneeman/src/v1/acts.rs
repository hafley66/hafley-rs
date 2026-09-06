//! Actuate one fighter's emitted `Act` into the `SimState`: the only place item intents become
//! item effects, plus the footstool cross-fighter effect. Split out of `lib.rs` (R5).

use crate::v1::combat::{Swing, strike};
use crate::v1::fighters::wear;
use crate::v1::items::act::{ItemAct, ItemActs, StrikeFollow};
use crate::v1::slice::{Never, Slice};
use crate::v1::state::Act;
use crate::v1::{
    Badge, SimState, Tune, ac, drop_item, fire_gun, pair_mut, pickup_item, push_fx, station,
    throw_item,
};

/// Actuate one fighter's emitted `Act` into the SimState. The only place item intents become item
/// effects; `reduce_next_state` decided WHAT to do (purely), this carries it out on the shared item array.
pub(crate) fn apply_act(n: &mut SimState, idx: usize, act: Act, t: &Tune) {
    match act {
        Act::None => {}
        Act::Fire { auto, aim, aim_y } => fire_gun(n, idx, auto, aim, aim_y, t),
        Act::Drop => drop_item(n, idx),
        Act::Throw { dir } => throw_item(n, idx, dir, t),
        Act::Pickup => pickup_item(n, idx, t),
        // node-laying needs the raw stick (cursor/ruler aim), which apply_act doesn't have — it runs
        // in `update_paths`. Act::Draw exists only so `reduce_next_state` suppressed the jab; nothing to do here.
        Act::Draw => {}
        Act::Footstool { victim } => footstool(n, idx, victim as usize, t),
        Act::ArmFire { aim } => ac::ac_fire(n, idx, aim, t),
        // occupy a ship station: the one-rider check + lock live in the mount seam module.
        Act::Occupy { station: s } => station::occupy(n, idx, s),
    }
}

/// Grant a fighter a badge bit. The one applier for both attach paths: `pickup_item`'s
/// reach-based grab (Wings) and `ItemAct::AttachBadge`'s touch-based AcCore attach (routed
/// through `apply_item_act` below). AcCore also arms its own fuel meter here
/// (plans/ac-ship-backlog.md item 1): `ac_gas` frames of wear before the badge auto-clears,
/// ticked in `fighters::tick_badge_meters`. Every OTHER badge arms the generic per-badge wear
/// table instead (`fighters::wear`, queue-2026-07-03 item 6) -- the default de-spawn-after-
/// pickup timeout, same tick site. Lives here, not `items/` (core-rx-refactor row 8: no item
/// file writes a fighter -- `items-no-fighter-mut.dl` is the hard-zero rail for that).
pub(crate) fn attach_badge(n: &mut SimState, fighter: usize, badge: Badge) {
    n.fighters[fighter].badges |= badge as u8;
    if badge == Badge::AcCore {
        n.fighters[fighter].ac_gas = ac::AC_GAS_FRAMES;
    }
    wear::arm(&mut n.fighters[fighter], badge);
}

/// The ONE applier that turns an `ItemAct` descriptor into a fighter effect (plans/core-rx-refactor.md
/// section 4). Lives here, beside `apply_act`, OUTSIDE `items/`, so no item file writes a fighter.
/// The catch/attach acts are batched (applied by `ApplyItemActs` after the item pass); `Strike` is
/// applied immediately at its emit site so strike timing -- and the catch gate's read of the
/// hitstun/hitlag a strike sets -- stays bit-identical.
pub(crate) fn apply_item_act(n: &mut SimState, act: ItemAct, t: &Tune) {
    match act {
        ItemAct::None => {}
        ItemAct::Caught { fighter, item } => {
            let f = &mut n.fighters[fighter as usize];
            f.holding = item;
            f.pickup_hold = true; // same release-before-fire latch as a ground pickup
            f.catch_win = 0; // latch spent: one press can't catch twice
        }
        ItemAct::AttachBadge {
            fighter,
            badge,
            arm,
        } => {
            attach_badge(n, fighter as usize, badge);
            let f = &mut n.fighters[fighter as usize];
            f.arm = arm;
            f.arm_cd = 0;
            f.qb_cd = 0;
        }
        ItemAct::Strike {
            fighter,
            hb,
            dmg,
            kb_scale,
            hitlag_bonus,
            interrupt,
            aim,
            from,
            follow,
        } => {
            let hit = strike(
                &mut n.fighters[fighter as usize],
                &Swing {
                    hb: &hb,
                    dmg,
                    kb_scale,
                    hitlag_bonus,
                    interrupt,
                    aim,
                },
                from,
                t,
            );
            match follow {
                StrikeFollow::None => {}
                StrikeFollow::ArmHits => {
                    if hit.is_some() {
                        n.fighters[fighter as usize].arm_hits();
                    }
                }
                StrikeFollow::Fx { kind, at } => {
                    if hit.is_some() {
                        push_fx(n, kind, at);
                    }
                }
            }
        }
    }
}

/// The batched applier for the deferred (catch/attach) intents `update_items` collected. State =
/// SimState, Event = the batch, Context = the tune (strikes need it; catch/attach ignore it).
pub struct ApplyItemActs;
impl Slice for ApplyItemActs {
    type Context<'a> = &'a Tune;
    type State = SimState;
    type Event = ItemActs;
    type Output = ();
    type Effect = Never;
    #[inline(always)]
    fn reduce(n: &mut SimState, acts: ItemActs, t: &Tune, _fx: &mut impl FnMut(Never)) {
        for act in acts.as_slice() {
            apply_item_act(n, *act, t);
        }
    }
}
const _: () = assert!(core::mem::size_of::<ApplyItemActs>() == 0);

/// Hop off another fighter's head: the jumper pops up (no air jump spent — the FSM never
/// consumed one), the victim staggers. Airborne victims get shoved down as well; grounded
/// ones just flinch in place. Bails if the victim turned guarded since the FSM's snapshot.
fn footstool(n: &mut SimState, idx: usize, victim: usize, t: &Tune) {
    if victim >= n.active as usize || victim == idx {
        return;
    }
    let (fa, fv) = pair_mut(&mut n.fighters, idx, victim);
    if fv.invuln > 0 || fv.intangible {
        return;
    }
    fa.vel.y = t.footstool_v;
    fa.fast_falling = false;
    if !fv.grounded() && !fv.on_ink() {
        fv.vel.y = fv.vel.y.max(t.footstool_spike); // shoved down, never popped up
        fv.hitstun = fv.hitstun.max(t.footstool_stun);
        fv.tumble = false; // a footstool never knocks down on landing
    } else {
        fv.hitstun = fv.hitstun.max(t.footstool_stun / 2);
    }
}
