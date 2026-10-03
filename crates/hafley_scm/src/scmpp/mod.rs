//! scm++: one nested query -> flat tree-sitter patterns + one SQL statement over capture/CST rows.
mod _0_types;
mod _1_parens;
mod _2_compile;
mod _3_lower;

pub use _0_types::*;
pub use _2_compile::{compile, first_scmpp_only, ROOT};
pub use _3_lower::{CAPTURE, DICT_PATH, DICT_TEXT, NODE};
