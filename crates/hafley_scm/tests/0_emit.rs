use hafley_scm::{EmittedValue, MatchArena, QueryExtError};

#[test]
fn emits_capture_and_literal_fields_into_one_arena() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let scm = r#"
      ((call_expression function: (identifier) @callee) @group
        (#emit! "call.site" "group" @group "span" @callee "callee" @callee))
      ((binary_expression "+" @operator) @group
        (#emit! "call.site" "group" @group "span" @operator "callee" "plus"))
    "#;
    let query = hafley_scm::build(&language, scm).expect("emissions compile");
    let src = b"fn f() { foo(); let x = a + b; }";
    let tree = hafley_scm::cst::parse(&language, src).expect("source parses");
    let mut arena = MatchArena::default();
    hafley_scm::run(&query, "f.rs", src, &tree, u32::MAX, &mut arena).expect("query runs");
    hafley_scm::run(&query, "g.rs", src, &tree, u32::MAX, &mut arena).expect("second file runs");

    let group = query.field_id("group").unwrap();
    let span = query.field_id("span").unwrap();
    let callee = query.field_id("callee").unwrap();
    let rows = arena.emitted.iter().map(|row| {
        let value = |key| row.get(&arena, key).unwrap().text(src, &query).unwrap();
        let source = matches!(row.get(&arena, callee), Some(EmittedValue::Bytes(_)));
        (row.file, query.relations[row.relation as usize].as_ref(), value(group), value(span), value(callee), source)
    }).collect::<Vec<_>>();
    assert_eq!(rows, [
        (0, "call.site", "foo()", "foo", "foo", true),
        (0, "call.site", "a + b", "+", "plus", false),
        (1, "call.site", "foo()", "foo", "foo", true),
        (1, "call.site", "a + b", "+", "plus", false),
    ]);
}

#[test]
fn emit_rejects_incomplete_and_duplicate_fields() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let incomplete = "((identifier) @name (#emit! \"call.site\" \"name\"))";
    assert!(matches!(
        hafley_scm::build(&language, incomplete),
        Err(QueryExtError::Arity { operator, got: 2 }) if operator == "emit!"
    ));
    let duplicate = "((identifier) @name (#emit! \"call.site\" \"name\" @name \"name\" @name))";
    assert!(matches!(hafley_scm::build(&language, duplicate), Err(QueryExtError::DuplicateField(_))));
}
