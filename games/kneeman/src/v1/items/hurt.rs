//! Items as strikeable bodies -- the FIRST generic solver over the Strikeable capability
//! (plans/trait-math.md), falsifying the trait-math bet with one row (plans/item-strikeable.md).
//! A ground item is a point-circle hurt body: an attacker's live hitbox lands the shared
//! `strike` ritual on it, hp runs out, the slot dies. This is the SECOND impl of combat.rs's
//! existing `PunchableFace` seam (Fighter is the first, unchanged); no parallel trait. Both the
//! per-kind const rows (`item_hp`/`item_hurt_r`) and the resolution pass (`item_strikes`, run by
//! `step` after fighter-vs-fighter combat) live here so item.rs / za_warudo.rs stay under their
//! .dl line-ratchet caps.

use crate::v1::combat::{Aim, Guard, Launch, PunchableFace, Swing, strike};
use crate::v1::geo::{self, Iso, Shape};
use crate::v1::moves::{attack_for, charge_mult, hitbox_center};
use crate::v1::{Item, ItemKind, MAX_ITEMS, MAX_PLAYERS, SimState, Tune, Vector2};

/// Per-kind hit points. `0.0` = this kind opts out of being strikeable and `item_strikes` skips
/// it: stations are inert fixtures, badges attach on touch (never a ground pickup you punch),
/// projectiles are transient hit-effects with their own resolution, `None` is the empty slot.
/// Ground pickups (guns + pens) are the strikeable bodies for v1. A `'static` const row, never
/// serialized. Reuses the held-tool kind set rather than restating it, so a new gun is strikeable
/// automatically.
pub(crate) fn item_hp(kind: ItemKind) -> f32 {
    match kind {
        ItemKind::LaserGun
        | ItemKind::BobGun
        | ItemKind::Pen
        | ItemKind::TetrisGun
        | ItemKind::InkGun
        | ItemKind::TetrisDropper
        | ItemKind::TerrainCell => 20.0,
        _ => 0.0,
    }
}

/// Per-kind hurt-circle radius (world space): the item is a point ball on its own `pos`, sized on
/// the order of the item body / pickup dims (`item.rs`'s `ITEM_BODY_R` is 16). A const row; every
/// strikeable kind shares it for v1, kept a fn so a fat kind can widen its own circle later.
pub(crate) fn item_hurt_r(_kind: ItemKind) -> f32 {
    18.0
}

/// The knockback formula's weight term (`knockback_units`'s `w`) for an item. Light next to a
/// fighter (weights run 104..200), so a clean hit sends it flying. Not a per-instance stat, so it
/// stays a const, never an `Item` field.
const ITEM_HEFT: f32 = 50.0;

/// Item is the second `PunchableFace` -- a ZST-free view over the item's OWN fields, exactly the
/// trait rule combat.rs states (every method is a field/table read). No hitlag/hitstun/tumble/
/// shield state exists on an item, so those Launch outputs are simply ignored in `absorb`.
impl PunchableFace for Item {
    fn bound(&self) -> (Vector2, f32) {
        (self.pos, item_hurt_r(self.kind))
    }
    fn hurt_shapes(&self, out: &mut impl FnMut(Iso, Shape)) {
        out(
            Iso::at(self.pos),
            Shape::Ball {
                r: item_hurt_r(self.kind),
            },
        );
    }
    fn percent(&self) -> f32 {
        0.0 // items carry no rage/percent accumulator: knockback comes from the hit alone.
    }
    fn heft(&self, _t: &Tune) -> f32 {
        ITEM_HEFT
    }
    fn guard(&self) -> Guard {
        Guard::Open // items never shield (v1): no GuardSet, always Open, so `strike` never whiffs.
    }
    fn absorb(&mut self, l: Launch, _contact: Vector2, _t: &Tune) {
        // Strikeable's whole policy for an item: chip hp, take the launch as raw velocity. Items
        // are knockbackable -- the launch IS the vel write. `hitstun`/`tumble`/`hitlag`/`interrupt`
        // have no meaning here, so those Launch fields are ignored (no FSM to drive).
        self.hp -= l.dmg;
        self.vel = l.vel;
    }
}

/// Resolve item strikes for the frame: every live attacker hitbox vs every strikeable item.
/// Runs AFTER fighter-vs-fighter combat (plans/item-strikeable.md solver order), attackers in
/// fighter handle order, items in slot order -- deterministic, no intent dam (items don't
/// cross-actuate this frame). Uniqueness: one strike per box per swing, gated on the box's FIRST
/// active frame (`f.frame == hb.start`) -- the SAME one-hit gate the melee-strikes-ink pass in
/// `lib::step` uses. Items get no per-victim re-hit grid: `Fighter.hit_cd` is `[[_; MAX_PLAYERS]; _]`,
/// too narrow to index the 128 item slots, and this first-frame gate needs none.
pub(crate) fn item_strikes(n: &mut SimState, np: usize, tunes: &[Tune; MAX_PLAYERS]) {
    for a in 0..np {
        let f = n.fighters[a];
        if f.hitlag > 0 {
            continue; // frozen on impact: `frame` isn't advancing, don't re-fire the start gate
        }
        let Some(atk) = attack_for(&tunes[a], f.state, f.special_started_air) else {
            continue; // not in an attacking state this frame
        };
        for hb in atk.live_boxes() {
            if f.frame != hb.start {
                continue; // one strike per box per swing (the ink-strike gate)
            }
            let (hc, hr) = hitbox_center(&f, hb);
            let dmg = hb.damage * charge_mult(&f, &tunes[a]); // banked smash charge pays out here too
            for k in 0..MAX_ITEMS {
                let it = n.items[k];
                if !it.active() || it.mount >= 0 {
                    continue; // empty slot, or a mounted station (inert fixture, skips its tick too)
                }
                if it.owner == a as i8 {
                    continue; // don't punch your own held/thrown tool
                }
                if item_hp(it.kind) == 0.0 {
                    continue; // this kind opted out of being strikeable
                }
                if !geo::circles_touch(hc, hr, it.pos, item_hurt_r(it.kind)) {
                    continue; // no overlap
                }
                strike(
                    &mut n.items[k],
                    &Swing {
                        hb,
                        dmg,
                        kb_scale: 1.0,
                        hitlag_bonus: 0, // items don't freeze the attacker (no item hitlag, v1)
                        interrupt: false,
                        aim: Aim::Angle {
                            deg: hb.angle,
                            facing: f.facing,
                        },
                    },
                    it.pos,
                    &tunes[a],
                );
                if n.items[k].hp <= 0.0 {
                    // DeathFx v1 = Despawn (plans/trait-math.md `DeathFx{Despawn, Explode, ...}`):
                    // clear the slot the same way the existing quiet-despawn sites do. Explode-on-
                    // death (Bomb) is OUT of scope -- see DeathFx in plans/trait-math.md.
                    n.items[k] = Item::EMPTY;
                }
            }
        }
    }
}
