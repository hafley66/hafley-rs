#![cfg(feature = "cli")]

use super::v5_support::{rows, run};
use serde_json::{json, Value};

struct Fixture {
    path: std::path::PathBuf,
    nodes: Vec<Value>,
    source: String,
}

impl Fixture {
    fn new(name: &str, source: &str, directory: &std::path::Path) -> Self {
        let path = directory.join(name);
        std::fs::write(&path, source).unwrap();
        let nodes = rows(path.to_str().unwrap(), source, false)
            .into_iter()
            .filter(|row| row["record"] == "node" && row["family"] == "df")
            .collect();
        Self {
            path,
            nodes,
            source: source.into(),
        }
    }

    fn named(&self, kind: &str, name: &str, function: &str) -> Vec<&Value> {
        self.nodes
            .iter()
            .filter(|node| {
                node["kind"] == kind && node["name"] == name && node["function"] == function
            })
            .collect()
    }

    fn calls(&self, text: &str) -> Vec<&Value> {
        self.nodes
            .iter()
            .filter(|node| {
                let span = &node["span"];
                node["kind"] == "call_res"
                    && &self.source[span["start"].as_u64().unwrap() as usize
                        ..span["end"].as_u64().unwrap() as usize]
                        == text
            })
            .collect()
    }

    fn walk(&self, node: &Value, reverse: bool) -> Vec<Value> {
        let span = &node["span"];
        let seed = format!("{}@{}:{}", self.path.display(), span["start"], span["end"]);
        let mut arguments = vec!["graph", "--flow-path", &seed];
        if reverse {
            arguments.push("--reverse");
        }
        let result = run(&arguments, &self.path);
        String::from_utf8(result.stdout)
            .unwrap()
            .lines()
            .map(|row| serde_json::from_str(row).unwrap())
            .collect()
    }

    fn check(&self, label: &str, from: Vec<&Value>, to: Vec<&Value>, expected: bool) -> Value {
        assert!(
            !from.is_empty() && !to.is_empty(),
            "{label}: fixture selectors must select nodes"
        );
        let forward: Vec<Value> = from
            .iter()
            .flat_map(|node| self.walk(node, false))
            .collect();
        let reverse: Vec<Value> = to.iter().flat_map(|node| self.walk(node, true)).collect();
        let has = |rows: &[Value], nodes: &[&Value]| {
            rows.iter().any(|row| {
                nodes.iter().any(|node| {
                    row["to_name"] == format!("{}:{}", node["span"]["start"], node["span"]["end"])
                })
            })
        };
        let actual = json!({"forward":has(&forward, &to), "reverse":has(&reverse, &from)});
        assert_eq!(
            actual,
            json!({"forward":expected, "reverse":expected}),
            "{label}"
        );
        json!({"case":label, "reaches":actual, "sources":from, "targets":to, "forward":forward, "reverse":reverse})
    }
}

#[test]
fn seeded_forward_and_reverse_reach_port_v5_dataflow_cases() {
    let _snapshots = super::v5_support::snapshots();
    let directory = tempfile::tempdir().unwrap();
    let fixtures = [
        ("6_chain.rs", include_str!("fixtures/v5_parity/6_chain.rs")),
        (
            "1_widget.ts",
            include_str!("fixtures/v5_parity/1_widget.ts"),
        ),
        ("9_chain.rs", include_str!("fixtures/v5_parity/9_chain.rs")),
        (
            "10_chain.ts",
            include_str!("fixtures/v5_parity/10_chain.ts"),
        ),
        (
            "7_branches.rs",
            include_str!("fixtures/v5_parity/7_branches.rs"),
        ),
        (
            "8_breaks.rs",
            include_str!("fixtures/v5_parity/8_breaks.rs"),
        ),
        ("12_labeled.rs", super::labeled_break::SOURCE),
        (
            "11_return.ts",
            include_str!("fixtures/v5_parity/11_return.ts"),
        ),
    ];
    let fixtures: Vec<Fixture> = fixtures
        .iter()
        .map(|(name, source)| Fixture::new(name, source, directory.path()))
        .collect();
    let mut output = vec![
        fixtures[0].check(
            "v5:140 name->u",
            fixtures[0].named("param", "name", "f"),
            fixtures[0].named("var_read", "u", "f"),
            true,
        ),
        fixtures[1].check(
            "v5:239 Widget.render name->label",
            fixtures[1].named("param", "name", "Widget.render"),
            fixtures[1].named("let_bind", "label", "Widget.render"),
            true,
        ),
        fixtures[2].check(
            "v5:341 Rust q->m",
            fixtures[2].named("param", "q", "go"),
            fixtures[2].named("var_read", "m", "go"),
            true,
        ),
        fixtures[3].check(
            "v5:341 TS q->m",
            fixtures[3].named("param", "q", "go"),
            fixtures[3].named("var_read", "m", "go"),
            true,
        ),
    ];
    for (index, label, calls, bindings) in [
        (
            4,
            "v5:925",
            vec!["produce()", "fallback()", "pick(k)"],
            vec!["x", "y"],
        ),
        (
            5,
            "v5:1029",
            vec!["produce()", "fallback()"],
            vec!["outcome"],
        ),
        (6, "v5:1116", vec!["produce()"], vec!["outcome"]),
    ] {
        let fixture = &fixtures[index];
        for call in calls {
            for node in fixture.calls(call) {
                // v5 branches contain two fallback calls, each reaches its own binding.
                let target = if index == 4
                    && node["span"]["start"].as_u64().unwrap()
                        > fixture.source.find("let y").unwrap() as u64
                {
                    "y"
                } else {
                    bindings[0]
                };
                output.push(fixture.check(
                    &format!("{label} {call}@{}->{target}", node["span"]["start"]),
                    vec![node],
                    fixture.named("let_bind", target, "orchestrate"),
                    true,
                ));
            }
        }
    }
    output.push(fixtures[7].check(
        "callee return->caller binding",
        fixtures[7].named("param", "value", "identity"),
        fixtures[7].named("let_bind", "result", "entry"),
        true,
    ));
    insta::assert_json_snapshot!(
        "v5_parity__reach__seeded_forward_and_reverse_reach_port_v5_dataflow_cases",
        output
    );
}

#[test]
fn jsx_ten_forward_and_reverse_reach_checks_match_v5() {
    let _snapshots = super::v5_support::snapshots();
    let directory = tempfile::tempdir().unwrap();
    let (_, source) = super::jsx::fixtures()[1];
    let fixture = Fixture::new("4_jsx_exprs.tsx", source, directory.path());
    let output: Vec<Value> = [
        ("secret", "title", true),
        ("fallback", "title", true),
        ("secret", "subtitle", true),
        ("backup", "subtitle", true),
        ("secret", "note", true),
        ("secret", "label", true),
        ("bag", "opt", true),
        ("first", "items", true),
        ("secret", "items", true),
        ("guarded", "note", false),
    ]
    .iter()
    .map(|(name, prop, expected)| {
        fixture.check(
            &format!("{name}->Card.{prop}"),
            fixture.named("var_read", name, "App"),
            fixture.named("param", prop, "Card"),
            *expected,
        )
    })
    .collect();
    insta::assert_json_snapshot!(
        "v5_parity__reach__jsx_ten_forward_and_reverse_reach_checks_match_v5",
        output
    );
}
