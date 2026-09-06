//! Pen: drawing tool (mod-api.md Tier 0 per-kind file). No per-tick behavior of its own
//! -- `update_items`'s generic held-tool arms (follow the hand, settle, unload when
//! empty) cover every `Attach::Hand` kind via `spec()` alone; node-laying itself runs in
//! `stage::update_paths` (the tool is `Item.tool`, not a kind-specific tick).

use crate::v1::items::behavior::{Attach, ItemBehavior};

// parity(v1-pen-draw-lifecycle): the held pen authors a toggled point trail through the shared path system, consumes its ink budget, and unloads only after an idle spent state
pub(crate) struct PenKind;

impl ItemBehavior for PenKind {
    fn draws(&self) -> bool {
        true
    }
    fn attach(&self) -> Attach {
        Attach::Hand
    }
    fn despawn_when_spent(&self) -> bool {
        true // an idle empty pen unloads (unlike a spent gun)
    }
    fn catchable(&self) -> bool {
        true
    }
}
