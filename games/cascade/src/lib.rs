//! Rust-authored property declarations, ordered by layer, specificity, and source order.
//! Selectors must depend only on their immutable arguments for deterministic replay.
//! The first receipt supplies one typed gravity-scale column.
#![forbid(unsafe_code)]

#[path = "1_rules.rs"]
mod rules;
#[path = "0_types.rs"]
mod types;

pub use rules::Rules;
pub use types::{DebugFrame, DeclarationTrace, GravityScale, Property, PropertyId, GRAVITY_SCALE};
