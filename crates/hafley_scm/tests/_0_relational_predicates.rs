//! Relation predicates are build errors naming pattern and op; `ryii query --scmpp` evaluates them.

fn build_error(language: &tree_sitter::Language, scm: &str) -> String {
    match hafley_scm::build(language, scm) {
        Err(error) => format!("{error:?}"),
        Ok(_) => "built".into(),
    }
}

#[test]
fn relation_predicates_are_build_errors() {
    let rust = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let ts = tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT);
    let rows = [
        (&rust, "((expression_statement) @c (#precedes? @c expression_statement stopBy: neighbor))"),
        (&rust, "((expression_statement) @c (#follows? @c expression_statement stopBy: end))"),
        (&rust, "((expression_statement) @c (#not-precedes? @c expression_statement stopBy: neighbor))"),
        (&rust, "((expression_statement) @c (#not-follows? @c expression_statement stopBy: end))"),
        (&rust, "((expression_statement) @c (#nth-child? @c 3))"),
        (&rust, "((expression_statement) @c (#not-nth-child? @c 2 of expression_statement))"),
        (&ts, "((expression_statement) @c (#nth-child? @c 3 of statement))"),
        (&rust, "((call_expression) @c\n (#has-ancestor? @c (function_item name: (identifier) @fn (#eq? @fn \"host\"))))"),
        (&rust, "((call_expression) @c (#has-ancestor? @c (function_item name: (identifier) @fn) rows: each))"),
        (&rust, "(identifier) @a\n((expression_statement) @c (#eq? @c \"a();\") (#has-parent? @c block))"),
    ]
    .map(|(language, scm)| build_error(language, scm))
    .join("\n");
    assert_eq!(
        rows,
        "ScmppOnly { pattern: 0, op: \"precedes?\" }\nScmppOnly { pattern: 0, op: \"follows?\" }\nScmppOnly { pattern: 0, op: \"not-precedes?\" }\nScmppOnly { pattern: 0, op: \"not-follows?\" }\nScmppOnly { pattern: 0, op: \"nth-child?\" }\nScmppOnly { pattern: 0, op: \"not-nth-child?\" }\nScmppOnly { pattern: 0, op: \"nth-child?\" }\nScmppOnly { pattern: 0, op: \"has-ancestor?\" }\nScmppOnly { pattern: 0, op: \"has-ancestor?\" }\nScmppOnly { pattern: 1, op: \"has-parent?\" }"
    );
}
