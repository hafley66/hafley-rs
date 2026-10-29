//! `query --lang typescript` names the TypeScript family: each file parses with the
//! grammar its extension picks, so a `.tsx` file is read as tsx.
use super::v5_support::{run, snapshots};

#[test]
fn query_lang_typescript_picks_tsx_by_extension() {
    let _snapshots = snapshots();
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/query_lang_family");
    let query = "(call_expression function: (identifier) @fn) (ERROR) @error";
    let table: Vec<_> = [&[][..], &["--lang", "typescript"][..], &["--lang", "tsx"][..]]
        .into_iter()
        .map(|lang| {
            let mut arguments = vec!["query", "--query", query];
            arguments.extend_from_slice(lang);
            let output = run(&arguments, fixtures);
            let rows: Vec<serde_json::Value> = String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .map(|line| {
                    let mut row: serde_json::Value = serde_json::from_str(line).unwrap();
                    let path = row["path"].as_str().unwrap().rsplit('/').next().unwrap().to_string();
                    row["path"] = path.into();
                    row
                })
                .collect();
            serde_json::json!({"lang": lang.join(" "), "rows": rows})
        })
        .collect();
    insta::assert_json_snapshot!("query_lang_typescript_picks_tsx_by_extension", table);
}
