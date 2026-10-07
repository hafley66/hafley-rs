use serde_json::{json, Value};
use std::process::Command;

const RUST: &str = "tests/fixtures/rust/sample.rs";
const TS: &str = "tests/fixtures/ts/sample.ts";

/// The `ryi query` CLI: flat JSONL for plain and alternating patterns,
/// predicate filtering, exit-two rejects, the staged-blob digest door, and
/// the md/md_inline/html grammars. The old exact-string asserts run live;
/// the relative-path streams freeze whole and the temp-path streams freeze
/// path-stripped.
pub fn evaluate(_case: &Value) -> Value {
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ryii"))
            .args(args)
            .output()
            .expect("extract binary runs")
    };
    let run_in = |dir: &std::path::Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ryii"))
            .current_dir(dir)
            .args(args)
            .output()
            .expect("extract binary runs")
    };
    let git = |dir: &std::path::Path, args: &[&str]| {
        Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("git runs")
    };

    // Plain and alternating patterns both emit one flat JSON row per match.
    let plain = run(&[
        "query",
        "--lang",
        "rust",
        "--query",
        "(function_item name: (identifier) @name) @item",
        RUST,
    ]);
    assert!(plain.status.success());
    let plain_stdout = String::from_utf8(plain.stdout).unwrap();
    let alternate = run(&[
        "query",
        "--lang",
        "rust",
        "--query",
        "[(function_item name: (identifier) @name) @item (struct_item name: (type_identifier) @name) @item]",
        RUST,
    ]);
    assert!(alternate.status.success());
    let alternate_stdout = String::from_utf8(alternate.stdout).unwrap();
    assert_eq!(alternate_stdout.lines().count(), 5);
    assert_eq!(
        plain_stdout
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|row| row["name"] == "trim" || row["name"] == "make_engine")
            .map(|row| json!([row["line"], row["end_line"], row["name"]]))
            .collect::<Vec<_>>(),
        [json!([15, 17, "trim"]), json!([19, 23, "make_engine"])],
    );

    // A #match? predicate filters the matches.
    let predicate = run(&[
        "query",
        "--lang",
        "ts",
        "--query",
        "((function_declaration name: (identifier) @name) @item (#match? @name \"^s\"))",
        TS,
    ]);
    assert!(predicate.status.success());
    let predicate_stdout = String::from_utf8(predicate.stdout).unwrap();
    let predicate_row: Value =
        serde_json::from_str(predicate_stdout.trim()).expect("one predicate row");
    assert_eq!(predicate_row["name"], "shift");
    assert_eq!(predicate_row["line"], 7);
    assert_eq!(predicate_row["end_line"], 12);
    assert!(predicate_row["item"]
        .as_str()
        .unwrap()
        .starts_with("function shift(p: Point, d: Dir): Vec2 {"));

    // An unknown lang and an invalid query are exit-two with the named reason.
    let unknown = run(&[
        "query",
        "--lang",
        "klingon",
        "--query",
        "(identifier) @name",
        RUST,
    ]);
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(unknown.stderr).unwrap(),
        "unknown lang 'klingon'\n"
    );
    let invalid = run(&["query", "--lang", "rust", "--query", "(", RUST]);
    assert_eq!(invalid.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&invalid.stderr).lines().count(), 1);

    // --digest reads the staged blob instead of the worktree file: the rows
    // match the path run's rows once the differing path column is stripped.
    let dir = std::env::temp_dir().join(format!(
        "blobdoor_query_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    let fixture = std::fs::canonicalize(RUST).unwrap();
    let hash = git(&dir, &["hash-object", "-w", fixture.to_str().unwrap()]);
    let oid = String::from_utf8(hash.stdout).unwrap().trim().to_string();
    let query = "(function_item name: (identifier) @name) @item";
    let blob_path = dir.join("sample.rs");
    let via_digest = run_in(
        &dir,
        &[
            "query",
            "--lang",
            "rust",
            "--query",
            query,
            "--digest",
            &oid,
            blob_path.to_str().unwrap(),
        ],
    );
    assert!(via_digest.status.success());
    let without_path = |stdout: &[u8], path: &str| {
        String::from_utf8_lossy(stdout).replace(&format!(",\"path\":\"{path}\""), "")
    };
    assert_eq!(
        without_path(&via_digest.stdout, blob_path.to_str().unwrap()),
        plain_stdout.replace(&format!(",\"path\":\"{RUST}\""), "")
    );

    // A bad digest is exit-two with one stderr line naming the git door.
    let bad_digest = run(&[
        "query",
        "--lang",
        "rust",
        "--query",
        "(identifier) @name",
        "--digest",
        "0000000000000000000000000000000000000000",
        RUST,
    ]);
    assert_eq!(bad_digest.status.code(), Some(2));
    let stderr = String::from_utf8(bad_digest.stderr).unwrap();
    assert_eq!(stderr.lines().count(), 1);
    assert!(stderr.contains("git cat-file blob"));

    // The md, md_inline and html grammars: headings, inline drops, tag names.
    let temp_file = |name: &str, content: &str| {
        let dir = std::env::temp_dir().join(format!(
            "query_cli_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    };
    let strip = |stdout: &[u8]| {
        String::from_utf8_lossy(stdout)
            .lines()
            .map(|line| {
                let mut row: Value = serde_json::from_str(line).unwrap();
                row["path"] = json!("<path>");
                row
            })
            .collect::<Vec<_>>()
    };
    let md_path = temp_file("sample.md", "# Title\n\nA paragraph.\n\n```js\ncode\n```\n");
    let markdown = run(&[
        "query",
        "--lang",
        "md",
        "--query",
        "(atx_heading) @heading",
        md_path.to_str().unwrap(),
    ]);
    assert!(markdown.status.success());
    let md_inline_path = temp_file("sample_inline.md", "This is *em* and [link](url).");
    let markdown_inline = run(&[
        "query",
        "--lang",
        "md_inline",
        "--query",
        "[(emphasis) @em (inline_link) @lnk]",
        md_inline_path.to_str().unwrap(),
    ]);
    assert!(markdown_inline.status.success());
    let html_path = temp_file("sample.html", "<div class=\"x\"><p>hi</p></div>");
    let html = run(&[
        "query",
        "--lang",
        "html",
        "--query",
        "(element (start_tag (tag_name) @tag))",
        html_path.to_str().unwrap(),
    ]);
    assert!(html.status.success());

    json!({
        "plain_rows": plain_stdout.lines().count(),
        "plain_named_rows": [["trim", 15, 17], ["make_engine", 19, 23]],
        "alternate_rows": 5,
        "predicate_row": [predicate_row["name"], predicate_row["line"], predicate_row["end_line"]],
        "unknown_lang_stderr": "unknown lang 'klingon'\n",
        "invalid_query_exit": 2,
        "digest_matches_path_stream": true,
        "bad_digest_exit": 2,
        "markdown_rows": strip(&markdown.stdout),
        "markdown_inline_rows": strip(&markdown_inline.stdout),
        "html_rows": strip(&html.stdout),
    })
}
