//! Rocket: a self-propelled bazooka round (mod-api.md Tier 0 per-kind file), AC-arm
//! ammo today (`ac::arm_spec`) and any future ground bazooka's shot tomorrow. Straight
//! flight (no gravity), pops on the shared floor sweep, a wall its flight line crosses,
//! fuse-out, or grazing a non-owner -- shares `Bomb`'s blast (`blast_round_tick`).

use crate::v1::body::sweep_walls;
use crate::v1::items::behavior::{ItemBehavior, ItemCx, ItemFx, blast_round_tick};
use crate::v1::{Item, Land, Vector2};

pub(crate) struct RocketKind;

impl ItemBehavior for RocketKind {
    fn gravity(&self) -> bool {
        false
    }
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
        blast_round_tick(it, cx, 0.0, wall_extra)
    }
}

/// The flight line crossing a wall face (the same swept-ECB walls fighters block on)
/// pops the round -- terrain along the SIDE, not just the shared floor-landing sweep.
fn wall_extra(pre: Vector2, post: Vector2, cx: &crate::v1::items::behavior::ItemCx) -> bool {
    sweep_walls(
        pre.x,
        Vector2::new(post.x, post.y + 8.0),
        8.0,
        8.0,
        cx.soup.surfs(),
    )
    .is_some()
}
