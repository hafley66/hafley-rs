//! Without `--sqlite` the scm++ store is the run's own: it holds only the node rows the plan's
//! walks read, and answers the rows the full CST store answers. Stdout drops a whole-file capture's text.
use crate::t_195_scmpp_growth::{scmpp, sqlite};

use super::v5_support::snapshots;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/scmpp_store_trim");

/// One per walk the SQL lowers to.
const CASES: &[(&str, &str)] = &[
    ("descendant", r#"((function_declaration name: (identifier) @fn) @f (#has? @f ((call_expression function: (identifier) @callee)) rows: each))"#),
    ("ancestor_field", r#"((call_expression function: (identifier) @hook) @call (#match? @hook "^use") (#has-ancestor? @call ((if_statement) @branch) field: consequence rows: each))"#),
    ("ancestor_stop", r#"((call_expression function: (identifier) @hook) @call (#has-ancestor? @call ((statement_block) @block) stopBy: ((arrow_function)) rows: each))"#),
    ("parent_field", r#"((identifier) @id (#has-parent? @id ((call_expression) @call) field: function))"#),
    ("precedes", r#"((lexical_declaration) @decl (#precedes? @decl ((if_statement) @after) rows: each))"#),
    ("nth_child", r#"((if_statement) @s (#nth-child? @s 2))"#),
    ("nth_child_of", r#"((expression_statement) @s (#nth-child? @s 1 of (expression_statement)))"#),
    ("sibling_split", r#"(formal_parameters (required_parameter) @a (required_parameter) @b (required_parameter) @c)"#),
    ("hooks", include_str!("../bench/rules-of-hooks/2_hooks.scm")),
];

fn answer(compiled: &hafley_scm::scmpp::Compiled, src: &[u8], trim: bool) -> (i64, Vec<serde_json::Map<String, serde_json::Value>>) {
    let grammar = sprefa_extract::RyiLang::parse_name("tsx").unwrap().tree_sitter_language();
    let tree = hafley_scm::cst::parse(&grammar, src).unwrap();
    let mut db = sqlite::Database::memory().unwrap();
    let mut store = scmpp::open(&mut db).unwrap();
    scmpp::write_file(&mut db, &mut store, compiled, "source.tsx", src, &tree, &|| false, trim).unwrap();
    let rows = scmpp::run_sql(&mut db, compiled).unwrap();
    let nodes = db.connection().query_row("SELECT count(*) FROM scmpp_node", [], |row| row.get(0)).unwrap();
    (nodes, rows)
}

#[test]
fn scmpp_trimmed_store_answers_the_full_store_rows() {
    let _snapshots = snapshots();
    let src = std::fs::read(format!("{DIR}/source.tsx")).unwrap();
    let grammar = sprefa_extract::RyiLang::parse_name("tsx").unwrap().tree_sitter_language();
    let table: Vec<_> = CASES
        .iter()
        .map(|(name, program)| {
            let compiled = hafley_scm::scmpp::compile(&grammar, program).unwrap();
            let (full_nodes, full) = answer(&compiled, &src, false);
            let (trimmed_nodes, trimmed) = answer(&compiled, &src, true);
            serde_json::json!({
                "case": name,
                "rows": full.len(),
                "same_rows": full == trimmed,
                "full_nodes": full_nodes,
                "trimmed_nodes": trimmed_nodes,
            })
        })
        .collect();
    insta::assert_json_snapshot!("scmpp_trimmed_store_answers_the_full_store_rows", table);
}

#[test]
fn scmpp_stdout_drops_whole_file_capture_text() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["query", "--scmpp", concat!(env!("CARGO_MANIFEST_DIR"), "/bench/rules-of-hooks/2_hooks.scm")])
        .arg(format!("{DIR}/source.tsx"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let first: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(String::from_utf8(output.stdout).unwrap().lines().next().unwrap()).unwrap();
    insta::assert_snapshot!(first.keys().cloned().collect::<Vec<_>>().join(" "), @"path callee__start callee__end callee__text invoked__start invoked__end invoked__text hook__start hook__end hook__text source__start source__end");
}
