//! Snapshots the crate-owned CallF definition rows: one test per claimed
//! shape, plus first-claim precedence when two patterns hit one range.

use hafley_scm::lang::rust::{
    call_definition_rows, call_definition_rows_from_arena, CallDefinitionKind, CallDefinitionRow,
    RUST_CALL_QUERY,
};
use hafley_scm::{build, run, MatchArena};
use tree_sitter::{Language, Parser};
use tree_sitter_rust::LANGUAGE;

type Snap = (u32, u32, CallDefinitionKind, Option<String>);

fn rows(src: &str) -> Vec<CallDefinitionRow> {
    let language = Language::new(LANGUAGE);
    let query = build(&language, RUST_CALL_QUERY).expect("bundled rust call query builds");
    let mut parser = Parser::new();
    parser.set_language(&language).expect("rust grammar");
    let tree = parser.parse(src.as_bytes(), None).expect("rust tree");
    call_definition_rows(&query, "snapshot", src.as_bytes(), &tree)
}

fn snap(src: &str) -> Vec<Snap> {
    rows(src)
        .iter()
        .map(|row| (row.range.start, row.range.end, row.kind, row.name.clone()))
        .collect()
}

#[test]
fn free_function_is_a_free_row() {
    let src = "fn free_fn(x: u32) -> u32 { x }";
    let start = src.find("free_fn").unwrap() as u32;
    let end = src.rfind('}').unwrap() as u32 + 1;
    assert_eq!(
        snap(src),
        vec![(start, end, CallDefinitionKind::Free, Some("free_fn".into()))]
    );
}

#[test]
fn impl_method_is_a_method_row() {
    let src = "struct S;\nimpl S {\n    fn method_fn(&self) -> u32 {\n        1\n    }\n}\n";
    let start = src.find("method_fn").unwrap() as u32;
    let end = src.find("\n    }\n}").unwrap() as u32 + "\n    }".len() as u32;
    assert_eq!(
        snap(src),
        vec![(
            start,
            end,
            CallDefinitionKind::Method,
            Some("method_fn".into())
        )]
    );
}

#[test]
fn impl_method_beats_the_free_pattern_on_one_range() {
    // The impl pattern and the bare free pattern both claim name..body; the
    // map keeps the first claim, so one Method row survives.
    let src = "struct S;\nimpl S {\n    fn dup_fn(&self) {}\n}\n";
    let snapshot = snap(src);
    assert_eq!(snapshot.len(), 1, "one range, one row: {snapshot:?}");
    assert_eq!(
        snapshot[0].2,
        CallDefinitionKind::Method,
        "the impl pattern claims first: {snapshot:?}"
    );
}

#[test]
fn trait_signatures_trim_trailing_whitespace() {
    let src = "trait T {\n    fn sig_fn(&self) -> u32;\n    fn gapped_fn(&self) -> u64   ;\n}\n";
    let s1 = src.find("sig_fn").unwrap() as u32;
    let e1 = src.find("u32;").unwrap() as u32 + 3;
    let s2 = src.find("gapped_fn").unwrap() as u32;
    let e2 = src.find("u64   ;").unwrap() as u32 + 3;
    assert_eq!(
        snap(src),
        vec![
            (s1, e1, CallDefinitionKind::Method, Some("sig_fn".into())),
            (s2, e2, CallDefinitionKind::Method, Some("gapped_fn".into()))
        ]
    );
}

#[test]
fn enum_variants_are_free_rows_on_the_ident() {
    let src = "enum Color {\n    Red,\n    Green(u32),\n}\n";
    let s1 = src.find("Red").unwrap() as u32;
    let s2 = src.find("Green").unwrap() as u32;
    assert_eq!(
        snap(src),
        vec![
            (s1, s1 + 3, CallDefinitionKind::Free, Some("Red".into())),
            (s2, s2 + 5, CallDefinitionKind::Free, Some("Green".into()))
        ]
    );
}

#[test]
fn closure_is_a_nameless_lambda_row() {
    let src = "fn wrap() {\n    let f = |x: u32| x + 1;\n    let _ = f(1);\n}\n";
    let wrap = (
        src.find("wrap").unwrap() as u32,
        src.rfind('}').unwrap() as u32 + 1,
        CallDefinitionKind::Free,
        Some("wrap".into()),
    );
    let closure = (
        src.find("|x: u32| x + 1").unwrap() as u32,
        src.find("x + 1").unwrap() as u32 + 5,
        CallDefinitionKind::Lambda,
        None,
    );
    assert_eq!(snap(src), vec![wrap, closure]);
}

#[test]
fn nested_module_fn_is_claimed() {
    let src = "mod inner_mod {\n    fn deep_fn() {\n        2\n    }\n}\n";
    let start = src.find("deep_fn").unwrap() as u32;
    let end = src.find("\n    }\n}").unwrap() as u32 + "\n    }".len() as u32;
    assert_eq!(
        snap(src),
        vec![(start, end, CallDefinitionKind::Free, Some("deep_fn".into()))]
    );
}

#[test]
fn arena_rows_project_the_same_as_the_one_shot() {
    let src = "fn top_fn() {}\ntrait T {\n    fn sig_fn(&self) -> u32;\n}\n";
    let language = Language::new(LANGUAGE);
    let query = build(&language, RUST_CALL_QUERY).expect("bundled rust call query builds");
    let mut parser = Parser::new();
    parser.set_language(&language).expect("rust grammar");
    let tree = parser.parse(src.as_bytes(), None).expect("rust tree");
    let mut arena = MatchArena::default();
    run(
        &query,
        "snapshot",
        src.as_bytes(),
        &tree,
        u32::MAX,
        &mut arena,
    )
    .expect("rust call query runs");
    let from_arena = call_definition_rows_from_arena(&query, &arena, src.as_bytes());
    assert_eq!(rows(src), from_arena, "one shot and arena agree");
    assert!(
        from_arena
            .iter()
            .any(|row| row.name.as_deref() == Some("sig_fn")
                && row.kind == CallDefinitionKind::Method)
    );
}
