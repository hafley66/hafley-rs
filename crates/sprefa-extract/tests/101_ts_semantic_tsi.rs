//! Stock tsgo semantic rows and demand-scoped coverage.
#![cfg(feature = "ts-checker")]
use serde_json::Value;
use std::collections::BTreeSet;
use std::process::Command;

fn stream_with_witness(file: &str, witness: bool) -> Vec<Value> {
    let mut args = vec![
        "--resolve",
        "--arms",
        "type",
        "--root",
        "tests/fixtures/tsi",
        "--ts-checker",
        file,
    ];
    if witness {
        args.insert(0, "--witness");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Check the wire's declaring positions independently of snapshot equality.
fn every_id_is_declared(rows: &[Value]) {
    let mut declared = BTreeSet::new();
    let mut referenced = BTreeSet::new();
    for row in rows.iter().filter(|row| row["record"] == "fact") {
        let args = row["args"].as_array().unwrap();
        let declaration = match row["relation"].as_str().unwrap() {
            "tsi.type" | "tsi.symbol" | "tsi.value" | "tsi.edge" | "rust.impl" => Some(0),
            "tsi.called" => Some(2),
            _ => None,
        };
        if let Some(position) = declaration {
            declared.insert(
                args[position]["id"]
                    .as_u64()
                    .expect("declaring argument is an id"),
            );
        }
        referenced.extend(args.iter().filter_map(|arg| arg["id"].as_u64()));
    }
    let undeclared: Vec<_> = referenced.difference(&declared).collect();
    assert_eq!(
        undeclared,
        Vec::<&u64>::new(),
        "every referenced id is declared in this stream"
    );
}

#[test]
fn stock_streams_preserve_coverage_and_declared_ids() {
    let mut outputs = Vec::new();
    for (file, witness) in [
        ("probe.ts", false),
        ("probe.ts", true),
        ("recursive.ts", true),
    ] {
        let rows = stream_with_witness(&format!("tests/fixtures/tsi/{file}"), witness);
        if !witness {
            assert!(!rows
                .iter()
                .any(|row| row["record"] == "run" && row["mode"] == "semantic"));
            assert!(!rows.iter().any(|row| matches!(
                row["record"].as_str(),
                Some("fact" | "witness" | "coverage" | "diagnostic")
            )));
        }
        if file == "probe.ts" && witness {
            assert!(rows
                .iter()
                .any(|row| row["record"] == "run" && row["tool"] == "tsgo"));
            for relation in ["tsi.subtype", "tsi.equivalent"] {
                assert!(!rows
                    .iter()
                    .any(|row| row["record"] == "fact" && row["relation"] == relation));
                assert!(rows.iter().any(|row| row["record"] == "coverage"
                    && row["run"] == 1
                    && row["relation"] == relation
                    && row["coverage"] == "partial"));
                assert!(rows.iter().any(|row| row["record"] == "diagnostic"
                    && row["run"] == 1
                    && row["relation"] == relation));
            }
            let semantic_ordinals: BTreeSet<_> = rows
                .iter()
                .filter(|row| row["record"] == "witness" && row["method"] == "checker_walk")
                .map(|row| row["fact"].as_u64().unwrap())
                .collect();
            let semantic_relations: BTreeSet<_> = rows
                .iter()
                .filter(|row| {
                    row["record"] == "fact"
                        && semantic_ordinals.contains(&row["fact"].as_u64().unwrap())
                })
                .map(|row| row["relation"].as_str().unwrap())
                .collect();
            for relation in [
                "tsi.product",
                "tsi.edge",
                "tsi.called",
                "tsi.argument",
                "tsi.callable",
                "tsi.input",
                "tsi.output",
                "ts.optional",
            ] {
                assert!(
                    semantic_relations.contains(relation),
                    "stock snapshot retains {relation}"
                );
            }
        }
        every_id_is_declared(&rows);
        outputs.push(serde_json::json!({"file": file, "witness": witness, "rows": rows}));
    }
    insta::assert_json_snapshot!(
        "stock_streams",
        crate::stock_tsgo::normalize(serde_json::json!(outputs))
    );
}

#[test]
fn stock_pairs_identity_and_bases_share_one_unopened_lsp_process() {
    use sprefa_extract::lang::ts7_lsp_session::TS_SESSIONS;
    use sprefa_extract::lang::ts_checker::{answer, TsDemand, TsSite};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tsi")
        .canonicalize()
        .unwrap();
    let file = root.join("0_stock.ts");
    let text = std::fs::read_to_string(&file).unwrap();
    let starts = [
        text.find("Derived extends").unwrap(),
        text.find("Base {").unwrap(),
        text.find("derived: Derived").unwrap() + "derived: ".len(),
    ];
    let make_demand = || TsDemand {
        sites: starts
            .iter()
            .zip(["Derived", "Base", "Derived"])
            .map(|(start, name)| TsSite {
                start: *start as u32,
                end: (*start + name.len()) as u32,
                name: name.into(),
                call: false,
                type_ref: true,
            })
            .collect(),
        pairs: vec![(0, 1), (1, 0)],
    };
    let run = || {
        answer(
            &root,
            &[("0_stock.ts".into(), file.clone(), make_demand())],
            true,
        )
        .unwrap()
    };
    let first = run();
    let pid = {
        let sessions = TS_SESSIONS.get().unwrap().lock().unwrap();
        let session = &sessions[&root];
        assert!(session.opened.is_empty(), "the checker sends no didOpen");
        session.lsp.child.id()
    };
    let second = run();
    {
        let sessions = TS_SESSIONS.get().unwrap().lock().unwrap();
        let session = &sessions[&root];
        assert_eq!(
            session.lsp.child.id(),
            pid,
            "one process per root across requests"
        );
        assert!(
            session.opened.is_empty(),
            "the hosted API leaves LSP documents unopened"
        );
    }
    assert_eq!(
        first.tsi, second.tsi,
        "snapshot handles cannot change run-local rows"
    );
    assert_eq!(first.coverage, second.coverage);
    assert_eq!(first.version, "7.0.2");
    assert_eq!(first.files_answered, 1);
    let relation = |name: &str| {
        first
            .tsi
            .iter()
            .filter(|fact| fact.relation == name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        relation("tsi.has_type").len(),
        3,
        "only requested sites acquire types"
    );
    assert_eq!(
        relation("tsi.symbol").len(),
        2,
        "two occurrences of Derived share symbol identity"
    );
    assert_eq!(
        relation("tsi.assignable").len(),
        1,
        "Derived -> Base passes; Base -> Derived fails"
    );
    assert_eq!(
        relation("tsi.conforms").len(),
        1,
        "Derived's stock base is Base"
    );
    assert!(!relation("tsi.name").iter().any(|fact| fact
        .args
        .contains(&sprefa_extract::tsi::Arg::Text("Unrequested".into()))));
    let rows: Vec<_> = first.tsi.iter().map(|fact| serde_json::json!({"record": "fact", "relation": fact.relation, "args": fact.args})).collect();
    every_id_is_declared(&rows);
    insta::assert_json_snapshot!(
        "stock_demanded_rows_and_coverage",
        (first.tsi, first.coverage)
    );
}
