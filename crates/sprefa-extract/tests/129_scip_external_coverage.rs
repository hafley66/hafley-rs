use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use sprefa_extract::scip_decode::load_index;
use sprefa_extract::{flatten_scip, v5_rel_rows, FlatFact};

const ROOT: &str = "tests/fixtures/scip_external";

#[test]
fn external_symbol_mentions_have_distinct_targets_and_a_coverage_receipt() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(ROOT);
    let index_path = root.join("index.scip");
    let index = load_index(&index_path).expect("committed SCIP index decodes");
    let raw = flatten_scip(&index, &|path| std::fs::read(root.join(path)).ok());
    let raw_occurrences = raw
        .iter()
        .filter(|row| matches!(row, FlatFact::ScipOccurrenceRow { .. }))
        .count();
    let raw_external_symbols = raw
        .iter()
        .filter(|row| matches!(row, FlatFact::ScipSymbolRow { path: None, .. }))
        .count();
    assert_eq!((raw_occurrences, raw_external_symbols), (6, 4));

    let family = v5_rel_rows(&index, &root, "scip_external");
    let mut emitted = BTreeMap::new();
    for row in &family {
        match row {
            FlatFact::ScipDefRow { symbol, .. } | FlatFact::ScipExternalRefRow { symbol, .. } => {
                *emitted.entry(symbol.as_str()).or_insert(0_usize) += 1;
            }
            _ => {}
        }
    }
    let mut drops = BTreeMap::new();
    let mut emitted_mentions = 0;
    for document in &index.documents {
        for occurrence in &document.occurrences {
            let symbol = index.symbol(occurrence.symbol);
            if emitted.get(symbol).is_some_and(|count| *count > 0) {
                emitted_mentions += 1;
            } else if symbol.is_empty() {
                *drops.entry("empty_symbol").or_insert(0_usize) += 1;
            } else {
                *drops.entry("unclassified").or_insert(0_usize) += 1;
            }
        }
    }
    println!(
        "scip_external fixture: raw_mentions={raw_occurrences} emitted_mentions={emitted_mentions} drops={drops:?} external_symbols={raw_external_symbols}"
    );
    assert_eq!((raw_occurrences, emitted_mentions), (6, 5));
    assert_eq!(drops, BTreeMap::from([("empty_symbol", 1)]));

    let mut external: Vec<_> = family
        .iter()
        .filter_map(|row| match row {
            FlatFact::ScipExternalRefRow { symbol, origin, .. } => {
                Some((symbol.as_str(), origin.as_str()))
            }
            _ => None,
        })
        .collect();
    external.sort();
    assert_eq!(external.len(), 4);
    for crate_name in ["core", "std", "alloc"] {
        assert!(
            external
                .iter()
                .any(|(_, origin)| origin.contains(&format!("cargo {crate_name} "))),
            "missing {crate_name}: {external:?}"
        );
    }
    let contains: Vec<_> = external
        .iter()
        .filter(|(symbol, _)| symbol.ends_with("contains()."))
        .collect();
    assert_eq!(contains.len(), 2);
    assert_ne!(contains[0].0, contains[1].0);
    assert!(contains
        .iter()
        .any(|(symbol, _)| symbol.contains("str#contains().")));
    assert!(contains
        .iter()
        .any(|(symbol, _)| symbol.contains("Vec#contains().")));

    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "--family",
            "scip",
            "--scip-index",
            index_path.to_str().unwrap(),
            ROOT,
        ])
        .output()
        .expect("ryi starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_external: Vec<FlatFact> = String::from_utf8(output.stdout)
        .expect("UTF-8 JSONL")
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid row"))
        .filter(|row| matches!(row, FlatFact::ScipExternalRefRow { .. }))
        .collect();
    let projected_external: Vec<FlatFact> = family
        .into_iter()
        .filter(|row| matches!(row, FlatFact::ScipExternalRefRow { .. }))
        .collect();
    assert_eq!(
        cli_external
            .iter()
            .map(|row| serde_json::to_value(row).unwrap())
            .collect::<Vec<_>>(),
        projected_external
            .iter()
            .map(|row| serde_json::to_value(row).unwrap())
            .collect::<Vec<_>>()
    );

    let raw_output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "--scip-facts",
            "--project-root",
            root.to_str().unwrap(),
            "--scip-index",
            index_path.to_str().unwrap(),
            root.join("0_external.rs").to_str().unwrap(),
        ])
        .output()
        .expect("ryi starts");
    assert!(
        raw_output.status.success(),
        "{}",
        String::from_utf8_lossy(&raw_output.stderr)
    );
    let raw_cli: Vec<FlatFact> = String::from_utf8(raw_output.stdout)
        .expect("UTF-8 JSONL")
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid raw row"))
        .collect();
    assert_eq!(
        raw_cli
            .iter()
            .filter(|row| matches!(row, FlatFact::ScipOccurrenceRow { .. }))
            .count(),
        raw_occurrences
    );
    assert_eq!(
        raw_cli
            .iter()
            .filter(|row| matches!(row, FlatFact::ScipSymbolRow { path: None, .. }))
            .count(),
        raw_external_symbols
    );
}
