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
    let (ported, added): (Vec<_>, Vec<_>) = actual.lines().partition(|line| {
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
