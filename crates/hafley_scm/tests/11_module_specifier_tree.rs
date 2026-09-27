#![cfg(feature = "rust_syn")]

use hafley_scm::lang::rust::{
    build_line_starts, module_specifier_rows, module_specifier_rows_from_tree,
};

#[test]
fn tree_module_specifiers_match_syn_for_import_shapes() {
    let source = r#"
use crate::outer::{Thing, inner as renamed, self, *};
pub use super::Api;
mod inline { use super::Thing; use super::*; }
#[path = "elsewhere.rs"] mod external;
"#;
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("Rust grammar is valid");
    let tree = parser.parse(source, None).expect("Rust parses");

    let rows = module_specifier_rows_from_tree(&tree, source.as_bytes());
    let parsed = syn::parse_file(source).expect("Rust parses");
    assert_eq!(
        rows,
        module_specifier_rows(&parsed, &build_line_starts(source)),
        "{}",
        tree.root_node().to_sexp()
    );
}
