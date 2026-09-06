//! LaserGun (pickup weapon) + LaserBolt (its shot): mod-api.md Tier 0 per-kind file.
//! The gun itself has no per-tick behavior -- `update_items`'s generic held-tool arms
//! (follow the hand, settle on drop, arm on throw) cover every `Attach::Hand` kind, keyed
//! off `spec()` alone. The bolt is a straight burn shot (`straight_burn_tick`, shared with
//! `PlasmaBall`): dead-flat, weaker on auto-fire, fizzles past the stage's edge margin.

use crate::v1::combat::Aim;
use crate::v1::items::behavior::{Attach, ItemBehavior, ItemCx, ItemFx, straight_burn_tick};
use crate::v1::{Item, Land};

pub(crate) struct LaserGunKind;

impl ItemBehavior for LaserGunKind {
    fn aims(&self) -> bool {
        true
    }
    fn attach(&self) -> Attach {
        Attach::Hand
    }
    fn catchable(&self) -> bool {
        true
    }
}

pub(crate) struct LaserBoltKind;

impl ItemBehavior for LaserBoltKind {
    fn gravity(&self) -> bool {
        false
    }
    fn land(&self) -> Land {
        Land::Ignore
    }
    fn ricochet(&self) -> bool {
        true
    }
    fn is_projectile(&self) -> bool {
        true
    }

    fn on_tick(&self, it: Item, cx: &mut ItemCx) -> (Item, ItemFx) {
        // auto-fire bolts (gas == 1.0, the weak flag) hit softer -- the funny tax.
        let scale = if it.gas == 1.0 {
            cx.t.laser.autofire_dmg
        } else {
            1.0
        };
        let dmg = cx.t.laser.hit.damage * scale;
        let aim = Aim::Angle {
            deg: cx.t.laser.hit.angle,
            facing: it.facing,
        };
        let cfg = cx.t.laser;
        straight_burn_tick(it, cx, &cfg, true, aim, 2, dmg, None)
    }
}
