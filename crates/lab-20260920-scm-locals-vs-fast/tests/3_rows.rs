use std::collections::BTreeSet;
use std::path::Path;

use lab_20260920_scm_locals_vs_fast::analyze;

#[test]
fn scip_scm_rows_use_sprefa_field_names() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../sprefa-extract/tests/fixtures/kotlin_receivers");
    let paths = [root.join("lib.kt"), root.join("use.kt")];
    let query = std::fs::read_to_string(manifest.join("queries/kotlin/locals.scm")).unwrap();
    let analysis = analyze("kotlin", &query, &paths).unwrap();
    let shapes = analysis
        .rows
        .iter()
        .map(|row| {
            let value = serde_json::to_value(row).unwrap();
            value
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        shapes,
        [
            "file,record,repo,symbol",
            "def_file,file,record,repo,symbol",
            "fn,name,record",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );
}
