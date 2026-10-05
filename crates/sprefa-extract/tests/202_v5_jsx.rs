#![cfg(feature = "cli")]

use super::v5_support::{rows, run};
use serde_json::Value;

pub(super) fn fixtures() -> [(&'static str, &'static str); 3] {
    [
        (
            "3_jsx_props.tsx",
            include_str!("fixtures/v5_parity/3_jsx_props.tsx"),
        ),
        (
            "4_jsx_exprs.tsx",
            include_str!("fixtures/v5_parity/4_jsx_exprs.tsx"),
        ),
        (
            "5_jsx_member.tsx",
            include_str!("fixtures/v5_parity/5_jsx_member.tsx"),
        ),
    ]
}

#[test]
fn whole_jsx_rows_and_resolved_prop_edges_match_v5_cases() {
    let _snapshots = super::v5_support::snapshots();
    let mut output = Vec::new();
    for (name, source) in fixtures() {
        let path = format!(
            "{}/tests/fixtures/v5_parity/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let syntax = rows(name, source, false);
        let result = run(&["--resolve", "--arms", "flow"], &path);
        let resolved: Vec<Value> = String::from_utf8(result.stdout)
            .unwrap()
            .lines()
            .map(|row| serde_json::from_str::<Value>(row).unwrap())
            .filter(|row| row["record"] == "flow_edge")
            .collect();
        output.push(serde_json::json!({"fixture":name,"syntax":syntax,"resolved":resolved}));
    }
    insta::assert_json_snapshot!(
        "v5_parity__jsx__whole_jsx_rows_and_resolved_prop_edges_match_v5_cases",
        output
    );
}
