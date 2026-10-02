use hafley_scm::scmpp::compile;

fn rust() -> tree_sitter::Language {
    tree_sitter_rust::LANGUAGE.into()
}

const SCHEMA: &str = "CREATE TABLE capture (_input_path TEXT, _content_id TEXT, pattern INTEGER, \"match\" INTEGER, capture TEXT, kind TEXT, text TEXT, start INTEGER, \"end\" INTEGER);
CREATE TABLE edge (_content_id TEXT, family TEXT, from__start INTEGER, from__end INTEGER, from_kind TEXT, to__start INTEGER, to__end INTEGER, to_kind TEXT, field TEXT, \"index\" INTEGER, named_index INTEGER);";

/// Flat pattern texts, then the SQL; the SQL is prepared against the capture/edge columns.
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
SELECT "r0"._input_path AS path,
  "c0_callee"."start" AS "callee__start",
  "c0_callee"."end" AS "callee__end",
  "c0_callee"."text" AS "callee__text",
  "c0_args"."start" AS "args__start",
  "c0_args"."end" AS "args__end",
  "c0_args"."text" AS "args__text",
  "c0_call"."start" AS "call__start",
  "c0_call"."end" AS "call__end",
  "c0_call"."text" AS "call__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_callee" ON "c0_callee"._input_path = "r0"._input_path AND "c0_callee".pattern = 0 AND "c0_callee"."match" = "r0"."match" AND "c0_callee".capture = 'callee'
  LEFT JOIN capture AS "c0_args" ON "c0_args"._input_path = "r0"._input_path AND "c0_args".pattern = 0 AND "c0_args"."match" = "r0"."match" AND "c0_args".capture = 'args'
  LEFT JOIN capture AS "c0_call" ON "c0_call"._input_path = "r0"._input_path AND "c0_call".pattern = 0 AND "c0_call"."match" = "r0"."match" AND "c0_call".capture = 'call'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
    );
}

#[test]
fn has_parent_is_one_edge() {
    assert_eq!(
        show(&rust(), "((identifier) @x (#has-parent? @x let_declaration))"),
        r#"0: ((identifier) @x @__root)
1: ((let_declaration) @__root)
--
SELECT "r0"._input_path AS path,
  "c0_x"."start" AS "x__start",
  "c0_x"."end" AS "x__end",
  "c0_x"."text" AS "x__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_x" ON "c0_x"._input_path = "r0"._input_path AND "c0_x".pattern = 0 AND "c0_x"."match" = "r0"."match" AND "c0_x".capture = 'x'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND EXISTS (SELECT 1 FROM capture AS "r1"
  JOIN edge AS "e0" ON "e0".family = 'cst' AND "e0"._content_id = "c0_x"._content_id AND "e0".to__start = "c0_x".start AND "e0".to__end = "c0_x"."end" AND "e0".to_kind = "c0_x".kind AND "e0".from__start = "r1".start AND "e0".from__end = "r1"."end" AND "e0".from_kind = "r1".kind
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._input_path = "r0"._input_path)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
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
WITH RECURSIVE
walk1(cid, fs, fe, fk, start, "end", kind, field) AS (
    SELECT s.cid, s.start, s."end", s.kind, e.from__start, e.from__end, e.from_kind, e.field
    FROM (SELECT DISTINCT _content_id AS cid, start, "end", kind FROM capture WHERE pattern = 0 AND capture = 'x') AS s
    JOIN edge AS e ON e._content_id = s.cid AND e.family = 'cst' AND e.to__start = s.start AND e.to__end = s."end" AND e.to_kind = s.kind
    UNION
    SELECT w.cid, w.fs, w.fe, w.fk, e.from__start, e.from__end, e.from_kind, e.field
    FROM walk1 AS w JOIN edge AS e ON e._content_id = w.cid AND e.family = 'cst' AND e.to__start = w.start AND e.to__end = w."end" AND e.to_kind = w.kind
    WHERE NOT EXISTS (SELECT 1 FROM capture AS "r2"
  WHERE "r2".pattern = 2
  AND "r2".capture = '__root'
  AND "r2"._content_id = w.cid AND "r2".start = w.start AND "r2"."end" = w."end" AND "r2".kind = w.kind))
SELECT "r0"._input_path AS path,
  "c0_x"."start" AS "x__start",
  "c0_x"."end" AS "x__end",
  "c0_x"."text" AS "x__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_x" ON "c0_x"._input_path = "r0"._input_path AND "c0_x".pattern = 0 AND "c0_x"."match" = "r0"."match" AND "c0_x".capture = 'x'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND EXISTS (SELECT 1 FROM capture AS "r1"
  JOIN walk1 AS "w0" ON "w0".cid = "c0_x"._content_id AND "w0".fs = "c0_x".start AND "w0".fe = "c0_x"."end" AND "w0".fk = "c0_x".kind AND "w0".start = "r1".start AND "w0"."end" = "r1"."end" AND "w0".kind = "r1".kind
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._input_path = "r0"._input_path)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
    );
}

#[test]
fn has_recurses_down() {
    assert_eq!(
        show(&rust(), "((function_item) @f (#has? @f (macro_invocation)))"),
        r#"0: ((function_item) @f @__root)
1: ((macro_invocation) @__root)
--
WITH RECURSIVE
walk1(cid, fs, fe, fk, start, "end", kind, field) AS (
    SELECT s.cid, s.start, s."end", s.kind, e.to__start, e.to__end, e.to_kind, e.field
    FROM (SELECT DISTINCT _content_id AS cid, start, "end", kind FROM capture WHERE pattern = 0 AND capture = 'f') AS s
    JOIN edge AS e ON e._content_id = s.cid AND e.family = 'cst' AND e.from__start = s.start AND e.from__end = s."end" AND e.from_kind = s.kind
    UNION
    SELECT w.cid, w.fs, w.fe, w.fk, e.to__start, e.to__end, e.to_kind, e.field
    FROM walk1 AS w JOIN edge AS e ON e._content_id = w.cid AND e.family = 'cst' AND e.from__start = w.start AND e.from__end = w."end" AND e.from_kind = w.kind)
SELECT "r0"._input_path AS path,
  "c0_f"."start" AS "f__start",
  "c0_f"."end" AS "f__end",
  "c0_f"."text" AS "f__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_f" ON "c0_f"._input_path = "r0"._input_path AND "c0_f".pattern = 0 AND "c0_f"."match" = "r0"."match" AND "c0_f".capture = 'f'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND EXISTS (SELECT 1 FROM capture AS "r1"
  JOIN walk1 AS "w0" ON "w0".cid = "c0_f"._content_id AND "w0".fs = "c0_f".start AND "w0".fe = "c0_f"."end" AND "w0".fk = "c0_f".kind AND "w0".start = "r1".start AND "w0"."end" = "r1"."end" AND "w0".kind = "r1".kind
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._input_path = "r0"._input_path)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
    );
}

#[test]
fn stop_by_neighbor_is_one_step() {
    assert_eq!(
        show(&rust(), "((block) @b (#has? @b (expression_statement) stopBy: neighbor))"),
        r#"0: ((block) @b @__root)
1: ((expression_statement) @__root)
--
SELECT "r0"._input_path AS path,
  "c0_b"."start" AS "b__start",
  "c0_b"."end" AS "b__end",
  "c0_b"."text" AS "b__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_b" ON "c0_b"._input_path = "r0"._input_path AND "c0_b".pattern = 0 AND "c0_b"."match" = "r0"."match" AND "c0_b".capture = 'b'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND EXISTS (SELECT 1 FROM capture AS "r1"
  JOIN edge AS "e0" ON "e0".family = 'cst' AND "e0"._content_id = "r1"._content_id AND "e0".to__start = "r1".start AND "e0".to__end = "r1"."end" AND "e0".to_kind = "r1".kind AND "e0".from__start = "c0_b".start AND "e0".from__end = "c0_b"."end" AND "e0".from_kind = "c0_b".kind
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._input_path = "r0"._input_path)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
    );
}

#[test]
fn precedes_neighbor_compares_named_index() {
    assert_eq!(
        show(&rust(), "((expression_statement) @c (#precedes? @c (let_declaration) stopBy: neighbor))"),
        r#"0: ((expression_statement) @c @__root)
1: ((let_declaration) @__root)
--
SELECT "r0"._input_path AS path,
  "c0_c"."start" AS "c__start",
  "c0_c"."end" AS "c__end",
  "c0_c"."text" AS "c__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_c" ON "c0_c"._input_path = "r0"._input_path AND "c0_c".pattern = 0 AND "c0_c"."match" = "r0"."match" AND "c0_c".capture = 'c'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND EXISTS (SELECT 1 FROM capture AS "r1"
  JOIN edge AS "e0a" ON "e0a".family = 'cst' AND "e0a"._content_id = "c0_c"._content_id AND "e0a".to__start = "c0_c".start AND "e0a".to__end = "c0_c"."end" AND "e0a".to_kind = "c0_c".kind
  JOIN edge AS "e0b" ON "e0b".family = 'cst' AND "e0b"._content_id = "r1"._content_id AND "e0b".to__start = "r1".start AND "e0b".to__end = "r1"."end" AND "e0b".to_kind = "r1".kind AND "e0b"._content_id = "e0a"._content_id AND "e0b".from__start = "e0a".from__start AND "e0b".from__end = "e0a".from__end AND "e0b".from_kind = "e0a".from_kind AND "e0b".named_index = "e0a".named_index + 1
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._input_path = "r0"._input_path)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
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
SELECT "r0"._input_path AS path,
  "c0_c"."start" AS "c__start",
  "c0_c"."end" AS "c__end",
  "c0_c"."text" AS "c__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_c" ON "c0_c"._input_path = "r0"._input_path AND "c0_c".pattern = 0 AND "c0_c"."match" = "r0"."match" AND "c0_c".capture = 'c'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND EXISTS (SELECT 1 FROM capture AS "r1"
  JOIN edge AS "e0a" ON "e0a".family = 'cst' AND "e0a"._content_id = "c0_c"._content_id AND "e0a".to__start = "c0_c".start AND "e0a".to__end = "c0_c"."end" AND "e0a".to_kind = "c0_c".kind
  JOIN edge AS "e0b" ON "e0b".family = 'cst' AND "e0b"._content_id = "r1"._content_id AND "e0b".to__start = "r1".start AND "e0b".to__end = "r1"."end" AND "e0b".to_kind = "r1".kind AND "e0b"._content_id = "e0a"._content_id AND "e0b".from__start = "e0a".from__start AND "e0b".from__end = "e0a".from__end AND "e0b".from_kind = "e0a".from_kind AND "e0b".named_index < "e0a".named_index AND NOT EXISTS (SELECT 1 FROM edge AS "e0c" WHERE "e0c".family = 'cst' AND "e0c"._content_id = "e0a"._content_id AND "e0c".from__start = "e0a".from__start AND "e0c".from__end = "e0a".from__end AND "e0c".from_kind = "e0a".from_kind
  AND "e0c".named_index > min("e0a".named_index, "e0b".named_index)
  AND "e0c".named_index < max("e0a".named_index, "e0b".named_index)
  AND EXISTS (SELECT 1 FROM capture AS "r2"
  WHERE "r2".pattern = 2
  AND "r2".capture = '__root'
  AND "r2"._content_id = "e0c"._content_id AND "r2".start = "e0c".to__start AND "r2"."end" = "e0c".to__end AND "r2".kind = "e0c".to_kind))
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._input_path = "r0"._input_path)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
    );
}

#[test]
fn nth_child_of_counts_matching_siblings() {
    assert_eq!(
        show(&rust(), "((expression_statement) @c (#nth-child? @c 2 of (expression_statement)))"),
        r#"0: ((expression_statement) @c @__root)
1: ((expression_statement) @__root)
--
WITH RECURSIVE
nth0(cid, start, "end", kind, n) AS (
    SELECT e._content_id, e.to__start, e.to__end, e.to_kind, ROW_NUMBER() OVER (PARTITION BY e._content_id, e.from__start, e.from__end, e.from_kind ORDER BY e.named_index)
    FROM edge AS e WHERE e.family = 'cst' AND e.named_index IS NOT NULL
    AND EXISTS (SELECT 1 FROM capture AS "r1"
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._content_id = e._content_id AND "r1".start = e.to__start AND "r1"."end" = e.to__end AND "r1".kind = e.to_kind))
SELECT "r0"._input_path AS path,
  "c0_c"."start" AS "c__start",
  "c0_c"."end" AS "c__end",
  "c0_c"."text" AS "c__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_c" ON "c0_c"._input_path = "r0"._input_path AND "c0_c".pattern = 0 AND "c0_c"."match" = "r0"."match" AND "c0_c".capture = 'c'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND EXISTS (SELECT 1 FROM nth0 AS s WHERE s.cid = "c0_c"._content_id AND s.start = "c0_c".start AND s."end" = "c0_c"."end" AND s.kind = "c0_c".kind AND s.n = 2)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
    );
}

#[test]
fn nth_child_of_a_supertype_kind() {
    assert_eq!(
        show(&rust(), "((identifier) @x (#nth-child? @x 1 of _expression))"),
        r#"0: ((identifier) @x @__root)
1: ((_expression) @__root)
--
WITH RECURSIVE
nth0(cid, start, "end", kind, n) AS (
    SELECT e._content_id, e.to__start, e.to__end, e.to_kind, ROW_NUMBER() OVER (PARTITION BY e._content_id, e.from__start, e.from__end, e.from_kind ORDER BY e.named_index)
    FROM edge AS e WHERE e.family = 'cst' AND e.named_index IS NOT NULL
    AND EXISTS (SELECT 1 FROM capture AS "r1"
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._content_id = e._content_id AND "r1".start = e.to__start AND "r1"."end" = e.to__end AND "r1".kind = e.to_kind))
SELECT "r0"._input_path AS path,
  "c0_x"."start" AS "x__start",
  "c0_x"."end" AS "x__end",
  "c0_x"."text" AS "x__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_x" ON "c0_x"._input_path = "r0"._input_path AND "c0_x".pattern = 0 AND "c0_x"."match" = "r0"."match" AND "c0_x".capture = 'x'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND EXISTS (SELECT 1 FROM nth0 AS s WHERE s.cid = "c0_x"._content_id AND s.start = "c0_x".start AND s."end" = "c0_x"."end" AND s.kind = "c0_x".kind AND s.n = 1)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
    );
}

#[test]
fn field_sits_on_the_step_next_to_the_target() {
    assert_eq!(
        show(&rust(), "((identifier) @x (#has-ancestor? @x (let_declaration) field: pattern))"),
        r#"0: ((identifier) @x @__root)
1: ((let_declaration) @__root)
--
WITH RECURSIVE
walk1(cid, fs, fe, fk, start, "end", kind, field) AS (
    SELECT s.cid, s.start, s."end", s.kind, e.from__start, e.from__end, e.from_kind, e.field
    FROM (SELECT DISTINCT _content_id AS cid, start, "end", kind FROM capture WHERE pattern = 0 AND capture = 'x') AS s
    JOIN edge AS e ON e._content_id = s.cid AND e.family = 'cst' AND e.to__start = s.start AND e.to__end = s."end" AND e.to_kind = s.kind
    UNION
    SELECT w.cid, w.fs, w.fe, w.fk, e.from__start, e.from__end, e.from_kind, e.field
    FROM walk1 AS w JOIN edge AS e ON e._content_id = w.cid AND e.family = 'cst' AND e.to__start = w.start AND e.to__end = w."end" AND e.to_kind = w.kind)
SELECT "r0"._input_path AS path,
  "c0_x"."start" AS "x__start",
  "c0_x"."end" AS "x__end",
  "c0_x"."text" AS "x__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_x" ON "c0_x"._input_path = "r0"._input_path AND "c0_x".pattern = 0 AND "c0_x"."match" = "r0"."match" AND "c0_x".capture = 'x'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND EXISTS (SELECT 1 FROM capture AS "r1"
  JOIN walk1 AS "w0" ON "w0".cid = "c0_x"._content_id AND "w0".fs = "c0_x".start AND "w0".fe = "c0_x"."end" AND "w0".fk = "c0_x".kind AND "w0".start = "r1".start AND "w0"."end" = "r1"."end" AND "w0".kind = "r1".kind AND "w0".field = 'pattern'
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._input_path = "r0"._input_path)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
    );
}

#[test]
fn not_binds_nothing() {
    assert_eq!(
        show(&rust(), "((function_item) @f (#not-has? @f (call_expression function: (identifier) @g)))"),
        r#"0: ((function_item) @f @__root)
1: ((call_expression function: (identifier) @g) @__root)
--
WITH RECURSIVE
walk1(cid, fs, fe, fk, start, "end", kind, field) AS (
    SELECT s.cid, s.start, s."end", s.kind, e.to__start, e.to__end, e.to_kind, e.field
    FROM (SELECT DISTINCT _content_id AS cid, start, "end", kind FROM capture WHERE pattern = 0 AND capture = 'f') AS s
    JOIN edge AS e ON e._content_id = s.cid AND e.family = 'cst' AND e.from__start = s.start AND e.from__end = s."end" AND e.from_kind = s.kind
    UNION
    SELECT w.cid, w.fs, w.fe, w.fk, e.to__start, e.to__end, e.to_kind, e.field
    FROM walk1 AS w JOIN edge AS e ON e._content_id = w.cid AND e.family = 'cst' AND e.from__start = w.start AND e.from__end = w."end" AND e.from_kind = w.kind)
SELECT "r0"._input_path AS path,
  "c0_f"."start" AS "f__start",
  "c0_f"."end" AS "f__end",
  "c0_f"."text" AS "f__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_f" ON "c0_f"._input_path = "r0"._input_path AND "c0_f".pattern = 0 AND "c0_f"."match" = "r0"."match" AND "c0_f".capture = 'f'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND NOT EXISTS (SELECT 1 FROM capture AS "r1"
  JOIN walk1 AS "w0" ON "w0".cid = "c0_f"._content_id AND "w0".fs = "c0_f".start AND "w0".fe = "c0_f"."end" AND "w0".fk = "c0_f".kind AND "w0".start = "r1".start AND "w0"."end" = "r1"."end" AND "w0".kind = "r1".kind
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._input_path = "r0"._input_path)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
    );
}

#[test]
fn correlated_eq_rows_each_recursion() {
    assert_eq!(
        show(&rust(), "(function_item name: (identifier) @fn body: (_) @body)\n(#has? @body\n  (call_expression function: (identifier) @callee\n    (#eq? @callee @fn))\n  rows: each)"),
        r#"0: ((function_item name: (identifier) @fn body: (_) @body) @__root)
1: ((call_expression function: (identifier) @callee) @__root)
--
WITH RECURSIVE
walk1(cid, fs, fe, fk, start, "end", kind, field) AS (
    SELECT s.cid, s.start, s."end", s.kind, e.to__start, e.to__end, e.to_kind, e.field
    FROM (SELECT DISTINCT _content_id AS cid, start, "end", kind FROM capture WHERE pattern = 0 AND capture = 'body') AS s
    JOIN edge AS e ON e._content_id = s.cid AND e.family = 'cst' AND e.from__start = s.start AND e.from__end = s."end" AND e.from_kind = s.kind
    UNION
    SELECT w.cid, w.fs, w.fe, w.fk, e.to__start, e.to__end, e.to_kind, e.field
    FROM walk1 AS w JOIN edge AS e ON e._content_id = w.cid AND e.family = 'cst' AND e.from__start = w.start AND e.from__end = w."end" AND e.from_kind = w.kind)
SELECT "r0"._input_path AS path,
  "c0_fn"."start" AS "fn__start",
  "c0_fn"."end" AS "fn__end",
  "c0_fn"."text" AS "fn__text",
  "c0_body"."start" AS "body__start",
  "c0_body"."end" AS "body__end",
  "c0_body"."text" AS "body__text",
  "c1_callee"."start" AS "callee__start",
  "c1_callee"."end" AS "callee__end",
  "c1_callee"."text" AS "callee__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_fn" ON "c0_fn"._input_path = "r0"._input_path AND "c0_fn".pattern = 0 AND "c0_fn"."match" = "r0"."match" AND "c0_fn".capture = 'fn'
  LEFT JOIN capture AS "c0_body" ON "c0_body"._input_path = "r0"._input_path AND "c0_body".pattern = 0 AND "c0_body"."match" = "r0"."match" AND "c0_body".capture = 'body'
  JOIN capture AS "r1" ON "r1".pattern = 1 AND "r1".capture = '__root' AND "r1"._input_path = "r0"._input_path
  LEFT JOIN capture AS "c1_callee" ON "c1_callee"._input_path = "r1"._input_path AND "c1_callee".pattern = 1 AND "c1_callee"."match" = "r1"."match" AND "c1_callee".capture = 'callee'
  JOIN walk1 AS "w0" ON "w0".cid = "c0_body"._content_id AND "w0".fs = "c0_body".start AND "w0".fe = "c0_body"."end" AND "w0".fk = "c0_body".kind AND "w0".start = "r1".start AND "w0"."end" = "r1"."end" AND "w0".kind = "r1".kind
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND "c1_callee".text = "c0_fn".text
ORDER BY "r0"._input_path, "r0".start, "r0"."match", "r1".start, "r1"."match""#
    );
}

#[test]
fn same_name_inside_is_an_identity_join() {
    assert_eq!(
        show(&rust(), "((call_expression function: (identifier) @callee) @call\n  (#has-ancestor? @call (function_item body: (block (expression_statement (call_expression) @call)))))"),
        r#"0: ((call_expression function: (identifier) @callee) @call @__root)
1: ((function_item body: (block (expression_statement (call_expression) @call))) @__root)
--
WITH RECURSIVE
walk1(cid, fs, fe, fk, start, "end", kind, field) AS (
    SELECT s.cid, s.start, s."end", s.kind, e.from__start, e.from__end, e.from_kind, e.field
    FROM (SELECT DISTINCT _content_id AS cid, start, "end", kind FROM capture WHERE pattern = 0 AND capture = 'call') AS s
    JOIN edge AS e ON e._content_id = s.cid AND e.family = 'cst' AND e.to__start = s.start AND e.to__end = s."end" AND e.to_kind = s.kind
    UNION
    SELECT w.cid, w.fs, w.fe, w.fk, e.from__start, e.from__end, e.from_kind, e.field
    FROM walk1 AS w JOIN edge AS e ON e._content_id = w.cid AND e.family = 'cst' AND e.to__start = w.start AND e.to__end = w."end" AND e.to_kind = w.kind)
SELECT "r0"._input_path AS path,
  "c0_callee"."start" AS "callee__start",
  "c0_callee"."end" AS "callee__end",
  "c0_callee"."text" AS "callee__text",
  "c0_call"."start" AS "call__start",
  "c0_call"."end" AS "call__end",
  "c0_call"."text" AS "call__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_callee" ON "c0_callee"._input_path = "r0"._input_path AND "c0_callee".pattern = 0 AND "c0_callee"."match" = "r0"."match" AND "c0_callee".capture = 'callee'
  LEFT JOIN capture AS "c0_call" ON "c0_call"._input_path = "r0"._input_path AND "c0_call".pattern = 0 AND "c0_call"."match" = "r0"."match" AND "c0_call".capture = 'call'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND EXISTS (SELECT 1 FROM capture AS "r1"
  LEFT JOIN capture AS "c1_call" ON "c1_call"._input_path = "r1"._input_path AND "c1_call".pattern = 1 AND "c1_call"."match" = "r1"."match" AND "c1_call".capture = 'call'
  JOIN walk1 AS "w0" ON "w0".cid = "c0_call"._content_id AND "w0".fs = "c0_call".start AND "w0".fe = "c0_call"."end" AND "w0".fk = "c0_call".kind AND "w0".start = "r1".start AND "w0"."end" = "r1"."end" AND "w0".kind = "r1".kind
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._input_path = "r0"._input_path
  AND "c1_call"._content_id = "c0_call"._content_id AND "c1_call".start = "c0_call".start AND "c1_call"."end" = "c0_call"."end" AND "c1_call".kind = "c0_call".kind)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
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
WITH RECURSIVE
walk1(cid, fs, fe, fk, start, "end", kind, field) AS (
    SELECT s.cid, s.start, s."end", s.kind, e.to__start, e.to__end, e.to_kind, e.field
    FROM (SELECT DISTINCT _content_id AS cid, start, "end", kind FROM capture WHERE pattern = 1 AND capture = 'cbody') AS s
    JOIN edge AS e ON e._content_id = s.cid AND e.family = 'cst' AND e.from__start = s.start AND e.from__end = s."end" AND e.from_kind = s.kind
    UNION
    SELECT w.cid, w.fs, w.fe, w.fk, e.to__start, e.to__end, e.to_kind, e.field
    FROM walk1 AS w JOIN edge AS e ON e._content_id = w.cid AND e.family = 'cst' AND e.from__start = w.start AND e.from__end = w."end" AND e.from_kind = w.kind),
walk3(cid, fs, fe, fk, start, "end", kind, field) AS (
    SELECT s.cid, s.start, s."end", s.kind, e.to__start, e.to__end, e.to_kind, e.field
    FROM (SELECT DISTINCT _content_id AS cid, start, "end", kind FROM capture WHERE pattern = 0 AND capture = 'body') AS s
    JOIN edge AS e ON e._content_id = s.cid AND e.family = 'cst' AND e.from__start = s.start AND e.from__end = s."end" AND e.from_kind = s.kind
    UNION
    SELECT w.cid, w.fs, w.fe, w.fk, e.to__start, e.to__end, e.to_kind, e.field
    FROM walk3 AS w JOIN edge AS e ON e._content_id = w.cid AND e.family = 'cst' AND e.from__start = w.start AND e.from__end = w."end" AND e.from_kind = w.kind)
SELECT "r0"._input_path AS path,
  "c0_fn"."start" AS "fn__start",
  "c0_fn"."end" AS "fn__end",
  "c0_fn"."text" AS "fn__text",
  "c0_body"."start" AS "body__start",
  "c0_body"."end" AS "body__end",
  "c0_body"."text" AS "body__text",
  "c1_cbody"."start" AS "cbody__start",
  "c1_cbody"."end" AS "cbody__end",
  "c1_cbody"."text" AS "cbody__text",
  "c2_callee"."start" AS "callee__start",
  "c2_callee"."end" AS "callee__end",
  "c2_callee"."text" AS "callee__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_fn" ON "c0_fn"._input_path = "r0"._input_path AND "c0_fn".pattern = 0 AND "c0_fn"."match" = "r0"."match" AND "c0_fn".capture = 'fn'
  LEFT JOIN capture AS "c0_body" ON "c0_body"._input_path = "r0"._input_path AND "c0_body".pattern = 0 AND "c0_body"."match" = "r0"."match" AND "c0_body".capture = 'body'
  JOIN capture AS "r1" ON "r1".pattern = 1 AND "r1".capture = '__root' AND "r1"._input_path = "r0"._input_path
  LEFT JOIN capture AS "c1_cbody" ON "c1_cbody"._input_path = "r1"._input_path AND "c1_cbody".pattern = 1 AND "c1_cbody"."match" = "r1"."match" AND "c1_cbody".capture = 'cbody'
  JOIN capture AS "r2" ON "r2".pattern = 2 AND "r2".capture = '__root' AND "r2"._input_path = "r1"._input_path
  LEFT JOIN capture AS "c2_callee" ON "c2_callee"._input_path = "r2"._input_path AND "c2_callee".pattern = 2 AND "c2_callee"."match" = "r2"."match" AND "c2_callee".capture = 'callee'
  JOIN walk1 AS "w0" ON "w0".cid = "c1_cbody"._content_id AND "w0".fs = "c1_cbody".start AND "w0".fe = "c1_cbody"."end" AND "w0".fk = "c1_cbody".kind AND "w0".start = "r2".start AND "w0"."end" = "r2"."end" AND "w0".kind = "r2".kind
  JOIN walk3 AS "w2" ON "w2".cid = "c0_body"._content_id AND "w2".fs = "c0_body".start AND "w2".fe = "c0_body"."end" AND "w2".fk = "c0_body".kind AND "w2".start = "r1".start AND "w2"."end" = "r1"."end" AND "w2".kind = "r1".kind
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND "c2_callee".text = "c0_fn".text
ORDER BY "r0"._input_path, "r0".start, "r0"."match", "r1".start, "r1"."match", "r2".start, "r2"."match""#
    );
}

#[test]
fn contains_and_kind_lists_lower_too() {
    assert_eq!(
        show(&rust(), "((string_literal) @s (#contains? @s \"seed\") (#has-ancestor? @s function_item closure_expression))"),
        r#"0: ((string_literal) @s @__root)
1: ([(function_item) (closure_expression)] @__root)
--
WITH RECURSIVE
walk1(cid, fs, fe, fk, start, "end", kind, field) AS (
    SELECT s.cid, s.start, s."end", s.kind, e.from__start, e.from__end, e.from_kind, e.field
    FROM (SELECT DISTINCT _content_id AS cid, start, "end", kind FROM capture WHERE pattern = 0 AND capture = 's') AS s
    JOIN edge AS e ON e._content_id = s.cid AND e.family = 'cst' AND e.to__start = s.start AND e.to__end = s."end" AND e.to_kind = s.kind
    UNION
    SELECT w.cid, w.fs, w.fe, w.fk, e.from__start, e.from__end, e.from_kind, e.field
    FROM walk1 AS w JOIN edge AS e ON e._content_id = w.cid AND e.family = 'cst' AND e.to__start = w.start AND e.to__end = w."end" AND e.to_kind = w.kind)
SELECT "r0"._input_path AS path,
  "c0_s"."start" AS "s__start",
  "c0_s"."end" AS "s__end",
  "c0_s"."text" AS "s__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_s" ON "c0_s"._input_path = "r0"._input_path AND "c0_s".pattern = 0 AND "c0_s"."match" = "r0"."match" AND "c0_s".capture = 's'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND instr("c0_s".text, 'seed') > 0
  AND EXISTS (SELECT 1 FROM capture AS "r1"
  JOIN walk1 AS "w0" ON "w0".cid = "c0_s"._content_id AND "w0".fs = "c0_s".start AND "w0".fe = "c0_s"."end" AND "w0".fk = "c0_s".kind AND "w0".start = "r1".start AND "w0"."end" = "r1"."end" AND "w0".kind = "r1".kind
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._input_path = "r0"._input_path)
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
    );
}

#[test]
fn match_on_an_enclosing_capture() {
    assert_eq!(
        show(&rust(), "((function_item name: (identifier) @fn) @f\n  (#has? @f (call_expression function: (identifier) @c (#not-match? @fn \"^test_\"))))"),
        r#"0: ((function_item name: (identifier) @fn) @f @__root)
1: ((call_expression function: (identifier) @c) @__root)
--
WITH RECURSIVE
walk1(cid, fs, fe, fk, start, "end", kind, field) AS (
    SELECT s.cid, s.start, s."end", s.kind, e.to__start, e.to__end, e.to_kind, e.field
    FROM (SELECT DISTINCT _content_id AS cid, start, "end", kind FROM capture WHERE pattern = 0 AND capture = 'f') AS s
    JOIN edge AS e ON e._content_id = s.cid AND e.family = 'cst' AND e.from__start = s.start AND e.from__end = s."end" AND e.from_kind = s.kind
    UNION
    SELECT w.cid, w.fs, w.fe, w.fk, e.to__start, e.to__end, e.to_kind, e.field
    FROM walk1 AS w JOIN edge AS e ON e._content_id = w.cid AND e.family = 'cst' AND e.from__start = w.start AND e.from__end = w."end" AND e.from_kind = w.kind)
SELECT "r0"._input_path AS path,
  "c0_fn"."start" AS "fn__start",
  "c0_fn"."end" AS "fn__end",
  "c0_fn"."text" AS "fn__text",
  "c0_f"."start" AS "f__start",
  "c0_f"."end" AS "f__end",
  "c0_f"."text" AS "f__text"
FROM capture AS "r0"
  LEFT JOIN capture AS "c0_fn" ON "c0_fn"._input_path = "r0"._input_path AND "c0_fn".pattern = 0 AND "c0_fn"."match" = "r0"."match" AND "c0_fn".capture = 'fn'
  LEFT JOIN capture AS "c0_f" ON "c0_f"._input_path = "r0"._input_path AND "c0_f".pattern = 0 AND "c0_f"."match" = "r0"."match" AND "c0_f".capture = 'f'
WHERE "r0".pattern = 0
  AND "r0".capture = '__root'
  AND EXISTS (SELECT 1 FROM capture AS "r1"
  JOIN walk1 AS "w0" ON "w0".cid = "c0_f"._content_id AND "w0".fs = "c0_f".start AND "w0".fe = "c0_f"."end" AND "w0".fk = "c0_f".kind AND "w0".start = "r1".start AND "w0"."end" = "r1"."end" AND "w0".kind = "r1".kind
  WHERE "r1".pattern = 1
  AND "r1".capture = '__root'
  AND "r1"._input_path = "r0"._input_path
  AND NOT (regexp('^test_', "c0_fn".text)))
ORDER BY "r0"._input_path, "r0".start, "r0"."match""#
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
scm++ syntax at byte 0: a level needs exactly one root pattern: `(block) (identifier)`"#
    );
}
