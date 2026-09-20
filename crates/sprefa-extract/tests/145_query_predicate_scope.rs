//! Predicate scope for tree-sitter queries, through the library and the real
//! Rust grammar. A match is gated by its own pattern's predicates and no others.

use sprefa_extract::lang::{query_tree_sitter_spans, TreeSitterQuery, TreeSitterSpannedMatch};

const RUST_SRC: &str = r#"fn outer(name: &str) -> bool {
    let needle = "ab";
    name.contains(needle)
}

fn other(items: &[u8]) -> bool {
    items.contains(&3)
}
"#;

/// `(line, capture label, capture text)` for every match, in query order.
fn rows(query: &str) -> Vec<(u32, String, String)> {
    let request = TreeSitterQuery {
        language: "rust".into(),
        query: query.into(),
    };
    let spans = query_tree_sitter_spans(RUST_SRC.as_bytes(), &request).expect("query runs");
    spans
        .iter()
        .map(|found: &TreeSitterSpannedMatch| {
            let capture = &found.captures[0];
            (found.line, capture.label.clone(), capture.text.clone())
        })
        .collect()
}

#[test]
fn a_predicate_on_one_pattern_never_gates_another_patterns_matches() {
    let found = rows(
        r#"(function_item name: (identifier) @fn)
((let_declaration pattern: (identifier) @var) (#eq? @var "needle"))"#,
    );
    assert_eq!(
        found,
        vec![
            (1, "fn".to_string(), "outer".to_string()),
            (2, "var".to_string(), "needle".to_string()),
            (6, "fn".to_string(), "other".to_string()),
        ]
    );
}

#[test]
fn a_patterns_own_predicate_still_filters_its_matches() {
    let found = rows(r#"((let_declaration pattern: (identifier) @var) (#eq? @var "nope"))"#);
    assert_eq!(found, vec![]);
}

#[test]
fn two_predicates_reach_only_their_own_patterns_matches() {
    let found = rows(
        r#"((function_item name: (identifier) @fn) (#eq? @fn "outer"))
((let_declaration pattern: (identifier) @var) (#eq? @var "needle"))"#,
    );
    assert_eq!(
        found,
        vec![
            (1, "fn".to_string(), "outer".to_string()),
            (2, "var".to_string(), "needle".to_string()),
        ]
    );
}

#[test]
fn a_failing_predicate_drops_only_its_own_patterns_matches() {
    let found = rows(
        r#"(function_item name: (identifier) @fn)
((let_declaration pattern: (identifier) @var) (#eq? @var "nope"))"#,
    );
    assert_eq!(
        found,
        vec![
            (1, "fn".to_string(), "outer".to_string()),
            (6, "fn".to_string(), "other".to_string()),
        ]
    );
}

#[test]
fn a_single_pattern_with_a_holding_predicate_is_unchanged() {
    let found = rows(r#"((function_item name: (identifier) @fn) (#eq? @fn "outer"))"#);
    assert_eq!(found, vec![(1, "fn".to_string(), "outer".to_string())]);
}
