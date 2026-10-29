//! scm++: one nested query -> flat tree-sitter patterns + one SQL statement over capture/CST rows.
mod _0_types;
mod _1_parens;
mod _2_compile;
mod _2a_siblings;
mod _3_lower;

pub use _0_types::*;
pub use _2_compile::{compile, compile_unsplit, first_scmpp_only, ROOT};
pub use _3_lower::{CAPTURE, DICT_PATH, DICT_TEXT, NODE};
