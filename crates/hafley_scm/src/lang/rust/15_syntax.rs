//! The one syn parse every Rust span producer reads.

/// `syn::parse_file` with the BOM and shebang it strips blanked to spaces
/// instead, so every span's `byte_range()` is a byte offset into `source`.
pub fn parse_rust_file(source: &str) -> syn::Result<syn::File> {
    let file = syn::parse_file(source)?;
    let bom = if source.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    let prefix = bom + file.shebang.as_ref().map_or(0, String::len);
    if prefix == 0 {
        return Ok(file);
    }
    let mut blanked = syn::parse_file(&(" ".repeat(prefix) + &source[prefix..]))?;
    blanked.shebang = file.shebang;
    Ok(blanked)
}
