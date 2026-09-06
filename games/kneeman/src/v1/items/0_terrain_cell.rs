//! Detached terrain uses the existing hand-item lifecycle.
use super::behavior::{Attach, ItemBehavior};

pub(crate) struct TerrainCellKind;

impl ItemBehavior for TerrainCellKind {
    fn attach(&self) -> Attach {
        Attach::Hand
    }
    fn catchable(&self) -> bool {
        true
    }
}
