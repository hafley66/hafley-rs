use serde_json::{json, Value};
use std::process::Command;

/// Recursive graph questions return the edge rows that witness each answer.
/// The committed-fixture arms (call, type, flow, slice) freeze their rows;
/// the scratch/sqlite digest door keeps its equality and closure claims as
/// live asserts and freezes only the derived tables.
pub fn evaluate(_case: &Value) -> Value {
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(args)
            .output()
            .expect("graph binary runs");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    let rows = |args: &[&str]| -> Vec<Value> {
        String::from_utf8(run(args).stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    };

    // One edge id per hop: depth n carries n witnesses, all on the call plane.
    let call_rows = rows(&["graph", "--call-path", "chainA", "tests/fixtures/graph_ts"]);
    let call_paths: Vec<Value> = call_rows
        .iter()
        .map(|row| {
            json!([
                row["to_name"],
                row["depth"],
                row["witness"].as_array().unwrap().len(),
            ])
        })
        .collect();
    assert_eq!(
        call_paths,
        [json!(["chainB", 1, 1]), json!(["chainC", 2, 2]), json!(["chainD", 3, 3])]
    );
    assert!(call_rows.iter().all(|row| row["plane"] == "call"));

    // Type paths follow resolved type edges.
    let type_rows = rows(&["graph", "--type-path", "readWidget", "tests/fixtures/graph_ts"]);
    assert_eq!(type_rows.len(), 1);
    assert_eq!(type_rows[0]["plane"], "type");
    assert_eq!(type_rows[0]["to_name"], "Widget");
    assert_eq!(type_rows[0]["depth"], 1);
    assert_eq!(type_rows[0]["witness"].as_array().unwrap().len(), 1);

    // Flow paths follow derived interprocedural edges: the seed comes from
    // the resolve arm's own flow_edge row (from_blob@start:end).
    let files = [
        "tests/fixtures/resolve/0_caller.ts",
        "tests/fixtures/resolve/1_callee.ts",
    ];
    let reference = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--resolve", "--arms", "flow"])
        .args(files)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let edge: Value = String::from_utf8(reference.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["record"] == "flow_edge")
        .expect("fixture has a flow edge");
    let seed = format!(
        "{}@{}:{}",
        edge["from_blob"].as_str().unwrap(),
        edge["from"]["start"].as_u64().unwrap(),
        edge["from"]["end"].as_u64().unwrap()
    );
    let flow_rows = rows(
        [
            "graph",
            "--flow-path",
            seed.as_str(),
            files[0],
            files[1],
        ]
        .as_slice(),
    );
    assert!(!flow_rows.is_empty());
    assert!(flow_rows.iter().all(|row| row["plane"] == "flow"));
    assert!(flow_rows
        .iter()
        .all(|row| row["witness"].as_array().unwrap().len()
            == row["depth"].as_u64().unwrap() as usize));
    let flow_depths: Vec<u64> = flow_rows
        .iter()
        .map(|row| row["depth"].as_u64().unwrap())
        .collect();

    // A file-path seed and a digest seed over the same bytes agree byte for
    // byte; the depth-1 answers equal the sqlite df edges out of the param.
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("0_local.ts");
    let database = scratch.path().join("facts.db");
    let source = "export function local(children: string) { return children; }\n";
    std::fs::write(&path, source).unwrap();
    let start = source.find("children: string").unwrap();
    let end = start + "children: string".len();
    let seed = format!("{}@{start}:{end}", path.display());
    let path_output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["graph", "--flow-path", &seed, "--sqlite"])
        .arg(&database)
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        path_output.status.success(),
        "{}",
        String::from_utf8_lossy(&path_output.stderr)
    );
    let connection = rusqlite::Connection::open(&database).unwrap();
    let digest: String = connection
        .query_row("SELECT digest FROM file", [], |row| row.get(0))
        .unwrap();
    let parameter: (u32, u32, String) = connection
        .query_row(
            "SELECT span__start, span__end, name FROM node WHERE family = 'df' AND kind = 'param'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(parameter, (start as u32, end as u32, "children".to_string()));
    let mut statement = connection
        .prepare(
            "SELECT to__start, to__end, min(_row) FROM edge WHERE family = 'df' \
             AND _content_id = ?1 AND from__start = ?2 AND from__end = ?3 \
             GROUP BY to__start, to__end",
        )
        .unwrap();
    let mut direct: Vec<(String, u64)> = statement
        .query_map(
            rusqlite::params![digest, start as u32, end as u32],
            |row| {
                Ok((
                    format!("{}:{}", row.get::<_, u32>(0)?, row.get::<_, u32>(1)?),
                    row.get::<_, i64>(2)? as u64,
                ))
            },
        )
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(!direct.is_empty());
    direct.sort();
    let digest_output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["graph", "--flow-path", &format!("{digest}@{start}:{end}")])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        digest_output.status.success(),
        "{}",
        String::from_utf8_lossy(&digest_output.stderr)
    );
    assert_eq!(path_output.stdout, digest_output.stdout);
    let param_rows: Vec<Value> = String::from_utf8(path_output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(param_rows.iter().all(|row| row["record"] == "graph_path"
        && row["plane"] == "flow"
        && row["from_path"] == digest
        && row["from_name"] == format!("{start}:{end}")
        && row["to_path"] == digest
        && row["witness"].as_array().unwrap().len()
            == row["depth"].as_u64().unwrap() as usize));
    let mut actual: Vec<(String, u64)> = param_rows
        .iter()
        .filter(|row| row["depth"] == 1)
        .map(|row| {
            (
                row["to_name"].as_str().unwrap().to_string(),
                row["witness"][0].as_u64().unwrap(),
            )
        })
        .collect();
    actual.sort();
    assert_eq!(actual, direct);
    let digest_door = json!({
        "path_and_digest_streams_equal": true,
        "param_row": [start, end, "children"],
        "from_path_is_digest": true,
        "depth_one_answers": direct.iter().map(|(to, edge)| json!([to, edge])).collect::<Vec<_>>(),
    });

    // The control slice is a closed statement set: it names the branch and
    // the sink, never code after them.
    let slice_path = "tests/fixtures/graph_ts/5_slice.ts";
    let slice_source = include_str!("../fixtures/graph_ts/5_slice.ts");
    let byte = slice_source.find("allow()").unwrap() + 1;
    let slice_rows = rows(&["graph", "--slice", &format!("{slice_path}:{byte}")]);
    assert!(!slice_rows.is_empty());
    assert!(slice_rows
        .iter()
        .all(|row| row["record"] == "node" && row["family"] == "cfg"));
    let selected: Vec<Value> = slice_rows
        .iter()
        .filter_map(|row| {
            let start = row["span"]["start"].as_u64()? as usize;
            let end = row["span"]["end"].as_u64()? as usize;
            slice_source.get(start..end).map(|text| json!(text))
        })
        .collect();
    let statements: Vec<&str> = selected.iter().map(|text| text.as_str().unwrap()).collect();
    assert!(statements.iter().any(|text| text.contains("if (flag)")));
    assert!(statements.iter().any(|text| text.contains("allow()")));
    assert!(statements.iter().all(|text| !text.contains("after()")));

    json!({
        "call_paths": call_paths,
        "type_path": [type_rows[0]["plane"], type_rows[0]["to_name"], type_rows[0]["depth"]],
        "flow_depths": flow_depths,
        "flow_plane_all": true,
        "digest_door": digest_door,
        "slice_statements": selected,
    })
}
