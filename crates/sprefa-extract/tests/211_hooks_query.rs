use super::v5_support::{run, snapshots};
use serde_json::{json, Value};

fn query_fixtures(group: &str) -> Vec<Value> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let program = root.join("bench/rules-of-hooks/2_hooks.scm");
    let sql = std::fs::read_to_string(root.join("bench/rules-of-hooks/3_violations.sql")).unwrap();
    let mut paths: Vec<_> = std::fs::read_dir(root.join("tests/fixtures/hooks_query").join(group))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let temp = tempfile::tempdir().unwrap();
            let facts = temp.path().join("facts.db");
            let query = temp.path().join("query.db");
            run(
                &["--kinds", "call,df", "--sqlite", facts.to_str().unwrap()],
                &path,
            );
            run(
                &[
                    "query",
                    "--scmpp",
                    program.to_str().unwrap(),
                    "--sqlite",
                    query.to_str().unwrap(),
                ],
                &path,
            );
            let db = rusqlite::Connection::open(query).unwrap();
            db.execute("ATTACH ?1 AS facts", [facts.to_str().unwrap()])
                .unwrap();
            let mut statement = db.prepare(&sql).unwrap();
            let columns: Vec<_> = statement
                .column_names()
                .into_iter()
                .map(str::to_owned)
                .collect();
            let results: Vec<Value> = statement
                .query_map([], |row| {
                    let mut result = serde_json::Map::new();
                    for (index, column) in columns.iter().enumerate() {
                        let value = match row.get_ref(index)? {
                            rusqlite::types::ValueRef::Null => Value::Null,
                            rusqlite::types::ValueRef::Integer(n) => json!(n),
                            rusqlite::types::ValueRef::Text(text) => {
                                let text = std::str::from_utf8(text).unwrap();
                                json!(if column == "path" {
                                    path.file_name().unwrap().to_str().unwrap()
                                } else {
                                    text
                                })
                            }
                            value => panic!("unexpected SQL value: {value:?}"),
                        };
                        result.insert(column.clone(), value);
                    }
                    Ok(Value::Object(result))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            json!({"fixture": path.file_name().unwrap().to_str().unwrap(), "rows": results})
        })
        .collect()
}

#[test]
fn hooks_callback_whole_output() {
    let _snapshots = snapshots();
    insta::assert_json_snapshot!("hooks_callback_whole_output", query_fixtures("callbacks"));
}
