use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

pub fn evaluate(directory: &str, case: impl Fn(&Value) -> Value) -> BTreeMap<String, Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(directory);
    let mut files: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let input = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            (
                path.file_stem().unwrap().to_str().unwrap().to_string(),
                case(&input),
            )
        })
        .collect()
}

pub fn snapshot(directory: &str, output: BTreeMap<String, Value>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(directory);
    insta::with_settings!({snapshot_path=>root,prepend_module_to_snapshot=>false,omit_expression=>true}, {
        insta::assert_json_snapshot!("output",output);
    });
}
