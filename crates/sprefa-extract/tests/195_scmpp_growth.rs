#![cfg(feature = "cli")]
#![allow(dead_code)]
//! scm++ growth classes, counted with hafley-observe spans: compile is per level, writes and rows per file size.

#[path = "../src/bin/ryi/2_scmpp.rs"]
mod scmpp;
#[path = "../src/bin/ryi/0_sqlite.rs"]
mod sqlite;

use hafley_observe::{assert_growth_sized, CountRecorder, Growth, SpanCounts};
use tracing_subscriber::prelude::*;

fn counted(work: impl FnOnce()) -> SpanCounts {
    let (recorder, layer) = CountRecorder::new();
    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, work);
    recorder.counts()
}

fn rust() -> tree_sitter::Language {
    sprefa_extract::RyiLang::parse_name("rust")
        .unwrap()
        .tree_sitter_language()
}

/// Two levels whatever `n` is; `n` adds kept text predicates, cross-level conditions and comments.
fn query(n: usize) -> String {
    let mut text = String::from("((function_item name: (identifier) @fn body: (_) @body)");
    for _ in 0..n {
        text.push_str("\n  ; padding\n  (#not-eq? @fn \"x\")");
    }
    text.push_str("\n  (#has? @body ((call_expression function: (identifier) @callee)");
    for _ in 0..n {
        text.push_str(" (#not-contains? @callee \"zz\")");
    }
    text.push_str(" (#eq? @callee @fn)) rows: each))");
    text
}

/// `n` recursive functions: one level-0 match, one call and one result row each.
fn source(n: usize) -> String {
    (0..n)
        .map(|i| format!("fn f{i}(n: u32) -> u32 {{ if n == 0 {{ 0 }} else {{ f{i}(n - 1) }} }}\n"))
        .collect()
}

#[test]
fn compile_counts_one_query_new_per_level_as_query_text_grows() {
    let (small, large) = (query(1), query(100));
    let small_counts = counted(|| {
        hafley_scm::scmpp::compile(&rust(), &small).unwrap();
    });
    let large_counts = counted(|| {
        hafley_scm::scmpp::compile(&rust(), &large).unwrap();
    });
    small_counts.assert_instances("scmpp_query_new", 2);
    large_counts.assert_instances("scmpp_query_new", 2);
    assert_growth_sized(
        &small_counts,
        &large_counts,
        "scmpp_query_new",
        small.len(),
        large.len(),
        Growth::Constant,
    );
}

fn store_and_query(functions: usize) -> (SpanCounts, usize) {
    let compiled = hafley_scm::scmpp::compile(&rust(), &query(1)).unwrap();
    let text = source(functions);
    let tree = hafley_scm::cst::parse(&rust(), text.as_bytes()).unwrap();
    let mut found = 0;
    let counts = counted(|| {
        let mut db = sqlite::Database::memory().unwrap();
        let mut store = scmpp::open(&mut db).unwrap();
        scmpp::write_file(&mut db, &mut store, &compiled, "growth.rs", text.as_bytes(), &tree).unwrap();
        found = scmpp::run_sql(&mut db, &compiled).unwrap().len();
    });
    (counts, found)
}

#[test]
fn capture_and_cst_writes_and_each_rows_grow_linearly_with_file_size() {
    let ((small, small_rows), (large, large_rows)) = (store_and_query(10), store_and_query(1000));
    assert_eq!((small_rows, large_rows), (10, 1000));
    for span in ["scmpp_capture_row", "scmpp_cst_row", "scmpp_row"] {
        assert_growth_sized(&small, &large, span, 10, 1000, Growth::Linear);
    }
}
