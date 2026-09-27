use std::path::{Path, PathBuf};
use std::process::Command;

fn rust_project() -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("sprefa-fast-path-spelling-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"path_spelling_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub mod api;\npub use crate::api::Thing;\n",
    )
    .unwrap();
    std::fs::write(root.join("src/api.rs"), "pub struct Thing;\n").unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "sprefa-extract")
            .env("GIT_AUTHOR_EMAIL", "sprefa-extract@example.invalid")
            .env("GIT_COMMITTER_NAME", "sprefa-extract")
            .env("GIT_COMMITTER_EMAIL", "sprefa-extract@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-qm", "fixture"]);
    root
}

fn fast(root: &Path, path: &str) -> Vec<serde_json::Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["fast", path])
        .current_dir(root)
        .env("RUST_LOG", "off")
        .output()
        .expect("ryii runs");
    assert!(
        output.status.success(),
        "ryii fast {path}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|row: &serde_json::Value| row["record"] == "resolved_import")
        .collect()
}

fn normalized(mut rows: Vec<serde_json::Value>, root: &Path) -> Vec<String> {
    let prefix = format!("{}/", root.display());
    for row in &mut rows {
        for field in ["src_path", "target_path"] {
            if let Some(path) = row[field].as_str() {
                if let Some(relative) = path.strip_prefix(&prefix) {
                    row[field] = relative.into();
                }
            }
        }
    }
    let mut rows: Vec<_> = rows
        .into_iter()
        .map(|row| serde_json::to_string(&row).unwrap())
        .collect();
    rows.sort();
    rows
}

#[test]
fn equivalent_directory_spellings_resolve_the_same_imports_and_keep_output_paths() {
    let root = rust_project();
    let relative = fast(&root, ".");
    let absolute = fast(&root, root.to_str().unwrap());

    assert_eq!(
        normalized(relative.clone(), &root),
        normalized(absolute.clone(), &root)
    );
    assert!(relative
        .iter()
        .all(|row| row["src_path"].as_str().unwrap().starts_with("src/")));
    assert!(absolute.iter().all(|row| {
        row["src_path"]
            .as_str()
            .unwrap()
            .starts_with(&format!("{}/src/", root.display()))
    }));
    assert!(
        !relative.is_empty(),
        "fixture must exercise resolved imports"
    );
    std::fs::remove_dir_all(root).unwrap();
}
