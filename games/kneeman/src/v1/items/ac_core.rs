//! AcCore: touch-attaches the Armored Core overlay (plans/ac-overlay.md; mod-api.md Tier
//! 0 per-kind file). Falls/settles like a badge, but attach is TOUCH: the first hurtbox
//! that overlaps it (falling or resting) gets body-replaced, rolling the arm weapon off
//! the sim LCG. Ghosts (i-framed/intangible) don't get drafted.

use crate::v1::item::{ITEM_R, item_landing};
use crate::v1::items::behavior::{ItemBehavior, ItemCx, ItemFx};
use crate::v1::{Badge, DT, Item, Vector2, geo, hurtbox, out_of_bounds, roll_rng};

pub(crate) struct AcCoreKind;

// Every classifier stays the trait default (falls + settles like a badge, `Attach::None`):
// AcCore's attach is TOUCH, not `pickup_item` reach or a badge-classifier grant, so it never
// overrides `attach()` -- see `Attach::Badge`'s doc comment for why that stays `None` here.
impl ItemBehavior for AcCoreKind {
    fn on_tick(&self, mut it: Item, cx: &mut ItemCx) -> (Item, ItemFx) {
        let mut p = it.pos;
        let mut v = it.vel;
        p += v * DT;
        if let Some(h) = item_landing(&it, p, v, cx.soup) {
            p.y = h.y;
            v = Vector2::ZERO;
        } else {
            v.y += cx.t.gravity * DT;
        }
        it.pos = p;
        it.vel = v;
        // static frame only (item rule, not `Tune::zone_mode`); reuses the plasma-round config's
        // zone_exempt flag, matching `item_zone_exempt`'s AcCore mapping.
        if out_of_bounds(p) && !cx.t.plasma.zone_exempt {
            return (Item::EMPTY, ItemFx::Despawn);
        }
        for fi in 0..cx.np {
            let f = &cx.n.fighters[fi];
            if f.invuln > 0 || f.intangible {
                continue; // ghosts don't get drafted
            }
            let (bc, br) = hurtbox(f);
            if geo::circles_touch(p, ITEM_R + 6.0, bc, br) {
                let roll = (roll_rng(cx.n) % crate::v1::ac::ARM_WEAPONS as u64) as u8;
                return (
                    Item::EMPTY,
                    ItemFx::Attach {
                        fighter: fi,
                        badge: Badge::AcCore,
                        arm: roll,
                        fx_at: bc,
                    },
                );
            }
        }
        (it, ItemFx::None)
    }
}
