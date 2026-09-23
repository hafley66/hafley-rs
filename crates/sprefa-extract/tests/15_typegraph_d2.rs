//! A board that does not compile is not a diagram. Every rule below is checked
//! by running the real `d2` binary; a missing `d2` fails loudly, never skips.

use std::path::{Path, PathBuf};
use std::process::Command;

/// At most this many shapes per board. Over budget SPLITS; it never crams.
const SHAPE_BUDGET: usize = 24;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sprefa-typegraph-d2-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn run_example(root: &str, entry: &str, out: &Path) -> String {
    let markdown = out.join("typegraph.md");
    std::fs::write(
        &markdown,
        "before\n<!-- ryi:typegraph-d2:start -->\n<!-- ryi:typegraph-d2:end -->\nafter\n",
    )
    .expect("markdown template");
    let output = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "run",
            "--quiet",
            "--example",
            "typegraph_d2",
            "--",
            "--root",
            root,
            "--entry",
            entry,
            "--out",
            &out.to_string_lossy(),
            "--markdown-into",
            &markdown.to_string_lossy(),
        ])
        .output()
        .expect("cargo run");
    assert!(
        output.status.success(),
        "the example exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn boards(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("out dir")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "d2"))
        .collect();
    found.sort();
    found
}

/// `direction` is a reserved keyword, not a shape; every other declaration line
/// in an emitted board is one shape, by construction (one line per shape).
fn shape_count(board: &Path) -> usize {
    std::fs::read_to_string(board)
        .expect("board")
        .lines()
        .filter(|line| !line.starts_with("direction:"))
        .filter(|line| {
            line.split_once(':').is_some_and(|(head, _)| {
                !head.is_empty()
                    && head
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '-')
            })
        })
        .count()
}

/// THE FOUR BOARD RULES, on real extractor output over this crate's own `src`.
///
/// 1. every board compiles under the real `d2`;
/// 2. the rendered viewBox is wider than it is tall;
/// 3. no board carries more than `SHAPE_BUDGET` shapes;
/// 4. the entrypoint appears on the first board.
///
/// NOT covered by this corpus: the over-budget chunk split. No hop band in
/// `src` reaches 24 nodes, so that branch is unexercised here rather than
/// asserted green.
#[test]
fn every_emitted_board_compiles_and_reads_wide() {
    let out = scratch("src");
    let stdout = run_example("src", "src/types.rs::RyiOutput", &out);

    let files = boards(&out);
    assert!(
        !files.is_empty(),
        "the example wrote no board; stdout was: {stdout}"
    );

    let first = std::fs::read_to_string(&files[0]).expect("first board");
    assert!(
        first.contains(": RyiOutput (hop 0"),
        "the entrypoint must be on the first board:\n{first}"
    );
    let markdown = std::fs::read_to_string(out.join("typegraph.md")).expect("generated markdown");
    assert!(markdown.starts_with("before\n<!-- ryi:typegraph-d2:start -->"));
    assert!(markdown.ends_with("<!-- ryi:typegraph-d2:end -->\nafter\n"));
    assert_eq!(markdown.matches("<details>").count(), files.len());

    for board in &files {
        let source = std::fs::read_to_string(board).expect("D2 source");
        assert!(
            markdown.contains(&format!("```d2\n{source}```")),
            "{} differs from the embedded board",
            board.display()
        );
        let svg = board.with_extension("svg");
        let render = Command::new("d2")
            .arg(board)
            .arg(&svg)
            .output()
            .unwrap_or_else(|err| {
                panic!("d2 is not on PATH and this gate never fakes green: {err}")
            });
        assert!(
            render.status.success(),
            "{} did not compile: {}",
            board.display(),
            String::from_utf8_lossy(&render.stderr)
        );

        let text = std::fs::read_to_string(&svg).expect("rendered svg");
        let view = text
            .split("viewBox=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("a viewBox on the rendered svg");
        let dims: Vec<f64> = view
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        assert_eq!(dims.len(), 4, "viewBox was {view:?}");
        assert!(
            dims[2] > dims[3],
            "{} rendered {}x{}, taller than wide; the board must read wide",
            board.display(),
            dims[2],
            dims[3]
        );

        let shapes = shape_count(board);
        assert!(
            shapes <= SHAPE_BUDGET,
            "{} carries {shapes} shapes, over the budget of {SHAPE_BUDGET}; split it",
            board.display()
        );
    }
}

fn d2_ident(path: &Path, name: &str) -> String {
    format!("{}__{name}", path.to_string_lossy())
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn board_summaries(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| {
            line.split_once(" layer=")
                .map(|(_, summary)| summary.to_string())
        })
        .collect()
}

/// SCC members share a topological layer while retaining separate hop labels.
/// Cross-board edges end at a visible board stub and name the target in the edge
/// label, so every directed resolver edge remains present in the export.
#[test]
fn scc_layers_keep_hops_cycles_and_cross_board_edges_deterministic() {
    let fixture_root = scratch("scc-fixture");
    let fixture_path = fixture_root.join("fixture.rs");
    std::fs::write(
        &fixture_path,
        "pub struct A { pub next: B }\npub struct B { pub back: A, pub next: C }\npub struct C { pub next: D }\npub struct D {}\n",
    )
    .expect("fixture source");
    let entry = format!("{}::A", fixture_path.display());
    let first_out = scratch("scc-run-one");
    let second_out = scratch("scc-run-two");

    let first_stdout = run_example(&fixture_root.to_string_lossy(), &entry, &first_out);
    let second_stdout = run_example(&fixture_root.to_string_lossy(), &entry, &second_out);
    let first_files = boards(&first_out);
    let second_files = boards(&second_out);
    assert_eq!(
        first_files.len(),
        3,
        "each SCC topological layer gets a board"
    );
    assert_eq!(second_files.len(), first_files.len());

    let first_sources: Vec<String> = first_files
        .iter()
        .map(|path| std::fs::read_to_string(path).expect("board source"))
        .collect();
    let second_sources: Vec<String> = second_files
        .iter()
        .map(|path| std::fs::read_to_string(path).expect("repeat board source"))
        .collect();
    assert_eq!(
        first_sources, second_sources,
        "board bytes are stable across runs"
    );

    assert_eq!(
        board_summaries(&first_stdout),
        vec![
            "0 nodes=2 shapes=3 edges=3".to_string(),
            "1 nodes=1 shapes=2 edges=1".to_string(),
            "2 nodes=1 shapes=1 edges=0".to_string(),
        ]
    );
    assert_eq!(
        board_summaries(&second_stdout),
        board_summaries(&first_stdout),
        "board summaries are stable across runs"
    );

    let first = &first_sources[0];
    let second = &first_sources[1];
    let third = &first_sources[2];
    let a = d2_ident(&fixture_path, "A");
    let b = d2_ident(&fixture_path, "B");
    let c = d2_ident(&fixture_path, "C");
    let d = d2_ident(&fixture_path, "D");
    assert!(first.contains(": A (hop 0, SCC 1 cycle) {"), "{first}");
    assert!(first.contains(": B (hop 1, SCC 1 cycle) {"), "{first}");
    assert!(first.contains(&format!("{a} -> {b}: field")), "{first}");
    assert!(first.contains(&format!("{b} -> {a}: field")), "{first}");
    assert!(first.contains("boundary stub {"), "{first}");
    assert!(first.contains(&format!("{b} -> ryi_boundary_stub_1: field to C at ")));
    assert!(first.contains("(board 2)"), "{first}");
    assert!(second.contains(": C (hop 2) {"), "{second}");
    assert!(second.contains(&format!("{c} -> ryi_boundary_stub_2: field to D at ")));
    assert!(second.contains("(board 3)"), "{second}");
    assert!(third.contains(&format!("{d}: D (hop 3) {{")), "{third}");

    let edge_count = first_sources
        .iter()
        .flat_map(|source| source.lines())
        .filter(|line| line.contains(" -> "))
        .count();
    assert_eq!(edge_count, 4, "all four reachable directed edges are drawn");

    for (path, source) in first_files.iter().zip(&first_sources) {
        assert!(
            shape_count(path) <= SHAPE_BUDGET,
            "{} exceeded the shape budget:\n{source}",
            path.display()
        );
    }
}

/// An entrypoint that names no type node is a nonzero exit with a message, not
/// an empty board.
#[test]
fn an_unknown_entrypoint_exits_nonzero() {
    let out = scratch("missing");
    let output = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "run",
            "--quiet",
            "--example",
            "typegraph_d2",
            "--",
            "--root",
            "src",
            "--entry",
            "src/types.rs::NoSuchTypeAnywhere",
            "--out",
            &out.to_string_lossy(),
        ])
        .output()
        .expect("cargo run");
    assert!(!output.status.success(), "an unknown entrypoint must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("NoSuchTypeAnywhere"),
        "the message must name what was not found: {stderr}"
    );
}
