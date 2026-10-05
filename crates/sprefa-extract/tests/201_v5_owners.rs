use serde_json::Value;
use sprefa_extract::{dispatch, flatten_jsonl, FamilyMask};

fn fixtures() -> [(&'static str, &'static str); 3] {
    [
        ("1_widget.ts", include_str!("fixtures/v5_parity/1_widget.ts")),
        ("2_loop.rs", include_str!("fixtures/v5_parity/2_loop.rs")),
        ("3_closures.ts", "function outer(value: string) { const read = (inner: string) => inner; return read(value); }\n")
    ]
}

fn rows(path: &str, source: &str) -> Vec<Value> {
    let out = dispatch(
        path,
        source.as_bytes(),
        FamilyMask {
            df: true,
            call: false,
            types: false,
            cst: false,
            data: false,
        },
    )
    .unwrap();
    flatten_jsonl(&out)
        .into_iter()
        .map(|row| serde_json::from_str(&row).unwrap())
        .collect()
}

#[test]
fn whole_flow_rows_name_their_owning_function() {
    let output: Vec<_> = fixtures()
        .into_iter()
        .map(|(path, source)| (path, rows(path, source)))
        .collect();
    insta::assert_json_snapshot!(output);
}

#[cfg(feature = "cli")]
#[test]
fn function_column_matches_jsonl_and_sqlite() {
    let mut output = Vec::new();
    for (path, source) in fixtures() {
        let scratch = tempfile::tempdir().unwrap();
        let input = scratch.path().join(path);
        let database = scratch.path().join("facts.db");
        std::fs::write(&input, source).unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
            .args(["--kinds", "df", "--sqlite"])
            .arg(&database)
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
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
        let mut json: Vec<Value> = rows(&input.to_string_lossy(), source)
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
    insta::assert_json_snapshot!(output);
}
