extern crate self as variant_reexport;

mod read;
mod types;

pub use types::*;

pub fn build(flag: bool) -> Result<(), QueryError> {
    if flag {
        return Err(QueryError::ScmOnly { pattern: 1, op: String::new() });
    }
    Ok(())
}
