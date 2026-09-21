//! `ryi graph --from NAME`: the recursive closure seeded at NAME. One row per
//! node reached, at its shortest depth, over a four-file a->b->c->d chain.
#![cfg(feature = "cli")]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn trace_path(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "sprefa_graph_from_{tag}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    directory.join("graph-from.json")
}

fn rows(tag: &str, name: &str) -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["graph", "--json", "--from", name])
        .arg("tests/fixtures/graph_ts")
        .env("HAFLEY_TRACE", trace_path(tag))
        .env("RUST_LOG", "off")
        .output()
        .expect("graph binary runs");
    assert!(
        output.status.success(),
        "graph failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn the_chain_reaches_three_nodes_at_rising_depth() {
    let rows = rows("head", "chainA");
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows,
        serde_json::from_str::<Vec<Value>>(
            r#"[
            {"record":"graph_node","path":"tests/fixtures/graph_ts/chain_b.ts","name":"chainB","depth":1,"grade":"+","line":null},
            {"record":"graph_node","path":"tests/fixtures/graph_ts/chain_c.ts","name":"chainC","depth":2,"grade":"+","line":null},
            {"record":"graph_node","path":"tests/fixtures/graph_ts/chain_d.ts","name":"chainD","depth":3,"grade":"+","line":null}
            ]"#,
        )
        .unwrap()
    );
}

#[test]
fn a_seed_further_down_the_chain_reaches_fewer_nodes() {
    let depths: Vec<u64> = rows("mid", "chainC")
        .iter()
        .map(|row| row.get("depth").and_then(Value::as_u64).unwrap())
        .collect();
    assert_eq!(depths, vec![1]);
}

#[test]
fn the_tail_of_the_chain_reaches_nothing() {
    assert_eq!(rows("tail", "chainD").len(), 0);
}

#[test]
fn an_unknown_seed_reaches_nothing() {
    assert_eq!(rows("absent", "nobodyHere").len(), 0);
}
