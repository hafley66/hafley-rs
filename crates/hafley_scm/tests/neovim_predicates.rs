
fn build_error(scm: &str) -> String {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    match hafley_scm::build(&language, scm) {
        Err(error) => format!("{error:?}"),
        Ok(_) => "built".into(),
    }
}

#[test]
fn relation_and_contains_predicates_are_build_errors() {
    let rows = [
        "((identifier) @x (#has-parent? @x \"let_declaration\"))",
        "((let_declaration (identifier) @x) (#has-parent? @x \"function_item\"))",
        "((identifier) @x (#has-parent? @x \"function_item\" \"let_declaration\"))",
        "((identifier) @x (#not-has-parent? @x \"function_item\"))",
        "((identifier) @x (#has-parent? @x))",
        "((function_item name: (identifier) @name body: (block) @b)
        (#not-has? @b \"return_expression\" \"try_expression\" \"break_expression\" \"continue_expression\"))",
        "((function_item name: (identifier) @name body: (block) @b)
        (#not-has? @b \"return_expression\" stopBy: neighbor))",
        "((identifier) @x (#has-ancestor? @x \"retrun_expression\"))",
        "((identifier) @x (#has? @x \"return_expression\" stopBy: neighbour))",
        "((identifier) @first)\n((identifier) @x (#has? @x \"retrun_expression\"))",
        "((identifier) @x (#contains? @x \"fo\" \"oo\"))",
        "((identifier) @x (#not-contains? @x \"zz\"))",
        "((identifier) @x (#contains? @x))",
    ]
    .map(build_error)
    .join("\n");
    assert_eq!(
        rows,
        "ScmppOnly { pattern: 0, op: \"has-parent?\" }\nScmppOnly { pattern: 0, op: \"has-parent?\" }\nScmppOnly { pattern: 0, op: \"has-parent?\" }\nScmppOnly { pattern: 0, op: \"not-has-parent?\" }\nScmppOnly { pattern: 0, op: \"has-parent?\" }\nScmppOnly { pattern: 0, op: \"not-has?\" }\nScmppOnly { pattern: 0, op: \"not-has?\" }\nScmppOnly { pattern: 0, op: \"has-ancestor?\" }\nScmppOnly { pattern: 0, op: \"has?\" }\nScmppOnly { pattern: 1, op: \"has?\" }\nScmppOnly { pattern: 0, op: \"contains?\" }\nScmppOnly { pattern: 0, op: \"not-contains?\" }\nScmppOnly { pattern: 0, op: \"contains?\" }"
    );
}
