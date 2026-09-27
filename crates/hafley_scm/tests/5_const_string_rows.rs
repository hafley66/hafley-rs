#![cfg(feature = "rust_syn")]

use hafley_scm::lang::rust::{build_line_starts, const_string_rows, const_string_rows_from_tree};

#[test]
fn item_string_consts_keep_order_spans_and_inline_module_depth() {
    let src = "const ROOT: &str = \"r\";\nmod inner { const CHILD: &str = \"c\"; }\nstruct S;\nimpl S { const SKIP: &str = \"x\"; }\n";
    let parsed = syn::parse_file(src).expect("Rust parses");
    let rows = const_string_rows(&parsed, &build_line_starts(src));
    let receipt: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|row| {
            (
                row.name.as_str(),
                row.value.as_str(),
                &src[row.range.start as usize..row.range.end as usize],
            )
        })
        .collect();
    assert_eq!(receipt, [("ROOT", "r", "ROOT"), ("CHILD", "c", "CHILD")]);
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("Rust grammar loads");
    let tree = parser.parse(src, None).expect("Rust tree parses");
    let tree_receipt: Vec<(String, String, &str)> =
        const_string_rows_from_tree(&tree, src.as_bytes())
            .into_iter()
            .map(|row| {
                (
                    row.name,
                    row.value,
                    &src[row.range.start as usize..row.range.end as usize],
                )
            })
            .collect();
    assert_eq!(
        tree_receipt,
        receipt
            .into_iter()
            .map(|(name, value, range)| (name.to_owned(), value.to_owned(), range))
            .collect::<Vec<_>>()
    );
}

#[test]
fn tree_string_consts_decode_raw_and_cooked_literals() {
    let src = "const COOKED: &str = \"line\\n\";\nconst RAW: &str = r#\"raw text\"#;\n";
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("Rust grammar loads");
    let tree = parser.parse(src, None).expect("Rust tree parses");
    let receipt: Vec<_> = const_string_rows_from_tree(&tree, src.as_bytes())
        .into_iter()
        .map(|row| (row.name, row.value))
        .collect();
    assert_eq!(
        receipt,
        [
            ("COOKED".to_owned(), "line\n".to_owned()),
            ("RAW".to_owned(), "raw text".to_owned())
        ]
    );
}
