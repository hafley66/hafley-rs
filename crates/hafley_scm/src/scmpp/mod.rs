//! scm++: one nested query -> flat tree-sitter patterns + one SQL statement over capture/CST rows.
mod _0_types;
mod _1_parens;
mod _2_compile;
mod _3_lower;
mod _4_route;
mod _5_rows;
#[cfg(feature = "shared")]
mod _6_eval;

pub use _0_types::*;
pub use _2_compile::{compile, ROOT};
pub use _4_route::{route, Routed};
pub use _5_rows::{capture_rows, cst_rows, CaptureRow, CstRow};
#[cfg(feature = "shared")]
pub use _6_eval::{accepted, register_regexp, INDEXES};
