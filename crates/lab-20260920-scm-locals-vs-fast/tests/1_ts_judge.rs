use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use lab_20260920_scm_locals_vs_fast::{analyze, NamedEdge};

#[test]
fn typescript_module_plane_judge() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../sprefa-extract/tests/fixtures/ts5_findings/module_plane");
    let mut paths = collect(&root);
    paths.sort();
    let query = std::fs::read_to_string(manifest.join("queries/typescript/locals.scm")).unwrap();
    let lab = paths
        .iter()
        .flat_map(|path| {
            analyze("ts", &query, std::slice::from_ref(path))
                .unwrap()
                .edges
        })
        .collect::<BTreeSet<_>>();

    let target = PathBuf::from(std::env::var("CARGO_TARGET_DIR").expect("CARGO_TARGET_DIR"));
    assert!(!target.to_string_lossy().contains("/.cache/boop/"));
    let ryi = target.join("debug/ryi");
    let output = Command::new(ryi)
        .arg("fast")
        .args(&paths)
        .env("HAFLEY_TRACE", manifest.join("traces/L5-fast.json"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let fast = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| NamedEdge {
            caller_path: row["caller_path"].as_str().unwrap_or("").into(),
            caller_name: row["caller_name"].as_str().unwrap_or("").into(),
            callee_path: row["callee_path"].as_str().unwrap_or("").into(),
            callee_name: row["callee_name"].as_str().unwrap_or("").into(),
        })
        .collect::<BTreeSet<_>>();
    let both = lab
        .intersection(&fast)
        .map(edge_text)
        .collect::<BTreeSet<_>>();
    let lab_only = lab
        .difference(&fast)
        .map(edge_text)
        .collect::<BTreeSet<_>>();
    let fast_only = fast
        .difference(&lab)
        .map(edge_text)
        .collect::<BTreeSet<_>>();
    assert_eq!(both, BTreeSet::new());
    assert_eq!(
        lab_only,
        set(&["shadow_private.ts:<root> -> shadow_private.ts:isIdentifier"])
    );
    assert_eq!(
        fast_only,
        set(&[
            "barrel_consumer.ts:run -> helpers.ts:normalize",
            "barrel_consumer.ts:run -> widgets.ts:widen",
            "cycle_consumer.ts:walk -> cycle_b.ts:fromB",
            "default_consumer.ts:callDefault -> default_target.ts:theDefault",
            "namespace_consumer.ts:callMember -> namespace_target.ts:member",
            "renamed_consumer.ts:callIt -> renamed_source.ts:inner",
            "shadow_consumer.ts:check -> shadow_export.ts:isIdentifier",
            "shadow_private.ts:parse -> shadow_private.ts:isIdentifier",
            "two_hop_consumer.ts:reach -> two_hop_inner.ts:deep",
        ]),
    );
    println!("ts edges both={} {both:?}", both.len());
    println!("ts edges lab-only={} {lab_only:?}", lab_only.len());
    println!("ts edges fast-only={} {fast_only:?}", fast_only.len());
}

fn edge_text(edge: &NamedEdge) -> String {
    format!(
        "{}:{} -> {}:{}",
        file(&edge.caller_path),
        edge.caller_name,
        file(&edge.callee_path),
        edge.callee_name
    )
}

fn file(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|part| part.to_str())
        .unwrap_or(path)
}

fn set(rows: &[&str]) -> BTreeSet<String> {
    rows.iter().map(|row| row.to_string()).collect()
}

fn collect(root: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(root)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|part| part.to_str()) == Some("ts"))
        .collect()
}
