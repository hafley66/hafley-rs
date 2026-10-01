#![cfg(feature = "rust_syn")]

use hafley_scm::lang::rust::{
    macro_invocation_rows_from_parsed, macro_invocation_rows_from_tree,
};

#[test]
fn tree_macro_invocation_rows_match_syn_across_rust_fixtures() {
    let fixture_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let roots = [
        fixture_root.join("../sprefa-extract/tests/fixtures/ratchet_soopy/src"),
        fixture_root.join("../sprefa-extract/tests/fixtures/type_ladder_scope/src"),
        fixture_root.join("../sprefa-extract/tests/fixtures/call_ladder/src"),
    ];
    let mut pending = roots.to_vec();
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(&path).expect("fixture directory reads") {
            let path = entry.expect("fixture entry reads").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(std::ffi::OsStr::to_str) != Some("rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("Rust fixture reads as UTF-8");
            let parsed = hafley_scm::lang::rust::parse_rust_file(&source).expect("Syn parses the Rust fixture");
            let syn_rows = macro_invocation_rows_from_parsed(&parsed);
            let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&language).expect("Rust grammar loads");
            let tree = parser
                .parse(&source, None)
                .expect("tree-sitter parses fixture");
            let tree_rows = macro_invocation_rows_from_tree(&tree, source.as_bytes());
            assert_eq!(
                tree_rows,
                syn_rows,
                "macro rows differ for {}",
                path.display()
            );
        }
    }
}
