mod _0_ts_sibling_holds;
pub(crate) mod _1_ts_nth_child;
pub(crate) mod _2_ts_related_captures;
mod dispatch_by_direction;
mod ts_ancestor_holds;
mod ts_descendant_holds;

pub use dispatch_by_direction::holds;
