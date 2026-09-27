use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "../src/1_analyze.rs"]
mod analyze;
#[path = "../src/0_types.rs"]
mod types;

fn fixture(files: &[(&str, &str)]) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("dead-files-{}-{nonce}", std::process::id()));
    for (path, contents) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    root
}

#[test]
fn dead_file_view_excludes_entries_and_inbound_edges() {
    let root = fixture(&[
        (
            "package.json",
            r#"{"main":"src/main.ts","exports":{".":"src/export.ts"}}"#,
        ),
        ("src/main.ts", ""),
        ("src/export.ts", ""),
        ("src/used.ts", ""),
        ("src/orphan.ts", ""),
    ]);
    let edges = analyze::read_edges(std::io::Cursor::new(
        r#"{"record":"file_edge","src_path":"src/main.ts","dst_path":"src/used.ts"}
"#,
    ))
    .unwrap();
    let dead = analyze::dead_files(&root, &edges).unwrap();
    assert_eq!(
        dead.iter()
            .map(|row| row.path.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        ["src/orphan.ts"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn orphan_comparison_lists_agreement_and_each_disagreement() {
    let root = fixture(&[("a.ts", ""), ("b.ts", ""), ("c.ts", "")]);
    let ryi = [types::DeadFile {
        path: PathBuf::from("a.ts"),
    }];
    let comparison =
        analyze::compare_orphans(&ryi, "Processed 3 files (1ms)\n\na.ts\nb.ts\n", &root).unwrap();
    assert_eq!(
        comparison,
        analyze::OrphanComparison {
            both: ["a.ts"].into_iter().map(str::to_owned).collect(),
            ryi_only: vec![],
            tool_only: ["b.ts"].into_iter().map(str::to_owned).collect(),
        }
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rustc_dead_code_diagnostics_project_primary_source_files() {
    let root = fixture(&[("src/lib.rs", ""), ("src/unused.rs", "")]);
    let cargo_json = format!(
        "{{\"reason\":\"compiler-message\",\"message\":{{\"code\":{{\"code\":\"dead_code\"}},\"spans\":[{{\"file_name\":\"{}\",\"is_primary\":true}}]}}}}\n",
        root.join("src/unused.rs").display()
    );
    assert_eq!(
        analyze::rustc_dead_code_files(&cargo_json, &root).unwrap(),
        ["src/unused.rs"].into_iter().map(str::to_owned).collect()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rustc_path_array_compares_against_file_orphans() {
    let root = fixture(&[
        ("src/lib.rs", ""),
        ("src/live.rs", ""),
        ("src/orphan.rs", ""),
    ]);
    let ryi = [types::DeadFile {
        path: PathBuf::from("src/orphan.rs"),
    }];
    let comparison = analyze::compare_orphans(&ryi, r#"["src/live.rs"]"#, &root).unwrap();
    assert_eq!(
        comparison,
        analyze::OrphanComparison {
            both: vec![],
            ryi_only: ["src/orphan.rs"].into_iter().map(str::to_owned).collect(),
            tool_only: ["src/live.rs"].into_iter().map(str::to_owned).collect(),
        }
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unresolved_module_rows_without_destinations_are_readable() {
    let rows = analyze::read_edges(std::io::Cursor::new(
        r#"{"record":"file_unresolved","src_path":"src/lib.rs","module":"live"}
"#,
    ))
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].record, "file_unresolved");
    assert_eq!(rows[0].dst_path, "");
}
