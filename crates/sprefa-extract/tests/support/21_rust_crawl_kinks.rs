use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::process::Command;

use sprefa_extract::{FamilyMask, RustSource, Source};

pub fn evaluate(_case: &Value) -> Value {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let sets = [
        ("const_block", vec!["tests/fixtures/rust_findings/const_block_defs.rs"]),
        ("static_init", vec!["tests/fixtures/rust_findings/static_init_call.rs"]),
        ("closure_chain", vec!["tests/fixtures/rust_findings/closure_caller_chain.rs"]),
        ("closure_mirror", vec!["tests/fixtures/rust_findings/closure_mirror_count.rs"]),
        (
            "qualified",
            vec![
                "tests/fixtures/rust_findings/qualified_path/main.rs",
                "tests/fixtures/rust_findings/qualified_path/wrapper.rs",
                "tests/fixtures/rust_findings/qualified_path/util/mod.rs",
                "tests/fixtures/rust_findings/qualified_path/util/deep.rs",
            ],
        ),
        ("unresolved", vec!["tests/fixtures/rust_findings/unresolved_reason.rs"]),
        (
            "ts_pair",
            vec!["tests/fixtures/resolve/0_caller.ts", "tests/fixtures/resolve/1_callee.ts"],
        ),
    ];
    let mut tables = BTreeMap::new();
    for (name, paths) in &sets {
        let facts = resolve_facts(paths);
        tables.insert(
            format!("{name}_edges"),
            json!(sorted_edges(&facts, name)),
        );
        let mut unresolved: Vec<(String, String)> = facts
            .iter()
            .filter(|fact| fact["record"] == "unresolved")
            .map(|fact| {
                (
                    fact["reason"].as_str().unwrap_or("").to_string(),
                    fact["detail"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        unresolved.sort();
        tables.insert(format!("{name}_unresolved"), json!(unresolved));
    }

    let extract_names = |path: &str| {
        let bytes = std::fs::read(format!("{manifest}/{path}")).expect("fixture readable");
        let output = RustSource.extract(path, &bytes, FamilyMask::ALL);
        let call = output.call.as_ref().expect("the call family is projected");
        let mut names: Vec<String> = call
            .nodes
            .iter()
            .filter_map(|node| node.name.map(|id| output.strings.lookup(id).to_string()))
            .collect();
        names.sort();
        names
    };
    let const_nodes = extract_names("tests/fixtures/rust_findings/const_block_defs.rs");
    let quiet_nodes =
        extract_names("tests/fixtures/rust_findings/no_call_initializers.rs");
    let unresolved_sites: usize = {
        let bytes =
            std::fs::read(format!("{manifest}/tests/fixtures/rust_findings/unresolved_reason.rs"))
                .expect("fixture readable");
        RustSource
            .extract("tests/fixtures/rust_findings/unresolved_reason.rs", &bytes, FamilyMask::ALL)
            .call
            .as_ref()
            .expect("the call family is projected")
            .aux
            .sites
            .len()
    };

    // kink 5: const-block fns mint call defs; the sibling call resolves.
    assert_eq!(const_nodes, vec!["inner", "outer"], "const-block defs");
    assert_eq!(
        tables["const_block_edges"],
        json!([[ "outer", "inner", 869, "name_resolve" ]]),
        "const-block sibling call"
    );
    // kink 6: initializer calls carry the item as caller; call-free initializers mint nothing.
    let static_edges = tables["static_init_edges"].as_array().unwrap();
    assert_eq!(
        static_edges.iter().map(|edge| edge[0].as_str().unwrap()).collect::<Vec<_>>(),
        ["ROW", "TABLE"],
        "initializer callers"
    );
    assert!(
        static_edges.iter().all(|edge| edge[1] == "helper"),
        "both initializer edges name helper: {static_edges:?}"
    );
    assert!(quiet_nodes.is_empty(), "call-free initializers minted {quiet_nodes:?}");
    // kink 3: closure edges mirror onto the innermost enclosing named def, once each.
    let chain = tables["closure_chain_edges"].as_array().unwrap();
    let worker_callers: std::collections::BTreeSet<&str> = chain
        .iter()
        .filter(|edge| edge[1] == "worker")
        .map(|edge| edge[0].as_str().unwrap())
        .collect();
    assert_eq!(
        worker_callers,
        ["closure@1182", "entry"].into_iter().collect(),
        "closure caller mirrors"
    );
    assert_eq!(chain.len(), 3, "2 primary edges + 1 mirror: {chain:?}");
    let mirror = tables["closure_mirror_edges"].as_array().unwrap();
    let closure_edges: Vec<&Value> = mirror
        .iter()
        .filter(|edge| edge[0].as_str().unwrap().starts_with("closure@"))
        .collect();
    assert_eq!(closure_edges.len(), 3, "{mirror:?}");
    for closure in &closure_edges {
        let site = closure[2].as_u64().unwrap();
        let callee = closure[1].as_str().unwrap();
        assert!(
            mirror.iter().any(|edge| {
                edge[2].as_u64() == Some(site)
                    && edge[1].as_str() == Some(callee)
                    && !edge[0].as_str().unwrap().starts_with("closure@")
            }),
            "no mirror for {closure:?} among {mirror:?}"
        );
    }
    assert!(
        mirror
            .iter()
            .filter(|edge| !edge[0].as_str().unwrap().starts_with("closure@")
                && closure_edges.iter().any(|closure| {
                    closure[2] == edge[2] && closure[1] == edge[1]
                }))
            .all(|mirror| mirror[0] == "entry"),
        "every mirror names the innermost enclosing NAMED def: {mirror:?}"
    );
    // 5 primaries (one per site) + 3 mirrors. Before the fix: 5.
    assert_eq!(mirror.len(), 8, "{mirror:?}");
    // kink 4: module-qualified calls bind in the module the path names.
    assert_eq!(
        tables["qualified_edges"],
        json!([
            ["main", "wrapper.rs", "main"],
            ["relative", "util/deep.rs", "local"],
            ["relative", "util/mod.rs", "helper"],
            ["spread", "main.rs", "build"],
            ["spread", "util/deep.rs", "helper"],
            ["spread", "util/mod.rs", "helper"],
        ]),
        "qualified bounds"
    );
    let spread = tables["qualified_edges"]
        .as_array().unwrap()
        .iter()
        .filter(|edge| edge[0] == "spread")
        .collect::<Vec<_>>();
    assert_eq!(spread.len(), 3, "3 of the 4 calls in `spread` bind");
    assert!(
        spread.iter().any(|edge| edge[2] == "build"),
        "Widget::build keeps the name leg: {spread:?}"
    );
    let qualified_unresolved = tables["qualified_unresolved"].as_array().unwrap();
    assert!(
        !qualified_unresolved
            .iter()
            .any(|row| row[1].as_str() == Some("Widget::build")),
        "a bound site mints no unresolved row"
    );
    assert!(
        qualified_unresolved.contains(&json!(["no_corpus_def", "other_crate::helper"])),
        "other_crate names no corpus module: {qualified_unresolved:?}"
    );
    // kink 7: every dropped site mints exactly one unresolved row naming why.
    assert_eq!(
        tables["unresolved_unresolved"],
        json!([["no_corpus_def", "Vec::new"]]),
        "unresolved reasons"
    );
    assert_eq!(unresolved_sites, 3, "first::compute, second::compute, Vec::new");
    let dropped = tables["unresolved_unresolved"].as_array().unwrap().len();
    assert_eq!(
        dropped,
        unresolved_sites - tables["unresolved_edges"].as_array().unwrap().len(),
        "sites {unresolved_sites}, rows {dropped}"
    );
    // The unresolved channel stays inside the rust arm.
    assert!(
        tables["ts_pair_unresolved"].as_array().unwrap().is_empty(),
        "ts resolve mints no unresolved rows"
    );

    tables.insert("const_block_nodes".to_string(), json!(const_nodes));
    tables.insert("no_call_initializer_nodes".to_string(), json!(quiet_nodes));
    tables.insert("unresolved_sites".to_string(), json!(unresolved_sites));
    json!(tables)
}

fn resolve_facts(paths: &[&str]) -> Vec<Value> {
    let out = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("--resolve")
        .args(["--arms", "call"])
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a flat fact is JSON"))
        .collect()
}

/// `(caller, callee, site, kind)` for qualified-name sets, `(caller,
/// callee-file, callee)` for the qualified set; both sorted, per the tables
/// the old tests graded.
fn sorted_edges(facts: &[Value], set: &str) -> Vec<Value> {
    if set == "qualified" {
        let mut bound: Vec<(String, String, String)> = facts
            .iter()
            .filter(|fact| fact["record"] == "resolved_edge")
            .map(|fact| {
                (
                    fact["caller_name"].as_str().unwrap_or("").to_string(),
                    fact["callee_path"]
                        .as_str()
                        .unwrap_or("")
                        .rsplit_once("qualified_path/")
                        .map_or_else(String::new, |(_, tail)| tail.to_string()),
                    fact["callee_name"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        bound.sort();
        bound.into_iter().map(|(a, b, c)| json!([a, b, c])).collect()
    } else {
        let mut edges: Vec<(String, String, u64, String)> = facts
            .iter()
            .filter(|fact| fact["record"] == "resolved_edge")
            .map(|fact| {
                (
                    fact["caller_name"].as_str().unwrap_or("").to_string(),
                    fact["callee_name"].as_str().unwrap_or("").to_string(),
                    fact["caller_site_start"].as_u64().unwrap_or(0),
                    fact["kind"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        edges.sort();
        edges
            .into_iter()
            .map(|(a, b, c, d)| json!([a, b, c, d]))
            .collect()
    }
}
