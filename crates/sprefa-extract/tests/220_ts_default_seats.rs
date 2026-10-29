//! Local exports seat on the declaration their name binds: plain default,
//! `export default memo(X)`, a re-exported import, and a named export list.
use super::v5_support::snapshots;

#[test]
fn ts_default_exports_resolve_to_their_declaration() {
    let _snapshots = snapshots();
    let dir = "tests/fixtures/ts_default_seats";
    let mut files: Vec<String> = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ts_default_seats"))
        .unwrap()
        .map(|entry| format!("{dir}/{}", entry.unwrap().file_name().to_string_lossy()))
        .collect();
    files.sort();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--resolve", "--arms", "call"])
        .args(&files)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let base = |row: &serde_json::Value, key: &str| {
        row[key].as_str().unwrap_or("").rsplit('/').next().unwrap().to_string()
    };
    let mut rows: Vec<String> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter_map(|row| match row["record"].as_str()? {
            "resolved_import" if row["kind"] != "module" => Some(format!(
                "import {} {} as {} -> {}:{} {} hops={}",
                base(&row, "src_path"),
                row["name"].as_str()?,
                row["local"].as_str()?,
                base(&row, "target_path"),
                row["target_name"].as_str()?,
                row["kind"].as_str()?,
                row["hops"],
            )),
            "resolved_edge" => Some(format!(
                "call {}:{}@{} -> {}:{}@{}",
                base(&row, "caller_path"),
                row["caller_name"].as_str()?,
                row["caller_site_start"],
                base(&row, "callee_path"),
                row["callee_name"].as_str()?,
                row["callee_start"],
            )),
            _ => None,
        })
        .collect();
    rows.sort();
    insta::assert_snapshot!("ts_default_exports_resolve_to_their_declaration", rows.join("\n"));
}
