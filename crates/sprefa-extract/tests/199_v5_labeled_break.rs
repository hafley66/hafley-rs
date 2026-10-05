use sprefa_extract::flatten_jsonl;

pub(super) const SOURCE: &str = "fn produce() -> i64 { 1 }\nfn consume(value: i64) {}\nfn orchestrate(flag: bool) {\n    let outcome = 'outer: loop {\n        loop {\n            break 'outer produce();\n        }\n    };\n    consume(outcome);\n}\n";

#[test]
fn labeled_break_cst_and_whole_flow() {
    let _snapshots = super::v5_support::snapshots();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(SOURCE, None).unwrap();
    insta::assert_snapshot!(
        "v5_parity__labeled_break__labeled_break_cst",
        tree.root_node().to_sexp()
    );
    let out = super::v5_support::facts("src/lib.rs", SOURCE.as_bytes(), false);
    let rows = flatten_jsonl(&out).join("\n");
    insta::assert_snapshot!(
        "v5_parity__labeled_break__labeled_break_cst_and_whole_flow",
        rows
    );
}
