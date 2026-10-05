use super::df_syntax_rows_from_tree;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[test]
fn tree_df_rows_match_snapshot_across_pinned_rust_fixtures() {
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sprefa-extract/tests/fixtures");
    let roots = [
        "type_ladder",
        "type_ladder_scope",
        "ratchet_soopy",
        "call_ladder",
    ];
    let mut files = Vec::new();
    for root in roots {
        rust_files(&fixtures.join(root), &mut files);
    }
    files.sort();
    assert!(!files.is_empty());

    let mut actual = String::new();
    for path in files {
        let source = std::fs::read(&path).expect("fixture reads");
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter::Language::new(tree_sitter_rust::LANGUAGE))
            .expect("Rust grammar");
        let tree = parser.parse(&source, None).expect("tree-sitter parse");
        let rows = df_syntax_rows_from_tree(&tree, "fixture.rs", &source);
        let relative = path.strip_prefix(&fixtures).expect("fixture-relative path");
        let rendered = format!("{rows:#?}");
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hasher::write(&mut hasher, rendered.as_bytes());
        writeln!(
            &mut actual,
            "{} nodes={} edges={} params={} args={} fields={} literals={} loops={} allocations={} loop_spans={} allocator_hits={} debug_hash={:016x}",
            relative.display(),
            rows.nodes.len(),
            rows.edges.len(),
            rows.aux.params.len(),
            rows.aux.args.len(),
            rows.aux.fields.len(),
            rows.aux.lits.len(),
            rows.aux.loops.len(),
            rows.aux.allocates.len(),
            rows.aux.loop_collection_spans.len(),
            rows.aux.allocator_hits.len(),
            std::hash::Hasher::finish(&hasher),
        )
        .unwrap();
    }

    let snapshot = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/lang/rust/11_df_syntax_rows.tree.snap");
    if std::env::var_os("UPDATE_TREE_DF_SNAPSHOT").is_some() {
        std::fs::write(snapshot, &actual).expect("write tree DF snapshot");
        return;
    }
    assert_eq!(actual, include_str!("11_df_syntax_rows.tree.snap"));
}

fn rust_files(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
