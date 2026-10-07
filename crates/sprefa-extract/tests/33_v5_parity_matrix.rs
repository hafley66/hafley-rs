//! THE PARITY MATRIX, AS A TABLE. The three legs — schema reach, per-file
//! emission, oracle coverage against the live roster — are one api step over
//! `tests/fixtures/v5_parity_matrix_cases/0_matrix.json`: the matrix, the
//! emission fixture set and the graded/waived roster rows live in the case
//! data, the closure computes every leg, and the whole projection freezes in
//! the snapshot. A renamed record, a deleted one, or a language quietly
//! gaining a plane still turns the claims red.
//!
//! v5's side is the CHECKED-IN oracle under `tests/fixtures/**/*.v5.jsonl`.
//! THE v5 BINARY IS NEVER RUN HERE, and that is deliberate: "I DO NOT WANT TO
//! RUN V5 ANYTHING ANYMORE" (CLAUDE.md). `golden_parity` asserts the FACT
//! equality against those captures; this file asserts the mapping's shape.
//!
//! SABOTAGE RECEIPTS (all three run 2026-08-21, all three red, then reverted):
//!  - renaming `df_lit` to `df_literal` in one MATRIX row -> legs 1 and 2 red,
//!    "matrix row v5 `df_lit` names record tag `df_literal`, absent from SCHEMA".
//!  - deleting the `("rust", "rust")` row from the graded list -> leg 3 red,
//!    "roster Source [\"rust\"] is in neither the graded nor the waived rows.
//!    Capture a v5 oracle for it, or add a row with the reason it has no v5
//!    twin."
//!  - swapping the rust and kotlin doc fixtures for one empty file -> leg 2
//!    red, "per-file plane never emitted these mapped records: [\"doc\",
//!    \"doc_tag\"]".
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("v5_parity_matrix_cases", |case| {
        crate::fixture_runner::commands(case, |step| {
            assert_eq!(step["api"], "legs", "unknown api step: {step}");
            legs(step)
        })
    });
}

fn legs(step: &serde_json::Value) -> serde_json::Value {
    use sprefa_extract::{dispatch, flatten, sources, FamilyMask, SCHEMA};
    let mut rows: Vec<String> = Vec::new();

    // LEG 1: every mapped record tag is in the crate's own contract text.
    for row in step["matrix"].as_array().unwrap() {
        let record = row[1].as_str().unwrap();
        if !SCHEMA.contains(&format!("record={record}")) {
            rows.push(format!(
                "{{\"kind\":\"schema_missing\",\"v5\":\"{}\",\"record\":\"{record}\"}}",
                row[0].as_str().unwrap()
            ));
        }
    }

    // LEG 2: the per-file planes really emit what the matrix maps. `file`
    // rides --file-fact rather than the family mask, so it is covered by
    // 4_capability_parity's binary leg, not by dispatch.
    let mut wanted: Vec<String> = Vec::new();
    for row in step["matrix"].as_array().unwrap() {
        if row[2].as_bool().unwrap() && row[1].as_str().unwrap() != "file" {
            let record = row[1].as_str().unwrap().to_string();
            if !wanted.contains(&record) {
                wanted.push(record);
            }
        }
    }
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    for fixture in step["fixtures"].as_array().unwrap() {
        let relative = fixture[0].as_str().unwrap();
        let mask = match fixture[1].as_str().unwrap() {
            // `doc_node` is projected only when the raw cst plane is NOT
            // requested (`22_doc_node.rs:19-20`), so the markdown row cannot
            // ride `FamilyMask::ALL`.
            "types_only" => FamilyMask {
                cst: false,
                types: true,
                call: false,
                df: false,
                data: false,
            },
            _ => FamilyMask::ALL,
        };
        let path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
        let content = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let Some(out) = dispatch(relative, &content, mask) else {
            panic!("no Source claimed {relative}");
        };
        for fact in flatten(&out) {
            let value = serde_json::to_value(&fact).expect("a fact serializes");
            if let Some(tag) = value.get("record").and_then(serde_json::Value::as_str) {
                seen.insert(tag.to_string());
            }
        }
    }
    for record in &wanted {
        if seen.contains(record) {
            rows.push(format!("{{\"kind\":\"emitted\",\"record\":\"{record}\"}}"));
        }
    }

    // LEG 3: oracle coverage, enforced against the live roster. A new arm
    // cannot land without either a captured oracle or a stated waiver.
    let graded: Vec<(String, String)> = step["graded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row[0].as_str().unwrap().to_string(),
                row[1].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let waived: Vec<(String, String)> = step["waived"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row[0].as_str().unwrap().to_string(),
                row[1].as_str().unwrap().to_string(),
            )
        })
        .collect();
    for (name, _) in &graded {
        if waived.iter().any(|(waived, _)| waived == name) {
            rows.push(format!("{{\"kind\":\"overlap\",\"name\":\"{name}\"}}"));
        }
    }
    for name in waived.iter().map(|(name, _)| name).chain(graded.iter().map(|(name, _)| name)) {
        if sources().iter().all(|source| source.name() != name.as_str()) {
            rows.push(format!("{{\"kind\":\"stale\",\"name\":\"{name}\"}}"));
        }
    }
    for source in sources() {
        let name = source.name();
        if graded.iter().any(|(graded, _)| graded == name) {
            rows.push(format!("{{\"kind\":\"graded\",\"name\":\"{name}\"}}"));
        } else if let Some((_, reason)) = waived.iter().find(|(waived, _)| waived == name) {
            rows.push(format!(
                "{{\"kind\":\"waived\",\"name\":\"{name}\",\"reason\":\"{reason}\"}}"
            ));
        } else {
            rows.push(format!("{{\"kind\":\"unclassified\",\"name\":\"{name}\"}}"));
        }
    }

    // LEG 4: every graded language still has at least one captured oracle.
    for (name, dir) in &graded {
        let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/{dir}"));
        let captures = std::fs::read_dir(&fixtures)
            .unwrap_or_else(|error| panic!("read {}: {error}", fixtures.display()))
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".v5.jsonl"))
            .count();
        if captures == 0 {
            rows.push(format!(
                "{{\"kind\":\"oracle_gap\",\"lang\":\"{name}\"}}"
            ));
        }
    }

    serde_json::Value::String(rows.join("\n"))
}
