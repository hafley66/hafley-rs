//! WingsBadge: a passive character mod granted on pickup (mod-api.md Tier 0 per-kind
//! file). No hand slot, no drop, no throw -- consumed the instant `pickup_item` reaches
//! it, setting the fighter's badge bit. Ground physics only otherwise (falls, settles);
//! no per-tick behavior lives here.

use crate::v1::Badge;
use crate::v1::items::behavior::{Attach, ItemBehavior};

pub(crate) struct WingsBadgeKind;

impl ItemBehavior for WingsBadgeKind {
    fn attach(&self) -> Attach {
        Attach::Badge(Badge::Wings)
    }
}
