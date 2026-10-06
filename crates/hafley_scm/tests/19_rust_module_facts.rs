#![cfg(feature = "rust")]
use hafley_scm::read::lang::rust_module_facts::{
    rust_module_facts_from_parsed, rust_module_facts_from_tree,
};
#[test]
fn tree_module_facts_match_syn_across_pinned_rust_fixtures() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut pending = vec![
        root.join("../sprefa-extract/tests/fixtures/ratchet_soopy/src"),
        root.join("../sprefa-extract/tests/fixtures/type_ladder_scope/src"),
        root.join("../sprefa-extract/tests/fixtures/call_ladder/src"),
        root.join("../sprefa-extract/tests/fixtures/rust_visibility"),
    ];
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("Rust grammar loads");
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("fixture directory reads") {
            let path = entry.expect("fixture entry reads").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(std::ffi::OsStr::to_str) != Some("rs") {
                continue;
            }
            let source = std::fs::read(&path).expect("fixture source reads");
            let source_text = std::str::from_utf8(&source).expect("fixture source is UTF-8");
            let parsed =
                hafley_scm::lang::rust::parse_rust_file(source_text).expect("Syn parses fixture");
            let tree = parser
                .parse(&source, None)
                .expect("tree-sitter parses fixture");
            let syn_facts = rust_module_facts_from_parsed(&parsed);
            let tree_facts = rust_module_facts_from_tree(&tree, &source);
            assert_eq!(tree_facts, syn_facts, "{}", path.display());
        }
    }
}
