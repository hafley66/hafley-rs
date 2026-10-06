use super::v5_support::{rows, run, snapshots};
use serde_json::{json, Value};

#[test]
fn ts_df_owner_flags_whole_output() {
    let _snapshots = snapshots();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixtures: std::collections::BTreeSet<_> =
        std::fs::read_dir(root.join("tests/fixtures/df_owner_flags"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
    let mut calls = Vec::new();
    let mut whole = Vec::new();
    for path in &fixtures {
        let name = path.file_name().unwrap().to_str().unwrap();
        let source = std::fs::read_to_string(path).unwrap();
        let mut nodes: Vec<_> = rows(name, &source, false)
            .into_iter()
            .filter(|row| row["record"] == "node" && row["family"] == "df")
            .collect();
        nodes.sort_by_key(|row| {
            (
                row["span"]["start"].as_u64().unwrap(),
                row["span"]["end"].as_u64().unwrap(),
                row["kind"].as_str().unwrap().to_owned(),
            )
        });
        for node in &nodes {
            if node["kind"] == "call_res" {
                let start = node["span"]["start"].as_u64().unwrap() as usize;
                let end = node["span"]["end"].as_u64().unwrap() as usize;
                if source[start..end].starts_with("read") {
                    calls.push(json!([
                        name,
                        &source[start..end],
                        node["is_async"],
                        node["owner_kind"]
                    ]));
                }
            }
        }
        whole.push(json!({"fixture": name, "nodes": nodes}));
    }
    let expected = json!([
        ["0_functions.ts", "readAsync()", true, "function"],
        ["0_functions.ts", "readSync()", false, "function"],
        ["0_functions.ts", "readExport()", true, "function"],
        ["0_functions.ts", "readArrow()", true, "function"],
        ["0_functions.ts", "readDefault()", true, "function"],
        ["1_class.ts", "readMethod()", true, "class_method"],
        ["1_class.ts", "readCallback()", false, "function"],
        ["1_class.ts", "readField()", true, "function"],
        ["1_class.ts", "readObject()", true, "function"],
        ["2_closures.tsx", "readClosure()", true, "function"],
        ["2_closures.tsx", "readNested()", false, "function"],
        ["2_closures.tsx", "readAttribute()", true, "function"]
    ]);
    calls.sort_by_key(|row| row.to_string());
    let mut expected_calls = expected.as_array().unwrap().clone();
    expected_calls.sort_by_key(|row| row.to_string());
    assert_eq!(calls, expected_calls);
    for (path, expected) in fixtures.iter().zip(&whole) {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("facts.db");
        run(
            &["--kinds", "df", "--sqlite", database.to_str().unwrap()],
            path,
        );
        let db = rusqlite::Connection::open(database).unwrap();
        let mut statement = db
            .prepare(
                "SELECT json_object('record','node','family',family,
            'span',json_object('start',span__start,'end',span__end),'kind',kind,'name',name,
            'function',function,'is_async',json(CASE is_async WHEN 1 THEN 'true' ELSE 'false' END),
            'owner_kind',owner_kind) FROM node WHERE family='df' ORDER BY span__start,span__end,kind",
            )
            .unwrap();
        let mut actual: Vec<Value> = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(|row| serde_json::from_str(&row.unwrap()).unwrap())
            .collect();
        for node in &mut actual {
            if let Some(text) = node["name"].as_str() {
                node["name"] = json!(text.replace(
                    path.to_str().unwrap(),
                    path.file_name().unwrap().to_str().unwrap()
                ));
            }
        }
        assert_eq!(json!(actual), expected["nodes"]);
    }
    insta::assert_json_snapshot!("ts_df_owner_flags_whole_output", whole);
}
