use hafley_scm::scmpp::compile;

fn rust() -> tree_sitter::Language {
    tree_sitter_rust::LANGUAGE.into()
}

const SCHEMA: &str = "CREATE TABLE scmpp_dict_path (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_dict_text (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_node (file INTEGER, pre INTEGER, last INTEGER, parent INTEGER, depth INTEGER, sib INTEGER, idx INTEGER, kind INTEGER, field INTEGER, start INTEGER, \"end\" INTEGER, named INTEGER, PRIMARY KEY (file, pre)) WITHOUT ROWID;
CREATE TABLE scmpp_capture (file INTEGER, pattern INTEGER, \"match\" INTEGER, capture INTEGER, node INTEGER, start INTEGER, \"end\" INTEGER, text INTEGER, PRIMARY KEY (file, pattern, \"match\", capture, node)) WITHOUT ROWID;";

/// Flat pattern texts, then the SQL; the SQL is prepared against the scm++ store tables.
fn show(language: &tree_sitter::Language, query: &str) -> String {
    let compiled = compile(language, query).unwrap_or_else(|error| panic!("{error}"));
    if !compiled.sql.contains("regexp(") {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(SCHEMA).unwrap();
        db.prepare(&compiled.sql).unwrap_or_else(|error| panic!("{error}\n{}", compiled.sql));
    }
    let patterns = compiled.patterns.iter().map(|p| format!("{}: {}", p.id, p.text)).collect::<Vec<_>>().join("\n");
    format!("{patterns}\n--\n{}", compiled.sql)
}

fn error(language: &tree_sitter::Language, query: &str) -> String {
    match compile(language, query) {
        Ok(_) => "ok".into(),
        Err(error) => error.to_string(),
    }
}

#[test]
fn level_match_selects_every_capture() {
    assert_eq!(
        show(&rust(), "((call_expression function: (identifier) @callee arguments: (_) @args) @call)"),
        r#"0: ((call_expression function: (identifier) @callee arguments: (_) @args) @call @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_callee"."start" AS "callee__start",
  "c0_callee"."end" AS "callee__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_callee".text) AS "callee__text",
  "c0_args"."start" AS "args__start",
  "c0_args"."end" AS "args__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_args".text) AS "args__text",
  "c0_call"."start" AS "call__start",
  "c0_call"."end" AS "call__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_call".text) AS "call__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_callee" ON "c0_callee".file = "r0".file AND "c0_callee".pattern = 0 AND "c0_callee"."match" = "r0"."match" AND "c0_callee".capture = 2
  LEFT JOIN scmpp_capture AS "c0_args" ON "c0_args".file = "r0".file AND "c0_args".pattern = 0 AND "c0_args"."match" = "r0"."match" AND "c0_args".capture = 3
  LEFT JOIN scmpp_capture AS "c0_call" ON "c0_call".file = "r0".file AND "c0_call".pattern = 0 AND "c0_call"."match" = "r0"."match" AND "c0_call".capture = 4
WHERE "r0".pattern = 0
  AND "r0".capture = 1
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn has_parent_is_one_edge() {
    assert_eq!(
        show(&rust(), "((identifier) @x (#has-parent? @x let_declaration))"),
        r#"0: ((identifier) @x @__root)
1: ((let_declaration) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_x"."start" AS "x__start",
  "c0_x"."end" AS "x__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_x".text) AS "x__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_x" ON "c0_x".file = "r0".file AND "c0_x".pattern = 0 AND "c0_x"."match" = "r0"."match" AND "c0_x".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  JOIN scmpp_node AS "e0" ON "e0".file = "c0_x".file AND "e0".pre = "c0_x".node AND "e0".parent = "r1".node
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn has_ancestor_recurses_up_with_a_stop_pattern() {
    assert_eq!(
        show(&rust(), "((identifier) @x (#has-ancestor? @x (function_item) stopBy: (closure_expression)))"),
        r#"0: ((identifier) @x @__root)
1: ((function_item) @__root)
2: ((closure_expression) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_x"."start" AS "x__start",
  "c0_x"."end" AS "x__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_x".text) AS "x__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_x" ON "c0_x".file = "r0".file AND "c0_x".pattern = 0 AND "c0_x"."match" = "r0"."match" AND "c0_x".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  JOIN scmpp_node AS "a0" ON "a0".file = "r1".file AND "a0".pre = "r1".node AND "r1".node < "c0_x".node AND "c0_x".node <= "a0".last AND NOT EXISTS (SELECT 1 FROM scmpp_capture AS "r2"
  JOIN scmpp_node AS "b0" ON "b0".file = "r2".file AND "b0".pre = "r2".node AND "b0".last >= "c0_x".node
  WHERE "r2".pattern = 2
  AND "r2".capture = 1
  AND "r2".file = "c0_x".file
  AND "r2".node > "a0".pre
  AND "r2".node < "c0_x".node)
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn has_recurses_down() {
    assert_eq!(
        show(&rust(), "((function_item) @f (#has? @f (macro_invocation)))"),
        r#"0: ((function_item) @f @__root)
1: ((macro_invocation) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_f"."start" AS "f__start",
  "c0_f"."end" AS "f__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_f".text) AS "f__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_f" ON "c0_f".file = "r0".file AND "c0_f".pattern = 0 AND "c0_f"."match" = "r0"."match" AND "c0_f".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  JOIN scmpp_node AS "d0" ON "d0".file = "c0_f".file AND "d0".pre = "c0_f".node AND "r1".node > "d0".pre AND "r1".node <= "d0".last
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn stop_by_neighbor_is_one_step() {
    assert_eq!(
        show(&rust(), "((block) @b (#has? @b (expression_statement) stopBy: neighbor))"),
        r#"0: ((block) @b @__root)
1: ((expression_statement) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_b"."start" AS "b__start",
  "c0_b"."end" AS "b__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_b".text) AS "b__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_b" ON "c0_b".file = "r0".file AND "c0_b".pattern = 0 AND "c0_b"."match" = "r0"."match" AND "c0_b".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  JOIN scmpp_node AS "e0" ON "e0".file = "r1".file AND "e0".pre = "r1".node AND "e0".parent = "c0_b".node
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn precedes_neighbor_compares_named_index() {
    assert_eq!(
        show(&rust(), "((expression_statement) @c (#precedes? @c (let_declaration) stopBy: neighbor))"),
        r#"0: ((expression_statement) @c @__root)
1: ((let_declaration) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_c"."start" AS "c__start",
  "c0_c"."end" AS "c__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_c".text) AS "c__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_c" ON "c0_c".file = "r0".file AND "c0_c".pattern = 0 AND "c0_c"."match" = "r0"."match" AND "c0_c".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  JOIN scmpp_node AS "e0a" ON "e0a".file = "c0_c".file AND "e0a".pre = "c0_c".node
  JOIN scmpp_node AS "e0b" ON "e0b".file = "r1".file AND "e0b".pre = "r1".node AND "e0b".parent = "e0a".parent AND "e0b".sib = "e0a".sib + 1
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn follows_with_a_stop_pattern_blocks_between() {
    assert_eq!(
        show(&rust(), "((expression_statement) @c (#follows? @c (let_declaration) stopBy: (expression_statement)))"),
        r#"0: ((expression_statement) @c @__root)
1: ((let_declaration) @__root)
2: ((expression_statement) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_c"."start" AS "c__start",
  "c0_c"."end" AS "c__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_c".text) AS "c__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_c" ON "c0_c".file = "r0".file AND "c0_c".pattern = 0 AND "c0_c"."match" = "r0"."match" AND "c0_c".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  JOIN scmpp_node AS "e0a" ON "e0a".file = "c0_c".file AND "e0a".pre = "c0_c".node
  JOIN scmpp_node AS "e0b" ON "e0b".file = "r1".file AND "e0b".pre = "r1".node AND "e0b".parent = "e0a".parent AND "e0b".sib < "e0a".sib AND NOT EXISTS (SELECT 1 FROM scmpp_capture AS "r2"
  JOIN scmpp_node AS "e0c" ON "e0c".file = "r2".file AND "e0c".pre = "r2".node
  WHERE "r2".pattern = 2
  AND "r2".capture = 1
  AND "r2".file = "e0a".file
  AND "e0c".parent = "e0a".parent
  AND "e0c".sib > "e0b".sib
  AND "e0c".sib < "e0a".sib)
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn nth_child_of_counts_matching_siblings() {
    assert_eq!(
        show(&rust(), "((expression_statement) @c (#nth-child? @c 2 of (expression_statement)))"),
        r#"0: ((expression_statement) @c @__root)
1: ((expression_statement) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_c"."start" AS "c__start",
  "c0_c"."end" AS "c__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_c".text) AS "c__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_c" ON "c0_c".file = "r0".file AND "c0_c".pattern = 0 AND "c0_c"."match" = "r0"."match" AND "c0_c".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND EXISTS (SELECT 1 FROM scmpp_node AS "m0"
  WHERE "m0".file = "c0_c".file AND "m0".pre = "c0_c".node AND "m0".sib IS NOT NULL
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "m0".file AND "r1".node = "m0".pre)
  AND (SELECT count(*) FROM scmpp_node AS "b0"
  WHERE "b0".file = "m0".file AND "b0".parent = "m0".parent AND "b0".sib < "m0".sib
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "b0".file AND "r1".node = "b0".pre)) = 1)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn nth_child_of_a_supertype_kind() {
    assert_eq!(
        show(&rust(), "((identifier) @x (#nth-child? @x 1 of _expression))"),
        r#"0: ((identifier) @x @__root)
1: ((_expression) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_x"."start" AS "x__start",
  "c0_x"."end" AS "x__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_x".text) AS "x__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_x" ON "c0_x".file = "r0".file AND "c0_x".pattern = 0 AND "c0_x"."match" = "r0"."match" AND "c0_x".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND EXISTS (SELECT 1 FROM scmpp_node AS "m0"
  WHERE "m0".file = "c0_x".file AND "m0".pre = "c0_x".node AND "m0".sib IS NOT NULL
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "m0".file AND "r1".node = "m0".pre)
  AND (SELECT count(*) FROM scmpp_node AS "b0"
  WHERE "b0".file = "m0".file AND "b0".parent = "m0".parent AND "b0".sib < "m0".sib
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "b0".file AND "r1".node = "b0".pre)) = 0)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn field_sits_on_the_step_next_to_the_target() {
    assert_eq!(
        show(&rust(), "((identifier) @x (#has-ancestor? @x (let_declaration) field: pattern))"),
        r#"0: ((identifier) @x @__root)
1: ((let_declaration) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_x"."start" AS "x__start",
  "c0_x"."end" AS "x__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_x".text) AS "x__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_x" ON "c0_x".file = "r0".file AND "c0_x".pattern = 0 AND "c0_x"."match" = "r0"."match" AND "c0_x".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  JOIN scmpp_node AS "a0" ON "a0".file = "r1".file AND "a0".pre = "r1".node AND "r1".node < "c0_x".node AND "c0_x".node <= "a0".last
  JOIN scmpp_node AS "k0" ON "k0".file = "a0".file AND "k0".parent = "a0".pre AND "k0".pre <= "c0_x".node AND "c0_x".node <= "k0".last AND "k0".field = 1
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn not_binds_nothing() {
    assert_eq!(
        show(&rust(), "((function_item) @f (#not-has? @f (call_expression function: (identifier) @g)))"),
        r#"0: ((function_item) @f @__root)
1: ((call_expression function: (identifier) @g) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_f"."start" AS "f__start",
  "c0_f"."end" AS "f__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_f".text) AS "f__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_f" ON "c0_f".file = "r0".file AND "c0_f".pattern = 0 AND "c0_f"."match" = "r0"."match" AND "c0_f".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND NOT EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  JOIN scmpp_node AS "d0" ON "d0".file = "c0_f".file AND "d0".pre = "c0_f".node AND "r1".node > "d0".pre AND "r1".node <= "d0".last
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn correlated_eq_rows_each_recursion() {
    assert_eq!(
        show(&rust(), "(function_item name: (identifier) @fn body: (_) @body)\n(#has? @body\n  (call_expression function: (identifier) @callee\n    (#eq? @callee @fn))\n  rows: each)"),
        r#"0: ((function_item name: (identifier) @fn body: (_) @body) @__root)
1: ((call_expression function: (identifier) @callee) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_fn"."start" AS "fn__start",
  "c0_fn"."end" AS "fn__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_fn".text) AS "fn__text",
  "c0_body"."start" AS "body__start",
  "c0_body"."end" AS "body__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_body".text) AS "body__text",
  "c1_callee"."start" AS "callee__start",
  "c1_callee"."end" AS "callee__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c1_callee".text) AS "callee__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_fn" ON "c0_fn".file = "r0".file AND "c0_fn".pattern = 0 AND "c0_fn"."match" = "r0"."match" AND "c0_fn".capture = 2
  LEFT JOIN scmpp_capture AS "c0_body" ON "c0_body".file = "r0".file AND "c0_body".pattern = 0 AND "c0_body"."match" = "r0"."match" AND "c0_body".capture = 3
  JOIN scmpp_capture AS "r1" ON "r1".pattern = 1 AND "r1".capture = 1 AND "r1".file = "r0".file
  LEFT JOIN scmpp_capture AS "c1_callee" ON "c1_callee".file = "r1".file AND "c1_callee".pattern = 1 AND "c1_callee"."match" = "r1"."match" AND "c1_callee".capture = 4
  JOIN scmpp_node AS "d0" ON "d0".file = "c0_body".file AND "d0".pre = "c0_body".node AND "r1".node > "d0".pre AND "r1".node <= "d0".last
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND "c1_callee".text = "c0_fn".text
ORDER BY path, "r0".start, "r0"."match", "r1".start, "r1"."match""#
    );
}

#[test]
fn same_name_inside_is_an_identity_join() {
    assert_eq!(
        show(&rust(), "((call_expression function: (identifier) @callee) @call\n  (#has-ancestor? @call (function_item body: (block (expression_statement (call_expression) @call)))))"),
        r#"0: ((call_expression function: (identifier) @callee) @call @__root)
1: ((function_item body: (block (expression_statement (call_expression) @call))) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_callee"."start" AS "callee__start",
  "c0_callee"."end" AS "callee__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_callee".text) AS "callee__text",
  "c0_call"."start" AS "call__start",
  "c0_call"."end" AS "call__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_call".text) AS "call__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_callee" ON "c0_callee".file = "r0".file AND "c0_callee".pattern = 0 AND "c0_callee"."match" = "r0"."match" AND "c0_callee".capture = 2
  LEFT JOIN scmpp_capture AS "c0_call" ON "c0_call".file = "r0".file AND "c0_call".pattern = 0 AND "c0_call"."match" = "r0"."match" AND "c0_call".capture = 3
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  LEFT JOIN scmpp_capture AS "c1_call" ON "c1_call".file = "r1".file AND "c1_call".pattern = 1 AND "c1_call"."match" = "r1"."match" AND "c1_call".capture = 3
  JOIN scmpp_node AS "a0" ON "a0".file = "r1".file AND "a0".pre = "r1".node AND "r1".node < "c0_call".node AND "c0_call".node <= "a0".last
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file
  AND "c1_call".file = "c0_call".file AND "c1_call".node = "c0_call".node)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn three_levels_each() {
    assert_eq!(
        show(&rust(), "((function_item name: (identifier) @fn body: (_) @body)\n  (#has? @body (closure_expression body: (_) @cbody\n    (#has? @cbody (call_expression function: (identifier) @callee (#eq? @callee @fn)) rows: each))\n    rows: each))"),
        r#"0: ((function_item name: (identifier) @fn body: (_) @body) @__root)
1: ((closure_expression body: (_) @cbody) @__root)
2: ((call_expression function: (identifier) @callee) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_fn"."start" AS "fn__start",
  "c0_fn"."end" AS "fn__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_fn".text) AS "fn__text",
  "c0_body"."start" AS "body__start",
  "c0_body"."end" AS "body__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_body".text) AS "body__text",
  "c1_cbody"."start" AS "cbody__start",
  "c1_cbody"."end" AS "cbody__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c1_cbody".text) AS "cbody__text",
  "c2_callee"."start" AS "callee__start",
  "c2_callee"."end" AS "callee__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c2_callee".text) AS "callee__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_fn" ON "c0_fn".file = "r0".file AND "c0_fn".pattern = 0 AND "c0_fn"."match" = "r0"."match" AND "c0_fn".capture = 2
  LEFT JOIN scmpp_capture AS "c0_body" ON "c0_body".file = "r0".file AND "c0_body".pattern = 0 AND "c0_body"."match" = "r0"."match" AND "c0_body".capture = 3
  JOIN scmpp_capture AS "r1" ON "r1".pattern = 1 AND "r1".capture = 1 AND "r1".file = "r0".file
  LEFT JOIN scmpp_capture AS "c1_cbody" ON "c1_cbody".file = "r1".file AND "c1_cbody".pattern = 1 AND "c1_cbody"."match" = "r1"."match" AND "c1_cbody".capture = 4
  JOIN scmpp_capture AS "r2" ON "r2".pattern = 2 AND "r2".capture = 1 AND "r2".file = "r1".file
  LEFT JOIN scmpp_capture AS "c2_callee" ON "c2_callee".file = "r2".file AND "c2_callee".pattern = 2 AND "c2_callee"."match" = "r2"."match" AND "c2_callee".capture = 5
  JOIN scmpp_node AS "d0" ON "d0".file = "c1_cbody".file AND "d0".pre = "c1_cbody".node AND "r2".node > "d0".pre AND "r2".node <= "d0".last
  JOIN scmpp_node AS "d1" ON "d1".file = "c0_body".file AND "d1".pre = "c0_body".node AND "r1".node > "d1".pre AND "r1".node <= "d1".last
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND "c2_callee".text = "c0_fn".text
ORDER BY path, "r0".start, "r0"."match", "r1".start, "r1"."match", "r2".start, "r2"."match""#
    );
}

#[test]
fn contains_and_kind_lists_lower_too() {
    assert_eq!(
        show(&rust(), "((string_literal) @s (#contains? @s \"seed\") (#has-ancestor? @s function_item closure_expression))"),
        r#"0: ((string_literal) @s @__root)
1: ([(function_item) (closure_expression)] @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_s"."start" AS "s__start",
  "c0_s"."end" AS "s__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_s".text) AS "s__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_s" ON "c0_s".file = "r0".file AND "c0_s".pattern = 0 AND "c0_s"."match" = "r0"."match" AND "c0_s".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND "c0_s".text IN (SELECT t.id FROM scmpp_dict_text AS t WHERE instr(t.text, 'seed') > 0)
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  JOIN scmpp_node AS "a0" ON "a0".file = "r1".file AND "a0".pre = "r1".node AND "r1".node < "c0_s".node AND "c0_s".node <= "a0".last
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file)
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn match_on_an_enclosing_capture() {
    assert_eq!(
        show(&rust(), "((function_item name: (identifier) @fn) @f\n  (#has? @f (call_expression function: (identifier) @c (#not-match? @fn \"^test_\"))))"),
        r#"0: ((function_item name: (identifier) @fn) @f @__root)
1: ((call_expression function: (identifier) @c) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_fn"."start" AS "fn__start",
  "c0_fn"."end" AS "fn__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_fn".text) AS "fn__text",
  "c0_f"."start" AS "f__start",
  "c0_f"."end" AS "f__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_f".text) AS "f__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_fn" ON "c0_fn".file = "r0".file AND "c0_fn".pattern = 0 AND "c0_fn"."match" = "r0"."match" AND "c0_fn".capture = 2
  LEFT JOIN scmpp_capture AS "c0_f" ON "c0_f".file = "r0".file AND "c0_f".pattern = 0 AND "c0_f"."match" = "r0"."match" AND "c0_f".capture = 3
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  JOIN scmpp_node AS "d0" ON "d0".file = "c0_f".file AND "d0".pre = "c0_f".node AND "r1".node > "d0".pre AND "r1".node <= "d0".last
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file
  AND NOT COALESCE(("c0_fn".text IN (SELECT t.id FROM scmpp_dict_text AS t WHERE regexp('^test_', t.text))), 0))
ORDER BY path, "r0".start, "r0"."match""#
    );
}

/// An absent optional capture makes a text test NULL: the positive form drops the match,
/// the `not-` form keeps it (`NOT COALESCE(.., 0)`).
#[test]
fn not_forms_hold_on_an_absent_capture() {
    assert_eq!(
        show(&rust(), "((arguments (integer_literal)? @n) @args (#not-contains? @n \"9\")\n  (#has-parent? @args ((call_expression function: (identifier) @f) (#not-eq? @f @n))))"),
        r#"0: ((arguments (integer_literal)? @n) @args @__root)
1: ((call_expression function: (identifier) @f) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_n"."start" AS "n__start",
  "c0_n"."end" AS "n__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_n".text) AS "n__text",
  "c0_args"."start" AS "args__start",
  "c0_args"."end" AS "args__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_args".text) AS "args__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_n" ON "c0_n".file = "r0".file AND "c0_n".pattern = 0 AND "c0_n"."match" = "r0"."match" AND "c0_n".capture = 2
  LEFT JOIN scmpp_capture AS "c0_args" ON "c0_args".file = "r0".file AND "c0_args".pattern = 0 AND "c0_args"."match" = "r0"."match" AND "c0_args".capture = 3
WHERE "r0".pattern = 0
  AND "r0".capture = 1
  AND NOT COALESCE(("c0_n".text IN (SELECT t.id FROM scmpp_dict_text AS t WHERE instr(t.text, '9') > 0)), 0)
  AND EXISTS (SELECT 1 FROM scmpp_capture AS "r1"
  LEFT JOIN scmpp_capture AS "c1_f" ON "c1_f".file = "r1".file AND "c1_f".pattern = 1 AND "c1_f"."match" = "r1"."match" AND "c1_f".capture = 4
  JOIN scmpp_node AS "e0" ON "e0".file = "c0_args".file AND "e0".pre = "c0_args".node AND "e0".parent = "r1".node
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file
  AND NOT COALESCE(("c1_f".text = "c0_n".text), 0))
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn rows_list_aggregates_one_array_per_outer_match() {
    assert_eq!(
        show(&rust(), "((function_item name: (identifier) @name) @fn\n  (#follows? @fn ((line_comment) @comment) stopBy: (function_item) rows: list))"),
        r#"0: ((function_item name: (identifier) @name) @fn @__root)
1: ((line_comment) @comment @__root)
2: ((function_item) @__root)
--
SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_name"."start" AS "name__start",
  "c0_name"."end" AS "name__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_name".text) AS "name__text",
  "c0_fn"."start" AS "fn__start",
  "c0_fn"."end" AS "fn__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_fn".text) AS "fn__text",
  (SELECT json_group_array(json_object(
    'comment__start', "c1_comment"."start",
    'comment__end', "c1_comment"."end",
    'comment__text', (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c1_comment".text)) ORDER BY "r1".node, "r1"."match")
  FROM scmpp_capture AS "r1"
  LEFT JOIN scmpp_capture AS "c1_comment" ON "c1_comment".file = "r1".file AND "c1_comment".pattern = 1 AND "c1_comment"."match" = "r1"."match" AND "c1_comment".capture = 4
  JOIN scmpp_node AS "e0a" ON "e0a".file = "c0_fn".file AND "e0a".pre = "c0_fn".node
  JOIN scmpp_node AS "e0b" ON "e0b".file = "r1".file AND "e0b".pre = "r1".node AND "e0b".parent = "e0a".parent AND "e0b".sib < "e0a".sib AND NOT EXISTS (SELECT 1 FROM scmpp_capture AS "r2"
  JOIN scmpp_node AS "e0c" ON "e0c".file = "r2".file AND "e0c".pre = "r2".node
  WHERE "r2".pattern = 2
  AND "r2".capture = 1
  AND "r2".file = "e0a".file
  AND "e0c".parent = "e0a".parent
  AND "e0c".sib > "e0b".sib
  AND "e0c".sib < "e0a".sib)
  WHERE "r1".pattern = 1
  AND "r1".capture = 1
  AND "r1".file = "r0".file) AS "comment__list"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_name" ON "c0_name".file = "r0".file AND "c0_name".pattern = 0 AND "c0_name"."match" = "r0"."match" AND "c0_name".capture = 2
  LEFT JOIN scmpp_capture AS "c0_fn" ON "c0_fn".file = "r0".file AND "c0_fn".pattern = 0 AND "c0_fn"."match" = "r0"."match" AND "c0_fn".capture = 3
WHERE "r0".pattern = 0
  AND "r0".capture = 1
ORDER BY path, "r0".start, "r0"."match""#
    );
}

#[test]
fn errors_name_the_rule() {
    let rows = [
        "((function_item) @f (#has? @f (identifier) @x rows: each) (#has? @f (call_expression (identifier) @x) rows: each))",
        "((function_item) @f (#has? @f ((identifier) @x) rows: each) (#has? @f ((call_expression (identifier) @x)) rows: each))",
        "((block) @b (#has? @b neighbor))",
        "((block) @b (#has? @b (identifier) field: nonesuch))",
        "((block) @__root)",
        "((block) @b (#has? @b (identifier (#eq? @nowhere \"x\"))))",
        "((block) @b (#has-parent? @b (function_item) stopBy: neighbor))",
        "((block) @b (#has? @b (identifier))",
        "(block) (identifier)",
        "((block) @b (#has? @b (identifier) rows: list))",
    ];
    let actual = rows.iter().map(|query| error(&rust(), query)).collect::<Vec<_>>().join("\n");
    assert_eq!(
        actual,
        r#"scm++: #has? at byte 20: unexpected argument Capture("x")
scm++: @x is bound by two levels; use two names and #eq?
scm++ level 1 `((neighbor) @__root)`: Query error at 1:3. Invalid node type "neighbor"
scm++: #has? at byte 12: unknown field nonesuch
scm++: @__root is reserved
scm++: #eq? at byte 34: unknown capture @nowhere
scm++: #has-parent? at byte 12: has-parent is one step; stopBy does not apply
scm++ syntax at byte 0: unbalanced open
scm++ syntax at byte 0: a level needs exactly one root pattern: `(block) (identifier)`
scm++: rows: list needs a @capture in its target"#
    );
}
