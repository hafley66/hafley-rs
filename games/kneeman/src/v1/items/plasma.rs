//! PlasmaBall: the AC energy cannon's round (mod-api.md Tier 0 per-kind file), any
//! future cannon's ammo too (`ac::arm_spec`). A straight burn shot (`straight_burn_tick`,
//! shared with `LaserBolt`): fat and slow, spent on the first body or ink it burns,
//! pops an explosion fx on a fighter connect.

use crate::v1::combat::Aim;
use crate::v1::items::behavior::{ItemBehavior, ItemCx, ItemFx, straight_burn_tick};
use crate::v1::{FxKind, Item, Land};

pub(crate) struct PlasmaBallKind;

impl ItemBehavior for PlasmaBallKind {
    fn gravity(&self) -> bool {
        false
    }
    fn land(&self) -> Land {
        Land::Ignore
    }
    fn is_projectile(&self) -> bool {
        true
    }

    fn on_tick(&self, it: Item, cx: &mut ItemCx) -> (Item, ItemFx) {
        let aim = Aim::Carry {
            vel: it.vel,
            up: 0.25,
        };
        let cfg = cx.t.plasma;
        let dmg = cfg.hit.damage;
        straight_burn_tick(it, cx, &cfg, false, aim, 3, dmg, Some(FxKind::Explosion))
    }
}
