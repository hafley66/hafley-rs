//! One syn parse and byte-offset table shared by Rust syntax producers.

use super::call_metadata_rows::build_line_starts;

pub struct RustSyntax {
    pub file: syn::File,
    pub line_starts: Vec<u32>,
}

pub fn parse_rust_syntax(source: &str) -> Result<RustSyntax, syn::Error> {
    let file = syn::parse_file(source)?;
    let line_starts = build_line_starts(source);
    Ok(RustSyntax { file, line_starts })
}
