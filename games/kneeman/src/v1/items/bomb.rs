//! BobGun (pickup weapon) + Bomb (its lob): mod-api.md Tier 0 per-kind file. The gun
//! has no per-tick behavior of its own -- see `laser_gun.rs`'s header. The bomb is an
//! arcing blast round (`blast_round_tick`, shared with `Rocket`): gravity drags the lob
//! into a parabola, detonating on fuse-out, any floor, or grazing a non-owner.

use crate::v1::items::behavior::{
    Attach, ItemBehavior, ItemCx, ItemFx, blast_round_tick, no_extra_boom,
};
use crate::v1::{Item, Land};

pub(crate) struct BobGunKind;

impl ItemBehavior for BobGunKind {
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

pub(crate) struct BombKind;

impl ItemBehavior for BombKind {
    fn land(&self) -> Land {
        Land::Detonate
    }
    fn boom_on_hit(&self) -> bool {
        true
    }
    fn is_projectile(&self) -> bool {
        true
    }

    fn on_tick(&self, it: Item, cx: &mut ItemCx) -> (Item, ItemFx) {
        let gravity = cx.t.bomb.proj_gravity;
        blast_round_tick(it, cx, gravity, no_extra_boom)
    }
}
