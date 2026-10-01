//! TypeSpec through the real binary: the cst plane over the `.tsp` fixture and
//! one `ryii query` over the vendored grammar's own node kinds.

use serde_json::Value;
use std::collections::BTreeMap;
use std::process::Command;

const FIXTURE: &str = "tests/fixtures/typespec/sample.tsp";

fn ryii(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(args)
        .output()
        .expect("ryii runs");
    assert!(
        output.status.success(),
        "ryii {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn typespec_cst_nodes_and_model_names_query() {
    let rows: Vec<Value> = ryii(&["--kinds", "cst", FIXTURE])
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let nodes: Vec<&Value> = rows.iter().filter(|row| row["record"] == "node").collect();
    let edges = rows.iter().filter(|row| row["record"] == "edge").count();
    assert_eq!((nodes.len(), edges), (129, 128));

    let named: Vec<String> = nodes
        .iter()
        .filter_map(|node| {
            Some(format!(
                "{} {}",
                node["kind"].as_str()?,
                node["name"].as_str()?
            ))
        })
        .collect();
    assert_eq!(
        named.join("\n"),
        "\
namespace_statement Pets
scalar_statement PetId
enum_statement Kind
enum_member dog
enum_member cat
model_statement Pet
model_property id
model_property name
model_property kind
model_property tags
model_statement Owner
model_property id
decorator key
model_property pets
operation_statement listPets
decorator route
model_property limit
decorator query"
    );

    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for node in &nodes {
        *kinds.entry(node["kind"].as_str().unwrap()).or_default() += 1;
    }
    let kinds: Vec<String> = kinds
        .iter()
        .map(|(kind, n)| format!("{kind} {n}"))
        .collect();
    assert_eq!(
        kinds.join("\n"),
        "\
annotation 3
annotation_list 3
array_expression 3
builtin_type 5
decorator 3
decorator_arguments 1
enum_body 1
enum_member 2
enum_member_value 1
enum_statement 1
identifier 29
identifier_or_member_expression 14
import_statement 1
member_expression 1
model_body 2
model_expression 2
model_property 7
model_statement 2
namespace_statement 1
operation_arguments 1
operation_signature_declaration 1
operation_statement 1
plain_identifier 24
quoted_string_fragment 3
quoted_string_literal 3
reference_expression 9
scalar_extends 1
scalar_statement 1
source_file 1
using_statement 1
value_list 1"
    );

    let strip = format!(",\"path\":\"{FIXTURE}\"");
    assert_eq!(
        ryii(&["query", "--query", "(model_statement name: (identifier) @name)", FIXTURE])
            .replace(&strip, ""),
        "{\"end_line\":14,\"line\":14,\"name\":\"Pet\"}\n{\"end_line\":21,\"line\":21,\"name\":\"Owner\"}\n"
    );
    assert_eq!(
        ryii(&[
            "query",
            "--query",
            "((model_property name: (_) @key type: (_) @type) (#has-ancestor? @key model_statement))",
            FIXTURE,
        ])
        .replace(&strip, ""),
        "{\"end_line\":15,\"key\":\"id\",\"line\":15,\"type\":\"PetId\"}\n{\"end_line\":16,\"key\":\"name\",\"line\":16,\"type\":\"string\"}\n{\"end_line\":17,\"key\":\"kind\",\"line\":17,\"type\":\"Kind\"}\n{\"end_line\":18,\"key\":\"tags\",\"line\":18,\"type\":\"string[]\"}\n{\"end_line\":22,\"key\":\"id\",\"line\":22,\"type\":\"int64\"}\n{\"end_line\":23,\"key\":\"pets\",\"line\":23,\"type\":\"Pet[]\"}\n"
    );
}
