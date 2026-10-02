use hafley_scm::{MatchArena, QueryExtError};

fn run_query(scm: &str, src: &[u8]) -> usize {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("the rust grammar loads");
    let tree = parser.parse(src, None).expect("the source parses");
    let query = hafley_scm::build(&language, scm).expect("the query builds");
    let mut arena = MatchArena::default();
    hafley_scm::run(&query, "test.rs", src, &tree, u32::MAX, &mut arena).expect("the query runs");
    arena.rows.len()
}

#[test]
fn has_parent_matches_only_the_direct_parent() {
    assert_eq!(
        run_query(
            "((identifier) @x (#has-parent? @x \"let_declaration\"))",
            b"let x = 1;"
        ),
        1
    );
    assert_eq!(
        run_query(
            "((let_declaration (identifier) @x) (#has-parent? @x \"function_item\"))",
            b"fn f() { let x = 1; }",
        ),
        0
    );
}

#[test]
fn has_parent_accepts_any_named_parent_kind() {
    assert_eq!(
        run_query(
            "((identifier) @x (#has-parent? @x \"function_item\" \"let_declaration\"))",
            b"let x = 1;",
        ),
        1
    );
}

#[test]
fn contains_requires_every_literal() {
    assert_eq!(
        run_query("((identifier) @x (#contains? @x \"fo\" \"oo\"))", b"foo;"),
        1
    );
    assert_eq!(
        run_query("((identifier) @x (#contains? @x \"fo\" \"zz\"))", b"foo;"),
        0
    );
}

#[test]
fn contains_reads_non_utf8_source_bytes() {
    let source = b"// \xff needle\nfn f() {}";
    assert_eq!(
        run_query("((line_comment) @x (#contains? @x \"needle\"))", source),
        1
    );
}

#[test]
fn generic_not_complements_parent_and_contains() {
    assert_eq!(
        run_query(
            "((identifier) @x (#not-has-parent? @x \"function_item\"))",
            b"let x = 1;"
        ),
        1
    );
    assert_eq!(
        run_query("((identifier) @x (#not-contains? @x \"zz\"))", b"foo;"),
        1
    );
}

#[test]
fn empty_kind_and_literal_lists_are_arity_errors() {
    for query in [
        "((identifier) @x (#has-parent? @x))",
        "((identifier) @x (#contains? @x))",
    ] {
        assert!(matches!(
            hafley_scm::build(
                &tree_sitter::Language::new(tree_sitter_rust::LANGUAGE),
                query
            ),
            Err(QueryExtError::Scmpp(hafley_scm::scmpp::ScmppError::Unsupported(_)))
        ));
    }
}

const HAS_BODIES: &[u8] = br#"
fn returns() { return; }
fn tries() -> Result<(), ()> { foo()?; Ok(()) }
fn breaks() { loop { break; } }
fn continues() { loop { continue; } }
fn clean() { 1; }
"#;

fn render_has_bodies(scm: &str) -> String {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let tree = hafley_scm::cst::parse(&language, HAS_BODIES).expect("the source parses");
    let query = hafley_scm::build(&language, scm).expect("the query builds");
    let mut arena = MatchArena::default();
    hafley_scm::run(&query, "test.rs", HAS_BODIES, &tree, u32::MAX, &mut arena)
        .expect("the query runs");
    let matched = arena.rows.iter().map(|row| {
        let name = arena.spans[row.spans.start as usize..row.spans.end as usize]
            .iter()
            .find(|span| query.names[span.name as usize].as_ref() == "name")
            .expect("each match captures a function name");
        std::str::from_utf8(&HAS_BODIES[name.bytes.start as usize..name.bytes.end as usize])
            .expect("function names are UTF-8")
    }).collect::<Vec<_>>();
    let mut lines = vec!["body      | matches".to_string()];
    for name in ["returns", "tries", "breaks", "continues", "clean"] {
        lines.push(format!("{name:<9} | {}", matched.iter().filter(|found| **found == name).count()));
    }
    lines.join("\n")
}

#[test]
fn not_has_kind_list_excludes_each_descendant_kind() {
    let scm = r#"((function_item name: (identifier) @name body: (block) @b)
        (#not-has? @b "return_expression" "try_expression" "break_expression" "continue_expression"))"#;
    assert_eq!(render_has_bodies(scm), "body      | matches\nreturns   | 0\ntries     | 0\nbreaks    | 0\ncontinues | 0\nclean     | 1");
}

#[test]
fn not_has_kind_list_neighbor_checks_direct_children() {
    let scm = r#"((function_item name: (identifier) @name body: (block) @b)
        (#not-has? @b "return_expression" "try_expression" "break_expression" "continue_expression" stopBy: neighbor))"#;
    assert_eq!(render_has_bodies(scm), "body      | matches\nreturns   | 1\ntries     | 1\nbreaks    | 1\ncontinues | 1\nclean     | 1");
}

#[test]
fn unknown_kinds_and_stop_words_name_the_predicate_and_pattern() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let rows = [
        ("has?", "\"retrun_expression\"", 0),
        ("has-ancestor?", "\"retrun_expression\"", 0),
        ("has-parent?", "\"retrun_expression\"", 0),
        ("has?", "\"return_expression\" stopBy: neighbour", 0),
        ("has-ancestor?", "\"return_expression\" stopBy: neighbour", 0),
        ("has?", "\"retrun_expression\"", 1),
    ].map(|(operator, args, pattern)| {
        let prefix = if pattern == 1 { "((identifier) @first)\n" } else { "" };
        let scm = format!("{prefix}((identifier) @x (#{operator} @x {args}))");
        let error = match hafley_scm::build(&language, &scm) {
            Err(QueryExtError::Scmpp(detail)) => detail.to_string(),
            _ => panic!("expected unknown kind for {scm}"),
        };
        format!("{operator:<13} | {error}")
    }).join("\n");
    assert_eq!(rows, "has?          | scm++ level 1 `((retrun_expression) @__root)`: Query error at 1:3. Invalid node type \"retrun_expression\"\nhas-ancestor? | scm++ level 1 `((retrun_expression) @__root)`: Query error at 1:3. Invalid node type \"retrun_expression\"\nhas-parent?   | scm++ level 1 `((retrun_expression) @__root)`: Query error at 1:3. Invalid node type \"retrun_expression\"\nhas?          | scm++: #has? at byte 17: bad option stopBy: Word(\"neighbour\")\nhas-ancestor? | scm++: #has-ancestor? at byte 17: bad option stopBy: Word(\"neighbour\")\nhas?          | scm++ level 1 `((retrun_expression) @__root)`: Query error at 1:3. Invalid node type \"retrun_expression\"");
}
