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
    rows_in(tag, name, "tests/fixtures/graph_ts")
}

fn rows_in(tag: &str, name: &str, corpus: &str) -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["graph", "--from", name])
        .arg(corpus)
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
            {"record":"graph_node","path":"tests/fixtures/graph_ts/chain_b.ts","name":"chainB","depth":1,"grade":"+","line":3},
            {"record":"graph_node","path":"tests/fixtures/graph_ts/chain_c.ts","name":"chainC","depth":2,"grade":"+","line":3},
            {"record":"graph_node","path":"tests/fixtures/graph_ts/chain_d.ts","name":"chainD","depth":3,"grade":"+","line":2}
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
    assert_eq!(
        rows("absent", "nobodyHere"),
        [serde_json::json!({"record":"seed_unmatched","seed":"nobodyHere","reason":"no_declaration"})]
    );
}

/// Two files declare `build`. Bare NAME seeds both; `PATH#NAME` seeds the
/// declarations in files whose path ends with PATH, and a PATH no file ends
/// with seeds nothing.
#[test]
fn a_path_anchor_selects_one_of_several_same_named_seeds() {
    let corpus = "tests/fixtures/graph_ts_same_name";
    let reached = |name: &str| {
        rows_in("anchor", name, corpus)
            .iter()
            .map(|row| match row["path"].as_str() {
                Some(path) => format!("\n{path} {} {}", row["name"].as_str().unwrap(), row["depth"]),
                None => format!("\n{} {}", row["record"].as_str().unwrap(), row["reason"].as_str().unwrap()),
            })
            .collect::<String>()
    };
    assert_eq!(
        [
            "build",
            "a/build.ts#build",
            "graph_ts_same_name/b/build.ts#build",
            "build.ts#build",
            "c/build.ts#build",
            "uild.ts#build",
        ]
        .map(|name| format!("{name}:{}", reached(name)))
        .join("\n"),
        "build:
tests/fixtures/graph_ts_same_name/a/build.ts stepA 1
tests/fixtures/graph_ts_same_name/b/build.ts stepB 1
a/build.ts#build:
tests/fixtures/graph_ts_same_name/a/build.ts stepA 1
graph_ts_same_name/b/build.ts#build:
tests/fixtures/graph_ts_same_name/b/build.ts stepB 1
build.ts#build:
tests/fixtures/graph_ts_same_name/a/build.ts stepA 1
tests/fixtures/graph_ts_same_name/b/build.ts stepB 1
c/build.ts#build:
seed_unmatched no_declaration
uild.ts#build:
seed_unmatched no_declaration"
    );
}
