use sprefa_extract::{dispatch, flatten_jsonl, FamilyMask};

const SOURCE: &str = "fn produce() -> i64 { 1 }\nfn consume(value: i64) {}\nfn orchestrate(flag: bool) {\n    let outcome = 'outer: loop {\n        loop {\n            break 'outer produce();\n        }\n    };\n    consume(outcome);\n}\n";

#[test]
fn labeled_break_cst_and_whole_flow() {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(SOURCE, None).unwrap();
    insta::assert_snapshot!("labeled_break_cst", tree.root_node().to_sexp());
    let out = dispatch(
        "src/lib.rs",
        SOURCE.as_bytes(),
        FamilyMask {
            df: true,
            cst: false,
            types: false,
            call: false,
            data: false,
        },
    )
    .unwrap();
    let rows = flatten_jsonl(&out).join("\n");
    insta::assert_snapshot!(rows);
}
