use hafley_scm::{MatchArena, QueryExtError};

#[test]
fn call_site_emits_capture_and_literal_names_into_one_arena() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let scm = r#"
      ((call_expression function: (identifier) @callee) @group
        (#emit-call-site! @group @callee @callee))
      ((binary_expression "+" @operator) @group
        (#emit-call-site! @group @operator "plus"))
    "#;
    let query = hafley_scm::build(&language, scm).expect("emissions compile");
    let src = b"fn f() { foo(); let x = a + b; }";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("rust grammar loads");
    let tree = parser.parse(src, None).expect("source parses");
    let mut arena = MatchArena::default();
    hafley_scm::run(&query, "f.rs", src, &tree, u32::MAX, &mut arena).expect("query runs");
    hafley_scm::run(&query, "g.rs", src, &tree, u32::MAX, &mut arena).expect("second file runs");

    let rows = arena.call_sites.iter().map(|row| {
        let group = std::str::from_utf8(&src[row.group.start as usize..row.group.end as usize]).unwrap();
        let span = std::str::from_utf8(&src[row.span.start as usize..row.span.end as usize]).unwrap();
        let callee = match (&row.callee_bytes, row.callee_literal) {
            (Some(bytes), None) => std::str::from_utf8(&src[bytes.start as usize..bytes.end as usize]).unwrap(),
            (None, Some(index)) => &query.call_site_literals[index as usize],
            _ => panic!("exactly one callee source"),
        };
        (row.file, group, span, callee)
    }).collect::<Vec<_>>();
    assert_eq!(rows, [
        (0, "foo()", "foo", "foo"),
        (0, "a + b", "+", "plus"),
        (1, "foo()", "foo", "foo"),
        (1, "a + b", "+", "plus"),
    ]);
}

#[test]
fn call_site_emit_rejects_an_incomplete_record() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let scm = "((identifier) @name (#emit-call-site! @name @name))";
    assert!(matches!(
        hafley_scm::build(&language, scm),
        Err(QueryExtError::Arity { operator, got: 2 }) if operator == "emit-call-site!"
    ));
}
