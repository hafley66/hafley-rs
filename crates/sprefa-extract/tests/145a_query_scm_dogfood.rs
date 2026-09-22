//! Dogfood: every predicate family of `ryi query` runs through the real
//! binary, the real Rust grammar, and the shared `hafley_scm` engine. One
//! temporary Rust file, one JSONL snapshot per query, no library-level fakes.

use std::process::Command;

const SRC: &str = "\
fn alpha() -> bool {
    let seed = \"seed\";
    seed.contains(\"seed\")
}

fn beta() -> bool {
    let grain = \"oat\";
    grain.contains(\"oat\")
}
";

fn jsonl(path: &std::path::Path, query: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["query", "--lang", "rust", "--query", query])
        .arg(path)
        .output()
        .expect("ryi binary runs");
    assert!(
        output.status.success(),
        "ryi query failed: {}",
        String::from_utf8(output.stderr).unwrap()
    );
    String::from_utf8(output.stdout).unwrap()
}

fn temp_file() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "query_scm_dogfood_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sample.rs");
    std::fs::write(&path, SRC).unwrap();
    path
}

#[test]
fn ryi_query_predicate_families_through_hafley_scm() {
    let path = temp_file();

    // 1. host ancestor walk: the let pattern identifiers, not the call sites.
    assert_eq!(
        jsonl(&path, "((identifier) @name (#has-parent? @name let_declaration))"),
        "{\"end_line\":2,\"line\":2,\"name\":\"seed\"}\n{\"end_line\":7,\"line\":7,\"name\":\"grain\"}\n"
    );

    // 2. host contains: the seed literals, not the oat ones.
    assert_eq!(
        jsonl(&path, "((string_literal) @lit (#contains? @lit \"seed\"))"),
        "{\"end_line\":2,\"line\":2,\"lit\":\"\\\"seed\\\"\"}\n{\"end_line\":3,\"line\":3,\"lit\":\"\\\"seed\\\"\"}\n"
    );

    // 3. generic not- form over a host predicate.
    assert_eq!(
        jsonl(&path, "((string_literal) @lit (#not-contains? @lit \"seed\"))"),
        "{\"end_line\":7,\"line\":7,\"lit\":\"\\\"oat\\\"\"}\n{\"end_line\":8,\"line\":8,\"lit\":\"\\\"oat\\\"\"}\n"
    );

    // 4. native eq?, evaluated by the tree-sitter cursor itself.
    assert_eq!(
        jsonl(&path, "((identifier) @name (#eq? @name \"seed\"))"),
        "{\"end_line\":2,\"line\":2,\"name\":\"seed\"}\n{\"end_line\":3,\"line\":3,\"name\":\"seed\"}\n"
    );

    // 5. native not-eq?.
    assert_eq!(
        jsonl(&path, "((identifier) @name (#not-eq? @name \"seed\"))"),
        "{\"end_line\":1,\"line\":1,\"name\":\"alpha\"}\n{\"end_line\":6,\"line\":6,\"name\":\"beta\"}\n{\"end_line\":7,\"line\":7,\"name\":\"grain\"}\n{\"end_line\":8,\"line\":8,\"name\":\"grain\"}\n"
    );

    // 6. native any-of? over several literals.
    assert_eq!(
        jsonl(&path, "((identifier) @name (#any-of? @name \"alpha\" \"beta\"))"),
        "{\"end_line\":1,\"line\":1,\"name\":\"alpha\"}\n{\"end_line\":6,\"line\":6,\"name\":\"beta\"}\n"
    );

    // 7. two patterns with conflicting eq? predicates: each pattern keeps the
    //    rows its own predicate passes, so neither predicate leaks into the
    //    other pattern's matches.
    assert_eq!(
        jsonl(
            &path,
            "((identifier) @name (#eq? @name \"seed\"))\n((identifier) @name (#eq? @name \"grain\"))"
        ),
        "{\"end_line\":2,\"line\":2,\"name\":\"seed\"}\n{\"end_line\":3,\"line\":3,\"name\":\"seed\"}\n{\"end_line\":7,\"line\":7,\"name\":\"grain\"}\n{\"end_line\":8,\"line\":8,\"name\":\"grain\"}\n"
    );

    std::fs::remove_file(&path).ok();
}

/// `#set!` metadata is outside this cut: `MatchArena` carries no settings.
/// tree-sitter parks `#set!` in `property_settings`, which `hafley_scm::build`
/// neither reads nor rejects, so such a query runs while the setting is
/// silently absent from every projected row. Unignore when the arena grows a
/// settings plane; until then this test records the gap instead of claiming
/// support.
#[test]
#[ignore = "MatchArena does not yet carry #set! settings"]
fn set_metadata_settings_are_absent_from_match_arena() {
    let path = temp_file();
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args([
            "query",
            "--lang",
            "rust",
            "--query",
            "((identifier) @name (#set! \"kind\" \"binding\"))",
        ])
        .arg(&path)
        .output()
        .expect("ryi binary runs");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "{\"end_line\":1,\"line\":1,\"name\":\"alpha\"}\n{\"end_line\":2,\"line\":2,\"name\":\"seed\"}\n{\"end_line\":3,\"line\":3,\"name\":\"seed\"}\n{\"end_line\":6,\"line\":6,\"name\":\"beta\"}\n{\"end_line\":7,\"line\":7,\"name\":\"grain\"}\n{\"end_line\":8,\"line\":8,\"name\":\"grain\"}\n"
    );
    std::fs::remove_file(&path).ok();
}
