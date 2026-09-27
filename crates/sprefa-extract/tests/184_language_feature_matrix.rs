use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn capabilities_roster_and_single_language_grammar_features_match() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("capabilities")
        .output()
        .expect("run capabilities");
    assert!(output.status.success());
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let sources: Vec<&str> = rows
        .iter()
        .filter(|row| row["source"] == true)
        .map(|row| row["language"].as_str().unwrap())
        .collect();
    assert_eq!(
        sources,
        [
            "rust",
            "go",
            "kotlin",
            "markdown",
            "prolog",
            "python",
            "data",
            "ts",
            "gdscript",
            "commonlisp",
            "fallback",
        ]
    );

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../hafley_scm/Cargo.toml");
    let grammar_features: &[(&str, &[&str])] = &[
        ("rust", &["tree-sitter-rust"]),
        (
            "typescript",
            &["tree-sitter-typescript", "tree-sitter-javascript"],
        ),
        ("go", &["tree-sitter-go"]),
        ("kotlin", &["tree-sitter-kotlin-sg"]),
        ("python", &["tree-sitter-python"]),
        ("prolog", &["tree-sitter-prolog"]),
        ("markdown", &["tree-sitter-md"]),
        (
            "data",
            &[
                "tree-sitter-json",
                "tree-sitter-yaml",
                "tree-sitter-toml-ng",
            ],
        ),
        ("fallback", &["tree-sitter-html"]),
        ("gdscript", &["tree-sitter-gdscript"]),
        ("commonlisp", &["tree-sitter-commonlisp"]),
    ];
    let all_grammars: Vec<&str> = grammar_features
        .iter()
        .flat_map(|(_, grammars)| grammars.iter().copied())
        .collect();

    for (feature, expected) in grammar_features {
        let source_name = if *feature == "typescript" {
            "ts"
        } else {
            feature
        };
        assert!(
            sources.iter().any(|source| *source == source_name),
            "{feature} has no capability row"
        );
        let output = Command::new(env!("CARGO"))
            .args([
                "tree",
                "--manifest-path",
                manifest.to_str().unwrap(),
                "--no-default-features",
                "--features",
                feature,
                "--edges",
                "features,normal,build",
            ])
            .output()
            .expect("run cargo tree");
        assert!(
            output.status.success(),
            "cargo tree failed for {feature}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let tree = String::from_utf8(output.stdout).unwrap();
        for grammar in *expected {
            assert!(tree.contains(grammar), "{feature} omits {grammar}");
        }
        for grammar in all_grammars
            .iter()
            .filter(|grammar| !expected.contains(grammar))
        {
            assert!(
                !tree.contains(grammar),
                "{feature} unexpectedly enables {grammar}"
            );
        }
    }

    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--manifest-path",
            manifest.to_str().unwrap(),
            "--no-default-features",
            "--features",
            "read",
            "--edges",
            "features,normal,build",
        ])
        .output()
        .expect("run aggregate cargo tree");
    assert!(output.status.success());
    let tree = String::from_utf8(output.stdout).unwrap();
    for grammar in &all_grammars {
        assert!(tree.contains(grammar), "read omits {grammar}");
    }

    let sprefa_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--manifest-path",
            sprefa_manifest.to_str().unwrap(),
            "--no-default-features",
            "--features",
            "cli,rust",
            "--edges",
            "features,normal,build",
        ])
        .output()
        .expect("run cli plus rust cargo tree");
    assert!(output.status.success());
    let tree = String::from_utf8(output.stdout).unwrap();
    assert!(tree.contains("tree-sitter-rust"));
    for grammar in all_grammars
        .iter()
        .filter(|grammar| **grammar != "tree-sitter-rust")
    {
        assert!(
            !tree.contains(grammar),
            "cli,rust unexpectedly enables {grammar}"
        );
    }
}
