#![cfg(feature = "cli")]
//! `ryii query --scmpp` end to end: real binary, real Rust grammar, rows from the lowered SQL.

use std::process::Command;

const RUST: &str = "\
fn fact(n: u32) -> u32 {
    if n == 0 { 1 } else { n * fact(n - 1) }
}

fn helper() -> u32 {
    let seed = \"seed\";
    let total = fact(3);
    other();
    total + other()
}

fn other() -> u32 {
    let run = || other();
    run() + other()
}
";

const RECURSION: &str = "\
; a call inside the body that names the function itself
(function_item name: (identifier) @fn body: (_) @body)
(#has? @body
  (call_expression function: (identifier) @callee
    (#eq? @callee @fn))
  rows: each)
";

fn run(rust: &str, query: &str, extra: &[&str]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("fixture.rs");
    let scm = dir.path().join("query.scm");
    std::fs::write(&source, rust).unwrap();
    std::fs::write(&scm, query).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("query")
        .arg("--scmpp")
        .arg(&scm)
        .args(extra)
        .arg(&source)
        .env("DL_TRAIL", "0")
        .output()
        .expect("ryii runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().replace(&source.display().to_string(), "fixture.rs")
}

/// One line per row: every `NAME__text` column as `NAME=text`, whitespace collapsed.
fn rows(query: &str) -> String {
    run(RUST, query, &[])
        .lines()
        .map(|line| {
            let row: serde_json::Map<String, serde_json::Value> = serde_json::from_str(line).unwrap();
            let cells = row
                .iter()
                .filter_map(|(key, value)| {
                    let name = key.strip_suffix("__text")?;
                    let text = value.as_str().unwrap_or("null").split_whitespace().collect::<Vec<_>>().join(" ");
                    Some(format!("{name}={text}"))
                })
                .collect::<Vec<_>>();
            cells.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn recursion_example_full_rows() {
    assert_eq!(
        run(RUST, RECURSION, &[]),
        r#"{"path":"fixture.rs","fn__start":3,"fn__end":7,"fn__text":"fact","body__start":23,"body__end":71,"body__text":"{\n    if n == 0 { 1 } else { n * fact(n - 1) }\n}","callee__start":56,"callee__end":60,"callee__text":"fact"}
{"path":"fixture.rs","fn__start":181,"fn__end":186,"fn__text":"other","body__start":196,"body__end":245,"body__text":"{\n    let run = || other();\n    run() + other()\n}","callee__start":215,"callee__end":220,"callee__text":"other"}
{"path":"fixture.rs","fn__start":181,"fn__end":186,"fn__text":"other","body__start":196,"body__end":245,"body__text":"{\n    let run = || other();\n    run() + other()\n}","callee__start":236,"callee__end":241,"callee__text":"other"}
"#
    );
}

#[test]
fn rows_per_lowering_rule() {
    let cases = [
        ("recursion rows: each", RECURSION.to_string()),
        ("recursion rows: first", RECURSION.replace("rows: each", "rows: first")),
        (
            "not- binds nothing",
            "(function_item name: (identifier) @fn body: (_) @body)
             (#not-has? @body (call_expression function: (identifier) @callee (#eq? @callee @fn)))"
                .to_string(),
        ),
        (
            "three levels",
            "(function_item name: (identifier) @fn body: (_) @body)
             (#has? @body ((closure_expression) @closure
               (#has? @closure (call_expression function: (identifier) @callee (#eq? @callee @fn)) rows: each))
               rows: each)"
                .to_string(),
        ),
        (
            "identity join",
            "((call_expression function: (identifier) @callee) @call
              (#has-ancestor? @call (function_item body: (block (expression_statement (call_expression) @call)))))"
                .to_string(),
        ),
        (
            "two names, no identity",
            "((call_expression function: (identifier) @callee) @call
              (#has-ancestor? @call (function_item body: (block (expression_statement (call_expression) @inner)))))"
                .to_string(),
        ),
        ("has-parent", "((identifier) @x (#has-parent? @x let_declaration))".to_string()),
        ("has without field", "((let_declaration) @l (#has? @l (identifier)))".to_string()),
        ("has field: function", "((let_declaration) @l (#has? @l (identifier) field: function))".to_string()),
        ("inside without field", "((identifier) @x (#has-ancestor? @x (let_declaration)))".to_string()),
        ("inside field: value", "((identifier) @x (#has-ancestor? @x (let_declaration) field: value))".to_string()),
        (
            "inside, stopBy pattern",
            "((call_expression function: (identifier) @x)
              (#has-ancestor? @x (function_item) stopBy: (closure_expression)))"
                .to_string(),
        ),
        ("has, stopBy: neighbor", "((block) @b (#has? @b (let_declaration) stopBy: neighbor))".to_string()),
        ("precedes neighbor", "((let_declaration) @l (#precedes? @l (expression_statement) stopBy: neighbor))".to_string()),
        ("follows end", "((expression_statement) @s (#follows? @s (let_declaration)))".to_string()),
        ("nth-child of a supertype", "((call_expression) @c (#nth-child? @c 2 of _expression))".to_string()),
        ("nth-child", "((let_declaration) @l (#nth-child? @l 2))".to_string()),
        (
            "kind list and contains",
            "((string_literal) @s (#contains? @s \"ee\") (#has-ancestor? @s function_item closure_expression))".to_string(),
        ),
        (
            "match on an enclosing capture",
            "((function_item name: (identifier) @fn) @f
              (#has? @f (call_expression function: (identifier) @c (#match? @fn \"^h\")) rows: each))"
                .to_string(),
        ),
        ("contains, capture absent", "((arguments (integer_literal)? @n) @args (#contains? @n \"3\"))".to_string()),
        ("not-contains, capture absent", "((arguments (integer_literal)? @n) @args (#not-contains? @n \"3\"))".to_string()),
        (
            "not-eq across levels, capture absent",
            "((arguments (integer_literal)? @n) @args
              (#has-parent? @args ((call_expression function: (identifier) @f) (#not-eq? @f @n))))"
                .to_string(),
        ),
    ];
    let actual = cases
        .iter()
        .map(|(name, query)| format!("== {name}\n{}", rows(query)))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        actual,
        r#"== recursion rows: each
fn=fact body={ if n == 0 { 1 } else { n * fact(n - 1) } } callee=fact
fn=other body={ let run = || other(); run() + other() } callee=other
fn=other body={ let run = || other(); run() + other() } callee=other
== recursion rows: first
fn=fact body={ if n == 0 { 1 } else { n * fact(n - 1) } }
fn=other body={ let run = || other(); run() + other() }
== not- binds nothing
fn=helper body={ let seed = "seed"; let total = fact(3); other(); total + other() }
== three levels
fn=other body={ let run = || other(); run() + other() } closure=|| other() callee=other
== identity join
callee=other call=other()
== two names, no identity
callee=fact call=fact(3)
callee=other call=other()
callee=other call=other()
== has-parent
x=seed
x=total
x=run
== has without field
l=let seed = "seed";
l=let total = fact(3);
l=let run = || other();
== has field: function
l=let total = fact(3);
l=let run = || other();
== inside without field
x=seed
x=total
x=fact
x=run
x=other
== inside field: value
x=fact
x=other
== inside, stopBy pattern
x=fact
x=fact
x=other
x=other
x=run
x=other
== has, stopBy: neighbor
b={ let seed = "seed"; let total = fact(3); other(); total + other() }
b={ let run = || other(); run() + other() }
== precedes neighbor
l=let total = fact(3);
== follows end
s=other();
== nth-child of a supertype
c=fact(n - 1)
c=other()
c=other()
== nth-child
l=let total = fact(3);
== kind list and contains
s="seed"
== match on an enclosing capture
fn=helper f=fn helper() -> u32 { let seed = "seed"; let total = fact(3); other(); total + other() } c=fact
fn=helper f=fn helper() -> u32 { let seed = "seed"; let total = fact(3); other(); total + other() } c=other
fn=helper f=fn helper() -> u32 { let seed = "seed"; let total = fact(3); other(); total + other() } c=other
== contains, capture absent
n=3 args=(3)
== not-contains, capture absent
n=null args=(n - 1)
n=null args=()
n=null args=()
n=null args=()
n=null args=()
n=null args=()
== not-eq across levels, capture absent
n=null args=(n - 1)
n=3 args=(3)
n=null args=()
n=null args=()
n=null args=()
n=null args=()
n=null args=()"#
    );
}

/// The `--sqlite` store's table counts after one run of `query`.
fn stored_counts(query: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("scmpp.db");
    let printed = run(RUST, query, &["--sqlite", db.to_str().unwrap()]);
    let connection = rusqlite::Connection::open(&db).unwrap();
    let count = |sql: &str| connection.query_row(sql, [], |row| row.get::<_, i64>(0)).unwrap();
    [
        ("printed rows", printed.lines().filter(|line| line.starts_with('{')).count() as i64),
        ("scmpp_row", count("SELECT count(*) FROM scmpp_row")),
        ("named nodes", count("SELECT count(*) FROM scmpp_node WHERE named = 1")),
        ("anonymous nodes", count("SELECT count(*) FROM scmpp_node WHERE named = 0")),
        ("edges", count("SELECT count(*) FROM scmpp_node WHERE parent >= 0")),
        ("edges with a field", count("SELECT count(*) FROM scmpp_node WHERE field <> 0")),
        ("capture rows", count("SELECT count(*) FROM scmpp_capture")),
    ]
    .map(|(name, value)| format!("{name} {value}"))
    .join("\n")
}

#[test]
fn sqlite_keeps_scmpp_row_and_the_cst() {
    assert_eq!(
        stored_counts(RECURSION),
        "printed rows 0\nscmpp_row 3\nnamed nodes 69\nanonymous nodes 58\nedges 126\nedges with a field 54\ncapture rows 23"
    );
}

/// No relation reads `edge`, so a relation-free query writes no CST rows.
#[test]
fn relation_free_query_writes_no_cst_rows() {
    assert_eq!(
        stored_counts("((call_expression function: (identifier) @callee) (#eq? @callee \"other\"))"),
        "printed rows 0\nscmpp_row 4\nnamed nodes 0\nanonymous nodes 0\nedges 0\nedges with a field 0\ncapture rows 8"
    );
}

/// A frozen copy of `src/project.rs`, 2799 lines, sha256
/// d58336b93f2103e321b35750f1ae5940cf6dd5390c75770b1c867c9e1ce3a1f9. The oracle at aae25cde:
/// `ryi query --lang rust --query '(identifier) @x' src/project.rs | wc -l` -> 3575, and the
/// `#inside?` / `#not-inside?` lowering over `function_item` -> 3197 / 378.
#[test]
fn corpus_counts_partition_identifiers_by_enclosing_function() {
    let corpus = include_str!("fixtures/sprefa_extract_project.rs.frozen");
    let counts = [
        "((identifier) @x)",
        "((identifier) @x (#has-ancestor? @x function_item stopBy: end))",
        "((identifier) @x (#not-has-ancestor? @x function_item stopBy: end))",
    ]
    .map(|query| run(corpus, query, &[]).lines().count());
    assert_eq!(counts, [3575, 3197, 378]);
}

/// Two languages in one run: each compiles the query on its own grammar, and the one SQL
/// statement runs over both files' rows because the compiled SQL strings are equal.
#[test]
fn two_languages_with_equal_sql_share_one_run() {
    let dir = tempfile::tempdir().unwrap();
    let rust = dir.path().join("a.rs");
    let python = dir.path().join("b.py");
    let scm = dir.path().join("query.scm");
    std::fs::write(&rust, "fn fact(n: u32) -> u32 { fact(n) }\n").unwrap();
    std::fs::write(&python, "def fact(n):\n    return fact(n)\n").unwrap();
    std::fs::write(&scm, "((identifier) @x (#eq? @x \"fact\"))").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("query")
        .arg("--scmpp")
        .arg(&scm)
        .arg(&rust)
        .arg(&python)
        .env("DL_TRAIL", "0")
        .output()
        .expect("ryii runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let prefix = format!("{}/", dir.path().display());
    let mut lines = String::from_utf8(output.stdout).unwrap().replace(&prefix, "").lines().map(str::to_string).collect::<Vec<_>>();
    lines.sort();
    assert_eq!(
        lines.join("\n"),
        r#"{"path":"a.rs","x__start":25,"x__end":29,"x__text":"fact"}
{"path":"a.rs","x__start":3,"x__end":7,"x__text":"fact"}
{"path":"b.py","x__start":24,"x__end":28,"x__text":"fact"}
{"path":"b.py","x__start":4,"x__end":8,"x__text":"fact"}"#
    );
}

/// Past SQLite's join width (64 captures on one level) or expression depth (30 nested levels) the run stops
/// before reading any input: the only input is unreadable, and the limit is the error.
#[test]
fn sqlite_limits_fail_before_any_file_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("unreadable.rs");
    std::fs::write(&source, RUST).unwrap();
    std::fs::set_permissions(&source, std::os::unix::fs::PermissionsExt::from_mode(0o000)).unwrap();
    let wide = format!(
        "((identifier) {})",
        (0..64).map(|n| format!("@c{n}")).collect::<Vec<_>>().join(" ")
    );
    let deep = (1..30).fold("(block) @b30".to_string(), |inner, n| {
        format!("((block) @b{n} (#has-ancestor? @b{n} ({inner})))")
    });
    let actual = [wide, format!("((identifier) @x (#has-ancestor? @x {deep}))")]
        .map(|query| {
            let scm = dir.path().join("query.scm");
            std::fs::write(&scm, query).unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
                .args(["query", "--scmpp"])
                .arg(&scm)
                .arg(&source)
                .env("DL_TRAIL", "0")
                .output()
                .expect("ryii runs");
            format!("{} {}", output.status.code().unwrap_or(-1), String::from_utf8_lossy(&output.stderr).trim())
        })
        .join("\n");
    assert_eq!(
        actual,
        "2 query (rust): scm++ SQL for 64 captures, 1 levels deep, exceeds a SQLite limit: at most 64 tables in a join\n\
         2 query (rust): scm++ SQL for 31 captures, 31 levels deep, exceeds a SQLite limit: Expression tree is too large (maximum depth 1000)"
    );
}

/// 200 nested blocks under 5 nested `rows: each` levels run for minutes; `--timeout 1`
/// stops the SQL and exits 3, the way `graph --timeout` does.
#[test]
fn timeout_stops_the_sql_with_exit_3() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("deep.rs");
    let scm = dir.path().join("deep.scm");
    std::fs::write(&source, format!("fn d() {{ {} g(); {} }}\n", "{ ".repeat(200), "} ".repeat(200))).unwrap();
    let mut query = "(block)".to_string();
    for level in 0..5 {
        query = format!("((block) @b{level} (#has-ancestor? @b{level} {query} rows: each))");
    }
    std::fs::write(&scm, format!("((call_expression) @c (#has-ancestor? @c {query} rows: each))")).unwrap();
    let started = std::time::Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["query", "--timeout", "1", "--scmpp"])
        .arg(&scm)
        .arg(&source)
        .env("DL_TRAIL", "0")
        .output()
        .expect("ryii runs");
    let actual = (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
        output.stdout.len(),
        started.elapsed() < std::time::Duration::from_secs(30),
    );
    assert_eq!(actual, (Some(3), "query: query exceeded 1s".to_string(), 0, true));
}
