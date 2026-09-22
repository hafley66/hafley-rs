#[path = "0_call_query.rs"]
mod call_query;

mod syn_macro_expansion_defs;

pub use call_query::RUST_CALL_QUERY;
pub use syn_macro_expansion_defs::{expand_file, Expanded};
