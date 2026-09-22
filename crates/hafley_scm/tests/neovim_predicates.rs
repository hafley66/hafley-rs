use hafley_scm::{MatchArena, QueryExtError};

fn run_query(scm: &str, src: &[u8]) -> usize {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("the rust grammar loads");
    let tree = parser.parse(src, None).expect("the source parses");
    let query = hafley_scm::build(&language, scm).expect("the query builds");
    let mut arena = MatchArena::default();
    hafley_scm::run(&query, "test.rs", src, &tree, u32::MAX, &mut arena)
        .expect("the query runs");
    arena.rows.len()
}

#[test]
fn has_parent_matches_only_the_direct_parent() {
    assert_eq!(run_query("((identifier) @x (#has-parent? @x \"let_declaration\"))", b"let x = 1;"), 1);
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
    assert_eq!(run_query("((identifier) @x (#contains? @x \"fo\" \"oo\"))", b"foo;"), 1);
    assert_eq!(run_query("((identifier) @x (#contains? @x \"fo\" \"zz\"))", b"foo;"), 0);
}

#[test]
fn contains_reads_non_utf8_source_bytes() {
    let source = b"// \xff needle\nfn f() {}";
    assert_eq!(run_query("((line_comment) @x (#contains? @x \"needle\"))", source), 1);
}

#[test]
fn generic_not_complements_parent_and_contains() {
    assert_eq!(run_query("((identifier) @x (#not-has-parent? @x \"function_item\"))", b"let x = 1;"), 1);
    assert_eq!(run_query("((identifier) @x (#not-contains? @x \"zz\"))", b"foo;"), 1);
}

#[test]
fn empty_kind_and_literal_lists_are_arity_errors() {
    for query in [
        "((identifier) @x (#has-parent? @x))",
        "((identifier) @x (#contains? @x))",
    ] {
        assert!(matches!(
            hafley_scm::build(&tree_sitter::Language::new(tree_sitter_rust::LANGUAGE), query),
            Err(QueryExtError::Arity { .. })
        ));
    }
}
