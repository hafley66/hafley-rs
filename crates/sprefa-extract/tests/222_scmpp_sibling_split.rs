//! A flat scm++ level with 3+ unanchored sibling children warns; when every child is a plain
//! node pattern it splits into one parent edge per child plus a `sib` order condition, and
//! answers the rows tree-sitter answers for the unsplit pattern.
use std::collections::BTreeMap;

use super::v5_support::snapshots;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/scmpp_siblings");

fn ryii(arguments: &[&str]) -> (Vec<BTreeMap<String, String>>, String) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let rows = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let row: BTreeMap<String, serde_json::Value> = serde_json::from_str(line).unwrap();
            row.into_iter()
                .filter(|(key, _)| !matches!(key.as_str(), "path" | "line" | "end_line"))
                .filter_map(|(key, value)| {
                    let label = match key.strip_suffix("__text") {
                        Some(label) => label.to_string(),
                        None if key.contains("__") => return None,
                        None => key,
                    };
                    Some((label, value.to_string()))
                })
                .collect()
        })
        .collect();
    (rows, String::from_utf8(output.stderr).unwrap())
}

/// The oracle: tree-sitter's own cursor over the unsplit pattern, captures by label, text values.
fn tree_sitter_rows(pattern: &str, source: &[u8]) -> Vec<BTreeMap<String, String>> {
    use tree_sitter::StreamingIterator as _;
    let language = sprefa_extract::RyiLang::parse_name("typescript").unwrap().tree_sitter_language();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let query = tree_sitter::Query::new(&language, pattern).unwrap();
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source);
    let mut rows = Vec::new();
    while let Some(found) = matches.next() {
        let mut row = BTreeMap::<String, Vec<String>>::new();
        for capture in found.captures() {
            let text = String::from_utf8_lossy(&source[capture.node.byte_range()]).into_owned();
            row.entry(query.capture_names()[capture.index as usize].to_string()).or_default().push(text);
        }
        rows.push(
            row.into_iter()
                .map(|(label, texts)| {
                    let value = match texts.as_slice() {
                        [one] => serde_json::Value::from(one.as_str()),
                        many => serde_json::Value::from(many.to_vec()),
                    };
                    (label, value.to_string())
                })
                .collect(),
        );
    }
    rows
}

#[test]
fn scmpp_sibling_split_answers_tree_sitter_rows() {
    let _snapshots = snapshots();
    let source = format!("{DIR}/source.ts");
    let mut cases: Vec<_> = std::fs::read_dir(DIR)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "scm"))
        .collect();
    cases.sort();
    let table: Vec<_> = cases
        .iter()
        .map(|case| {
            let pattern = std::fs::read_to_string(case).unwrap();
            let (mut split, stderr) = ryii(&["query", "--scmpp", case.to_str().unwrap(), &source]);
            let (mut plain, _) = ryii(&["query", "--query", &pattern, &source]);
            let mut oracle = tree_sitter_rows(&pattern, &std::fs::read(&source).unwrap());
            split.sort();
            plain.sort();
            oracle.sort();
            serde_json::json!({
                "case": case.file_name().unwrap().to_str().unwrap(),
                "warning": stderr.lines().filter(|line| line.contains("scm++ level")).collect::<Vec<_>>(),
                "scmpp_same_as_tree_sitter": split == oracle,
                "query_same_as_tree_sitter": plain == oracle,
                "rows": split.len(),
            })
        })
        .collect();
    insta::assert_json_snapshot!("scmpp_sibling_split_answers_tree_sitter_rows", table);
}

/// 100 pairs, C(100, 3) rows: tree-sitter alone runs past a minute; the split answers in seconds.
#[test]
fn scmpp_sibling_split_answers_100_pairs() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("object.ts");
    let keys: String = (0..100).map(|key| format!("  k{key}: {key},\n")).collect();
    std::fs::write(&file, format!("export const big = {{\n{keys}}};\n")).unwrap();
    // `--timeout 20` bounds the run: past it the command exits 3 and `ryii` fails.
    let (rows, _) = ryii(&[
        "query",
        "--scmpp",
        &format!("{DIR}/0_pairs.scm"),
        "--timeout",
        "20",
        file.to_str().unwrap(),
    ]);
    assert_eq!(rows.len(), 161_700);
}
