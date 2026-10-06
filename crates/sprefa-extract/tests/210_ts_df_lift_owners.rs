use super::v5_support::{project, rows, snapshots};
use serde_json::{json, Value};

#[test]
fn ts_df_lift_owners_whole_output() {
    let _snapshots = snapshots();
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/df_owner_lift");
    let mut fixtures: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    fixtures.sort();
    let mut calls = Vec::new();
    let mut whole = Vec::new();
    for fixture in fixtures {
        let name = fixture.file_name().unwrap().to_str().unwrap();
        let source = std::fs::read_to_string(&fixture).unwrap();
        let facts = rows(name, &source, false);
        let mut owners: Vec<Value> = facts
            .iter()
            .filter(|row| row["record"] == "node" && row["kind"] == "call_res")
            .map(|row| {
                json!([
                    &source[row["span"]["start"].as_u64().unwrap() as usize
                        ..row["span"]["end"].as_u64().unwrap() as usize],
                    row["function"]
                ])
            })
            .collect();
        owners.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
        calls.push(json!({"fixture": name, "calls": owners}));
        whole.push(format!("{name}\n{}", project(&facts)));
    }
    // Expected ownership is derived from the fixture's lexical frames.
    insta::assert_json_snapshot!(calls, @r###"
    [
      {
        "fixture": "0_statements.ts",
        "calls": [
          [
            "readCatch()",
            "outer"
          ],
          [
            "readFinally()",
            "outer"
          ],
          [
            "readLabel()",
            "outer"
          ],
          [
            "readNamed()",
            "inner"
          ],
          [
            "readTry()",
            "outer"
          ]
        ]
      },
      {
        "fixture": "1_bindings.ts",
        "calls": [
          [
            "readAssignment()",
            "outer::closure::78"
          ],
          [
            "readBinding()",
            "outer::closure::37"
          ],
          [
            "readReturned()",
            "exported::closure::151"
          ]
        ]
      },
      {
        "fixture": "2_class.ts",
        "calls": [
          [
            "readField()",
            ".field::closure::19"
          ],
          [
            "readMethod()",
            ".method"
          ]
        ]
      },
      {
        "fixture": "3_default.ts",
        "calls": [
          [
            "readDefault()",
            "<top>::closure::15"
          ]
        ]
      }
    ]
    "###);
    insta::assert_snapshot!("ts_df_lift_owners_whole_output", whole.join("\n\n"));
}
