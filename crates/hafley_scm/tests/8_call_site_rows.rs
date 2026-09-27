#![cfg(feature = "rust_syn")]

use hafley_scm::lang::rust::{build_line_starts, call_site_rows, call_site_rows_from_tree};

#[test]
fn sites_const_initializers_and_cfg_classification_share_the_syn_walk() {
    let src = "const TOP: u8 = make();\n#[cfg(test)] fn only() { secret(); }\nfn live() { api::make(); value.run(); S { x: 1 }; E::Variant { x: 1 }; }\nmod inner { static N: u8 = make(); }\n";
    let parsed = syn::parse_file(src).expect("Rust parses");
    let rows = call_site_rows(&parsed, &build_line_starts(src), &[]);
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("Rust grammar loads");
    let tree = parser.parse(src, None).expect("Rust tree parses");
    assert_eq!(call_site_rows_from_tree(&tree, src.as_bytes(), &[]), rows);
    let sites: Vec<_> = rows
        .sites
        .iter()
        .map(|row| {
            (
                row.callee.as_str(),
                row.callee_path.as_deref(),
                row.cfg.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        sites,
        [
            ("make", None, None),
            ("secret", None, Some("test")),
            ("make", Some("api::make"), None),
            ("run", None, None),
            ("S", None, None),
            ("make", None, None),
        ]
    );
    assert_eq!(
        rows.const_inits
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["TOP", "N"]
    );
    assert_eq!(
        rows.test_only_calls,
        [("secret".to_string(), "test".to_string())]
    );
}
