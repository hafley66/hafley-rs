#![cfg(feature = "cli")]

use std::path::Path;
use std::process::Command;

use rusqlite::Connection;
use serde_json::Value;

const COMPONENTS: &str = "tests/fixtures/rtkq_jsx/components.tsx";
const NESTED: &str = "tests/fixtures/rtkq_jsx/0_nested.tsx";
const HOOKS: &str = "tests/fixtures/rtkq_jsx/hooks.ts";
const GOLDEN: &str = include_str!("goldens/193_ts_syntax.jsonl");

fn run(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .env("RUST_LOG", "off")
        .env("DL_TRAIL", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn syntax_rows(jsonl: &str) -> Vec<Value> {
    let mut rows: Vec<Value> = jsonl
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| {
            matches!(
                row["record"].as_str(),
                Some("call_site" | "jsx_element" | "jsx_attribute")
            )
        })
        .map(|mut row| {
            row["path"] = Path::new(row["path"].as_str().unwrap())
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .into();
            row
        })
        .collect();
    rows.sort_by_key(Value::to_string);
    rows
}

fn database_rows(db: &Connection) -> Vec<Value> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT json_object('record',record,'callee',callee,'path',path,'line',line,'fn',\"fn\",'start',start,'end',end) FROM call_site",
        "SELECT json_object('record',record,'name',name,'path',path,'line',line,'fn',\"fn\",'start',start,'end',end,'parent_start',parent_start) FROM jsx_element",
        "SELECT json_object('record',record,'path',path,'element_start',element_start,'name',name,'value',value,'start',start,'end',end) FROM jsx_attribute",
    ] {
        rows.extend(db.prepare(sql).unwrap()
            .query_map([], |row| row.get::<_, String>(0)).unwrap()
            .map(Result::unwrap));
    }
    syntax_rows(&rows.join("\n"))
}

#[test]
fn written_calls_and_nested_jsx_match_jsonl_and_sqlite_goldens() {
    let expected = syntax_rows(GOLDEN);
    let scratch = tempfile::tempdir().unwrap();
    for mode in ["fast", "--resolve"] {
        assert_eq!(
            syntax_rows(&run(&[mode, COMPONENTS, NESTED, HOOKS])),
            expected,
            "{mode} JSONL"
        );
        let path = scratch.path().join(format!("{mode}.db"));
        run(&[
            mode,
            COMPONENTS,
            NESTED,
            HOOKS,
            "--sqlite",
            path.to_str().unwrap(),
        ]);
        assert_eq!(
            database_rows(&Connection::open(path).unwrap()),
            expected,
            "{mode} SQLite"
        );
    }
    // Direct scm projection agrees with the fused fast parser's retained captures.
    let direct =
        sprefa_extract::scm_facts(&[COMPONENTS.into(), NESTED.into(), HOOKS.into()]).unwrap();
    let direct = direct
        .iter()
        .map(|row| serde_json::to_string(row).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(syntax_rows(&direct), expected);
}

#[test]
fn slow_retains_written_syntax_with_an_empty_index() {
    let scratch = tempfile::tempdir().unwrap();
    // Empty protobuf is a valid SCIP index with no documents. Syntax rows
    // must survive independently of compiler/index answers.
    let index = scratch.path().join("empty.scip");
    std::fs::write(&index, b"").unwrap();
    let index = index.to_str().unwrap();
    let args = [
        "slow", "--no-checker", "--scip-index", index,
        COMPONENTS, NESTED, HOOKS,
    ];
    let expected = syntax_rows(GOLDEN);
    assert_eq!(syntax_rows(&run(&args)), expected, "slow JSONL");
    let path = scratch.path().join("slow.db");
    let mut sqlite_args = args.to_vec();
    sqlite_args.extend(["--sqlite", path.to_str().unwrap()]);
    run(&sqlite_args);
    assert_eq!(
        database_rows(&Connection::open(path).unwrap()),
        expected,
        "slow SQLite"
    );
}

#[test]
fn written_tsx_calls_are_additive_to_the_original_resolved_site() {
    let scratch = tempfile::tempdir().unwrap();
    let caller = scratch.path().join("0_caller.tsx");
    let callee = scratch.path().join("1_callee.ts");
    std::fs::write(&caller, include_str!("fixtures/resolve/0_caller.ts")).unwrap();
    std::fs::write(&callee, include_str!("fixtures/resolve/1_callee.ts")).unwrap();
    let caller = caller.to_str().unwrap();
    let callee = callee.to_str().unwrap();
    let rows: Vec<Value> = run(&["--resolve", caller, callee])
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let sites: Vec<_> = rows
        .iter()
        .map(|row| {
            if row["record"] == "resolved_edge" {
                serde_json::json!([
                    row["record"], row["caller_name"], row["callee_name"],
                    row["caller_site_start"], row["caller_site_end"]
                ])
            } else {
                serde_json::json!([
                    row["record"], row["fn"], row["callee"], row["start"], row["end"]
                ])
            }
        })
        .collect();
    assert_eq!(
        Value::from(sites),
        serde_json::json!([
            ["resolved_edge", "run", "helper", 33, 39],
            ["call_site", "run", "helper", 33, 41]
        ])
    );
}
