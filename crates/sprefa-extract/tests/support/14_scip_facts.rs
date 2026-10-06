use serde_json::{json, Value};
use sprefa_extract::{flatten_scip, OccurrenceRole, ScipDiagnostic, ScipDocument, ScipIndex, ScipOccurrence, ScipSignature, ScipSymbolInfo, SymbolInterner};

fn occurrence(row: &Value, symbols: &mut SymbolInterner) -> ScipOccurrence {
    ScipOccurrence {
        symbol: symbols.intern(row["symbol"].as_str().unwrap()), range: serde_json::from_value(row["range"].clone()).unwrap(), roles: OccurrenceRole(row["roles"].as_i64().unwrap() as i32), syntax_kind: row["syntax_kind"].as_i64().unwrap() as i32, enclosing_range: None,
        override_documentation: serde_json::from_value(row["override_documentation"].clone()).unwrap(),
        diagnostics: row["diagnostics"].as_array().unwrap().iter().map(|d| ScipDiagnostic { severity:d["severity"].as_i64().unwrap() as i32, code:d["code"].as_str().unwrap().into(), message:d["message"].as_str().unwrap().into(), source:d["source"].as_str().unwrap().into(), tags:serde_json::from_value(d["tags"].clone()).unwrap() }).collect(),
    }
}

pub fn evaluate(case: &Value) -> Value {
    crate::fixture_runner::commands(case, |step| {
        if step["api"] == "flatten" {
            let mut symbols = SymbolInterner::default();
            let occurrences = step["occurrences"].as_array().unwrap().iter().map(|row| occurrence(row, &mut symbols)).collect();
            let infos = step["symbols"].as_array().unwrap().iter().map(|row| ScipSymbolInfo {
                symbol:symbols.intern(row["symbol"].as_str().unwrap()), display_name:row["display_name"].as_str().unwrap().into(), kind:row["kind"].as_i64().unwrap() as i32, relationships:Vec::new(), documentation:Vec::new(), enclosing_symbol:row["enclosing_symbol"].as_str().unwrap().into(),
                signature:Some(ScipSignature { language:row["signature"]["language"].as_str().unwrap().into(), text:row["signature"]["text"].as_str().unwrap().into(), occurrences:row["signature"]["occurrences"].as_array().unwrap().iter().map(|row| occurrence(row, &mut symbols)).collect() }),
            }).collect();
            let index = ScipIndex { documents:vec![ScipDocument { relative_path:step["path"].as_str().unwrap().into(), occurrences, symbols:infos, ..ScipDocument::default() }], symbols:symbols.table(), ..ScipIndex::default() };
            return json!(flatten_scip(&index, &|_| Some(step["source"].as_str().unwrap().as_bytes().to_vec())).iter().map(|row| serde_json::to_string(row).unwrap()).collect::<Vec<_>>().join("\n"));
        }
        let text = std::fs::read_to_string(step["path"].as_str().unwrap()).unwrap();
        match step["api"].as_str().unwrap() {
            "golden" => {
                let rows: String = text.lines().filter(|line| !line.contains("\"record\":\"scip_metadata\"")).map(|line| format!("{line}\n")).collect();
                assert_eq!(rows, std::fs::read_to_string(crate::fixture_runner::expand_text(step["expected"].as_str().unwrap(), "")).unwrap());
                json!({"golden_equal":true})
            },
            "prefix" => {
                let (first, rest) = text.split_once('\n').unwrap();
                assert_eq!(first, step["expected"].as_str().unwrap());
                assert_eq!(rest, std::fs::read_to_string(step["plain"].as_str().unwrap()).unwrap());
                json!({"first":first,"rest_equal":true})
            },
            "locals" => {
                let count = text.lines().filter(|line| line.contains("\"symbol\":\"local ")).count();
                assert!(count > step["minimum"].as_u64().unwrap() as usize);
                json!({"local_rows_above_minimum":true,"minimum":step["minimum"]})
            },
            other => panic!("unknown API {other}"),
        }
    })
}
