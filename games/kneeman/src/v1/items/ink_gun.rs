//! InkGun: drawn-shot gun (mod-api.md Tier 0 per-kind file). C-stick aims the anchor,
//! hold attack to draw a shape riding the hand, release to fire it as an impulsed ink
//! body (plans/body-bus.md step 8). No per-tick behavior of its own here -- the anchored
//! draw/fire cycle runs in `stage::update_paths`; `update_items`'s generic held-tool arms
//! cover follow/settle/unload via `spec()` alone (same `Attach::Hand` family as the pen).

use crate::v1::items::behavior::{Attach, ItemBehavior};

// parity(v1-ink-gun-anchored-shot): the held ink gun aims an anchored loop, keeps that loop attached while drawing, then releases it as one traveling ink body
pub(crate) struct InkGunKind;

impl ItemBehavior for InkGunKind {
    fn aims(&self) -> bool {
        true
    }
    fn draws(&self) -> bool {
        true
    }
    fn attach(&self) -> Attach {
        Attach::Hand
    }
    fn despawn_when_spent(&self) -> bool {
        true // an idle empty ink gun unloads, same as the pen
    }
    fn catchable(&self) -> bool {
        true
    }
}
