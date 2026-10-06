use std::collections::BTreeSet;

pub fn oracle_starts(oracle: &str) -> BTreeSet<u32> {
    oracle
        .lines()
        .filter(|line| line.starts_with("df_node\t"))
        .map(|line| line.rsplit('\t').next().unwrap().parse().unwrap())
        .collect()
}

// Preserve byte-exact legacy JSON rows. The authorized DF traversal increment
// adds rows at new spans; report those separately without rewriting the oracle.
pub fn json_ported(actual: &str, expected: &str, path: &str) -> (String, String) {
    if !sprefa_extract::source_for(path).is_some_and(|source| source.name() == "ts") {
        return (actual.to_owned(), String::new());
    }
    let anchors: BTreeSet<u64> = expected
        .lines()
        .flat_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            row.as_object()
                .unwrap()
                .values()
                .filter_map(|value| value["start"].as_u64())
                .collect::<Vec<_>>()
        })
        .collect();
    let projected: Vec<_> = actual.lines().map(legacy_owner_columns).collect();
    let (ported, added): (Vec<_>, Vec<_>) =
        projected.iter().map(String::as_str).partition(|line| {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            row["family"] != "df"
                || row
                    .as_object()
                    .unwrap()
                    .values()
                    .filter_map(|value| value["start"].as_u64())
                    .all(|start| anchors.contains(&start))
        });
    (ported.join("\n"), added.join("\n"))
}

// The declared owner-metadata increment appends these two TS DF node columns.
// Remove only that suffix for historical byte comparisons; new fixtures assert
// every column through both JSONL and SQLite. All original bytes remain intact.
pub fn legacy_owner_columns(line: &str) -> String {
    let Some((original, _)) = line.rsplit_once(",\"is_async\":") else {
        return line.to_owned();
    };
    let row: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(row["record"], "node");
    assert_eq!(row["family"], "df");
    assert!(row["is_async"].is_boolean());
    assert!(matches!(
        row["owner_kind"].as_str(),
        Some("function" | "class_method" | "class_field" | "top_level")
    ));
    let suffix = format!(
        ",\"is_async\":{},\"owner_kind\":{}}}",
        row["is_async"], row["owner_kind"]
    );
    assert_eq!(line.trim_end(), format!("{original}{suffix}"));
    format!(
        "{original}}}{}",
        if line.ends_with('\n') { "\n" } else { "" }
    )
}
