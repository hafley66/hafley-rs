#![cfg(feature = "rust_syn")]

use hafley_scm::lang::rust::{build_line_starts, const_string_rows};

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
}
