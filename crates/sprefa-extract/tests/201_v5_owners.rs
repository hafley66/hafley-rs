use super::v5_support::rows;
#[cfg(feature = "cli")]
use super::v5_support::run;
use serde_json::Value;

fn fixtures() -> [(&'static str, &'static str); 3] {
    [
        ("1_widget.ts", include_str!("fixtures/v5_parity/1_widget.ts")),
        ("2_loop.rs", include_str!("fixtures/v5_parity/2_loop.rs")),
        ("3_closures.ts", "function outer(value: string) { const read = (inner: string) => inner; return read(value); }\n")
    ]
}

#[test]
fn whole_flow_rows_name_their_owning_function() {
    let _snapshots = super::v5_support::snapshots();
    let output: Vec<_> = fixtures()
        .into_iter()
        .map(|(path, source)| (path, rows(path, source, false)))
        .collect();
    insta::assert_json_snapshot!(
        "v5_parity__owners__whole_flow_rows_name_their_owning_function",
        output
    );
}

#[cfg(feature = "cli")]
#[test]
fn function_column_matches_jsonl_and_sqlite() {
    let _snapshots = super::v5_support::snapshots();
    let mut output = Vec::new();
    for (path, source) in fixtures() {
        let scratch = tempfile::tempdir().unwrap();
        let input = scratch.path().join(path);
        let database = scratch.path().join("facts.db");
        std::fs::write(&input, source).unwrap();
        run(
            &["--kinds", "df", "--sqlite", database.to_str().unwrap()],
            &input,
        );
        let connection = rusqlite::Connection::open(database).unwrap();
        let mut query = connection.prepare("SELECT span__start, span__end, kind, name, function FROM node WHERE family = 'df' ORDER BY span__start, span__end, kind, name, function").unwrap();
        let mut sql: Vec<Value> = query
            .query_map([], |row| {
                Ok(serde_json::json!([
                    row.get::<_, u32>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?
                ]))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let mut json: Vec<Value> = rows(&input.to_string_lossy(), source, false)
            .into_iter()
            .filter(|row| row["record"] == "node")
            .map(|row| {
                serde_json::json!([
                    row["span"]["start"],
                    row["span"]["end"],
                    row["kind"],
                    row["name"],
                    row["function"]
                ])
            })
            .collect();
        json.sort_by_key(|row| {
            (
                row[0].as_u64().unwrap(),
                row[1].as_u64().unwrap(),
                row[2].as_str().unwrap().to_string(),
                row[3].to_string(),
                row[4].to_string(),
            )
        });
        for row in sql.iter_mut().chain(json.iter_mut()) {
            if let Some(name) = row[3].as_str() {
                row[3] = name.replace(input.to_str().unwrap(), path).into();
            }
        }
        assert_eq!(json, sql);
        output.push((path, sql));
    }
    insta::assert_json_snapshot!(
        "v5_parity__owners__function_column_matches_jsonl_and_sqlite",
        output
    );
}
