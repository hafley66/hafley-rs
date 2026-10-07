//! Explicit native TypeScript barrel scaling benchmark.
use std::process::Command;
use std::time::Instant;

const RATIO_BUDGET: f64 = 2.5;

/// A barrel corpus of `n` leaf modules, one barrel starring all of them, and
/// `n` consumers each importing one name through it. The shape that makes the
/// plane work hardest: every consumer's binding walks the whole star list.
fn barrel_corpus(dir: &std::path::Path, n: usize) -> Vec<String> {
    let dir = dir.join(format!("n{n}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let mut paths = Vec::new();
    let mut barrel = String::new();
    for index in 0..n {
        let leaf = dir.join(format!("leaf{index}.ts"));
        std::fs::write(
            &leaf,
            format!("export function pick{index}(n: number): number {{ return n + {index}; }}\n"),
        )
        .expect("leaf file");
        paths.push(leaf.to_string_lossy().into_owned());
        barrel.push_str(&format!("export * from \"./leaf{index}.js\";\n"));
    }
    let barrel_path = dir.join("barrel.ts");
    std::fs::write(&barrel_path, barrel).expect("barrel file");
    paths.push(barrel_path.to_string_lossy().into_owned());
    for index in 0..n {
        let consumer = dir.join(format!("use{index}.ts"));
        std::fs::write(
            &consumer,
            format!(
                "import {{ pick{index} }} from \"./barrel.js\";\n\nexport function call{index}(): number {{ return pick{index}({index}); }}\n"
            ),
        )
        .expect("consumer file");
        paths.push(consumer.to_string_lossy().into_owned());
    }
    paths
}

fn resolve_wall(args: &[String]) -> f64 {
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("--resolve")
        .arg("--arms")
        .arg("call")
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    start.elapsed().as_secs_f64()
}

/// COUNT test on cost: doubling the corpus must not more than 2.5x the wall.
/// A ResolveExport that re-walked a barrel's star list per call site instead of
/// per binding would show up here as a quadratic, not as a wrong answer.
#[test]
#[ignore = "explicit wall bench: bench/scripts/6_wall_contracts.py"]
fn barrel_resolve_wall_grows_linearly_with_file_count() {
    let dir = std::env::temp_dir().join("sprefa-extract-54-module-plane");
    std::fs::create_dir_all(&dir).expect("scratch root");
    let small = barrel_corpus(&dir, 200);
    let large = barrel_corpus(&dir, 400);
    let wall200 = resolve_wall(&small);
    let wall400 = resolve_wall(&large);
    crate::wall_bench::check("tests/54_ts_module_plane.rs:barrel_resolve_wall_grows_linearly_with_file_count", (wall400 / wall200) as f64, (RATIO_BUDGET) as f64, false);
}
