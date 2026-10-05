#![cfg(feature = "cli")]

use serde_json::Value;
use sprefa_extract::{dispatch, flatten_jsonl, FamilyMask};

pub(super) fn fixtures() -> [(&'static str, &'static str); 3] {
    [
        ("3_jsx_props.tsx", include_str!("fixtures/v5_parity/3_jsx_props.tsx")),
        ("4_jsx_exprs.tsx", include_str!("fixtures/v5_parity/4_jsx_exprs.tsx")),
        ("5_jsx_member.tsx", include_str!("fixtures/v5_parity/5_jsx_member.tsx")),
    ]
}

#[test]
fn whole_jsx_rows_and_resolved_prop_edges_match_v5_cases() {
    let mut output = Vec::new();
    for (name, source) in fixtures() {
        let path = format!("{}/tests/fixtures/v5_parity/{name}", env!("CARGO_MANIFEST_DIR"));
        let syntax = dispatch(name, source.as_bytes(), FamilyMask {df: true, cst: false, types: false, call: false, data: false}).unwrap();
        let syntax: Vec<Value> = flatten_jsonl(&syntax).into_iter().map(|row| serde_json::from_str(&row).unwrap()).collect();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
            .args(["--resolve", "--arms", "flow", &path]).output().unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        let resolved: Vec<Value> = String::from_utf8(result.stdout).unwrap().lines()
            .map(|row| serde_json::from_str::<Value>(row).unwrap()).filter(|row| row["record"] == "flow_edge").collect();
        output.push(serde_json::json!({"fixture":name,"syntax":syntax,"resolved":resolved}));
    }
    insta::assert_json_snapshot!(output);
}
