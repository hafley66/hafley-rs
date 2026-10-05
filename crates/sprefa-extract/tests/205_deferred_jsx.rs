#![cfg(feature = "cli")]

use super::v5_support::{project, rows as syntax_rows, run};
use serde_json::{json, Value};

fn rows(name: &str, arguments: &[&str]) -> Vec<Value> {
    let path = format!(
        "{}/tests/fixtures/v5_parity/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let result = run(arguments, &path);
    String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect()
}

#[test]
fn deferred_jsx_props_captures_and_callers() {
    let _snapshots = super::v5_support::snapshots();
    let mut output = Vec::new();
    for name in [
        "16_direct_vs_jsx.tsx",
        "17_other_object.tsx",
        "18_second_scope.tsx",
        "19_eager_hook.tsx",
        "20_capture.ts",
        "21_capture.rs",
    ] {
        let path = format!(
            "{}/tests/fixtures/v5_parity/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let source = std::fs::read(&path).unwrap();
        let syntax = syntax_rows(name, std::str::from_utf8(&source).unwrap(), true);
        let resolved = rows(name, &["--resolve", "--arms", "flow"]);
        let callers = if name.ends_with(".tsx") {
            rows(name, &["graph", "--callers", "Card"])
        } else {
            Vec::new()
        };
        let hooks = if name == "19_eager_hook.tsx" {
            rows(name, &["graph", "--callers", "useHook"])
        } else {
            Vec::new()
        };
        let normalize = |rows: Vec<Value>| {
            rows.into_iter()
                .map(|mut row| {
                    for field in ["path", "from_path", "to_path", "caller_path", "callee_path"] {
                        if let Some(value) = row.get_mut(field).filter(|value| value.is_string()) {
                            *value = json!(std::path::Path::new(value.as_str().unwrap())
                                .file_name()
                                .unwrap()
                                .to_str()
                                .unwrap());
                        }
                    }
                    row
                })
                .collect::<Vec<_>>()
        };
        output.push(json!({"fixture":name, "syntax":syntax, "resolved":resolved, "callers":normalize(callers), "hooks":normalize(hooks)}));
    }
    let mut proofs = Vec::new();
    for fixture in &output {
        let syntax = fixture["syntax"].as_array().unwrap();
        let flow = fixture["resolved"].as_array().unwrap();
        let param_targets: Vec<Value> = flow
            .iter()
            .filter(|edge| {
                edge["kind"] == "arg_to_param"
                    && syntax.iter().any(|node| {
                        node["record"] == "node"
                            && node["family"] == "df"
                            && node["kind"] == "param"
                            && node["name"] == "title"
                            && node["span"] == edge["to"]
                    })
            })
            .map(|edge| json!({"from":edge["from"],"to":edge["to"]}))
            .collect();
        let member_targets = flow
            .iter()
            .filter(|edge| {
                edge["kind"] == "arg_to_param"
                    && syntax.iter().any(|node| {
                        node["record"] == "node"
                            && node["family"] == "df"
                            && node["kind"] == "member"
                            && node["span"] == edge["to"]
                    })
            })
            .count();
        proofs.push(json!({"fixture":fixture["fixture"], "title_targets":param_targets, "member_targets":member_targets, "lambda_elem":flow.iter().any(|edge|edge["kind"] == "lambda_elem"), "lambda_ret":flow.iter().any(|edge|edge["kind"] == "lambda_ret"), "callers":fixture["callers"], "hooks":fixture["hooks"]}));
    }
    insta::assert_json_snapshot!("v5_parity__deferred_jsx__deferred_jsx_proofs", proofs);
    let projected: Vec<_> = output
        .iter()
        .map(|fixture| {
            let mut rows = fixture["syntax"].as_array().unwrap().clone();
            rows.extend(fixture["resolved"].as_array().unwrap().iter().cloned());
            format!(
                "{}\n{}",
                fixture["fixture"].as_str().unwrap(),
                project(&rows)
            )
        })
        .collect();
    insta::assert_snapshot!(
        "v5_parity__deferred_jsx__deferred_jsx_whole_output",
        projected.join("\n\n")
    );
}
