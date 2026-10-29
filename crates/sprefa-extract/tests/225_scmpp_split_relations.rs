//! A split level that also carries relations and cross-level conditions: every reference to a
//! moved capture pins its owner child. Oracle: the same program compiled unsplit.
use crate::t_195_scmpp_growth::{scmpp, sqlite};

use super::v5_support::snapshots;

const CASES: &[(&str, &str)] = &[
    // The report's shape: an ancestor relation on the split level whose inner `#eq?` reads a sibling capture.
    ("ancestor_eq_sibling", r#"((object (pair key: (property_identifier) @k) (pair) @b (pair) @c) (#has-ancestor? @b ((variable_declarator name: (identifier) @name) (#eq? @name @k))))"#),
    ("ancestor_eq_sibling_each", r#"((object (pair key: (property_identifier) @k) (pair) @b (pair) @c) (#has-ancestor? @c ((variable_declarator name: (identifier) @name) (#eq? @name @k)) rows: each))"#),
    ("descendant_from_child_each", r#"((object (pair) @a (pair) @b (pair value: (_) @v)) (#has? @v ((identifier) @id) rows: each))"#),
    ("not_has_on_child", r#"((object (pair) @a (pair) @b (pair) @c) (#not-has? @c ((call_expression))))"#),
    ("root_capture_relation", r#"((object (pair) @a (pair) @b (pair) @c) @obj (#has-ancestor? @obj ((if_statement) @branch)))"#),
    ("contains_on_child", r#"((object (pair) @a (pair) @b (pair) @c) (#contains? @a "k"))"#),
    ("match_and_ancestor", r#"((object (pair key: (property_identifier) @k) (pair) @b (pair) @c) (#match? @k "^k") (#has-ancestor? @b ((function_declaration) @fn)))"#),
];

fn rows(compiled: &hafley_scm::scmpp::Compiled, src: &[u8]) -> Vec<String> {
    let grammar = sprefa_extract::RyiLang::parse_name("typescript").unwrap().tree_sitter_language();
    let tree = hafley_scm::cst::parse(&grammar, src).unwrap();
    let mut db = sqlite::Database::memory().unwrap();
    let mut store = scmpp::open(&mut db).unwrap();
    scmpp::write_file(&mut db, &mut store, compiled, "source.ts", src, &tree, &|| false, false).unwrap();
    let mut rows: Vec<String> = scmpp::run_sql(&mut db, compiled)
        .unwrap()
        .into_iter()
        .map(|row| {
            let sorted: std::collections::BTreeMap<_, _> = row.into_iter().collect();
            serde_json::to_string(&sorted).unwrap()
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn scmpp_split_with_relations_answers_the_unsplit_rows() {
    let _snapshots = snapshots();
    let src = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/scmpp_split_relations/source.ts")).unwrap();
    let grammar = sprefa_extract::RyiLang::parse_name("typescript").unwrap().tree_sitter_language();
    let table: Vec<_> = CASES
        .iter()
        .map(|(name, program)| {
            let split = hafley_scm::scmpp::compile(&grammar, program).unwrap();
            let whole = hafley_scm::scmpp::compile_unsplit(&grammar, program).unwrap();
            let (split_rows, whole_rows) = (rows(&split, &src), rows(&whole, &src));
            serde_json::json!({
                "case": name,
                "split": split.warnings.iter().any(|warning| warning.contains("split into")),
                "same_rows": split_rows == whole_rows,
                "rows": whole_rows.len(),
            })
        })
        .collect();
    insta::assert_json_snapshot!("scmpp_split_with_relations_answers_the_unsplit_rows", table);
}
