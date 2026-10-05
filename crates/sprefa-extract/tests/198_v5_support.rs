use serde_json::Value;
use sprefa_extract::{dispatch, flatten_jsonl, FamilyMask, RyiOutput};

pub(super) fn facts(path: &str, source: &[u8], call: bool) -> std::sync::Arc<RyiOutput> {
    dispatch(
        path,
        source,
        FamilyMask {
            df: true,
            call,
            types: false,
            cst: false,
            data: false,
        },
    )
    .unwrap()
}

pub(super) fn rows(path: &str, source: &str, call: bool) -> Vec<Value> {
    flatten_jsonl(&facts(path, source.as_bytes(), call))
        .into_iter()
        .map(|row| serde_json::from_str(&row).unwrap())
        .collect()
}

pub(super) fn snapshots() -> impl Drop {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"));
    settings.set_prepend_module_to_snapshot(false);
    settings.bind_to_scope()
}

#[cfg(feature = "cli")]
pub(super) fn run(arguments: &[&str], path: impl AsRef<std::ffi::OsStr>) -> std::process::Output {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(arguments)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

pub(super) fn node_label(node: &Value) -> String {
    format!(
        "{} {}:{} {} @{}",
        node["kind"].as_str().unwrap(),
        node["span"]["start"],
        node["span"]["end"],
        node["name"].as_str().unwrap_or("-"),
        node["function"].as_str().unwrap_or("-")
    )
}

fn endpoint(rows: &[Value], span: &Value) -> String {
    let nodes: Vec<_> = rows
        .iter()
        .filter(|row| row["record"] == "node" && row["family"] == "df" && row["span"] == *span)
        .map(node_label)
        .collect();
    if nodes.is_empty() {
        format!("{}:{}", span["start"], span["end"])
    } else {
        nodes.join(" | ")
    }
}

pub(super) fn project(rows: &[Value]) -> String {
    let mut output = Vec::new();
    for row in rows {
        let line = match row["record"].as_str().unwrap() {
            "node" if row["family"] == "df" => format!("node {}", node_label(row)),
            "edge" if row["family"] == "df" => format!(
                "{} {} {}:{} -> {} {}:{}",
                row["kind"].as_str().unwrap(),
                row["from_kind"].as_str().unwrap(),
                row["from"]["start"],
                row["from"]["end"],
                row["to_kind"].as_str().unwrap(),
                row["to"]["start"],
                row["to"]["end"]
            ),
            "flow_edge" => {
                let (from, to) = if row["kind"] == "ret_to_call_res" {
                    (&row["to"], &row["from"])
                } else {
                    (&row["from"], &row["to"])
                };
                format!(
                    "{} {} -> {}",
                    row["kind"].as_str().unwrap(),
                    endpoint(rows, from),
                    endpoint(rows, to)
                )
            }
            "df_field" => format!(
                "field {}:{} {} <- {}",
                row["owner"]["start"],
                row["owner"]["end"],
                row["name"].as_str().unwrap(),
                endpoint(rows, &row["value"])
            ),
            "arg" if row["family"] == "df" => format!(
                "arg {}:{} pos {} <- {}",
                row["call"]["start"],
                row["call"]["end"],
                row["pos"],
                endpoint(rows, &row["arg"])
            ),
            "param" if row["family"] == "df" => {
                format!("param {} pos {}", endpoint(rows, &row["span"]), row["pos"])
            }
            "df_lit" => format!("lit {} {}", endpoint(rows, &row["node"]), row["text"]),
            _ => continue,
        };
        output.push(line);
    }
    output.join("\n")
}
