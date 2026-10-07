use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::process::Command;

use sprefa_extract::{resolve_specifier, Policy, TsconfigPaths};

const DEPS_ROOT: &str = "tests/fixtures/deps";

/// One `--deps` run over the whole corpus, one `--kinds call` run over
/// `app.ts`, the three library policy tables, and the named no-root error;
/// every old assert runs as code BEFORE the snapshot freezes the tables.
pub fn evaluate(_case: &Value) -> Value {
    // The diet resolver's universe IS its argument list, so the corpus is
    // named explicitly.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DEPS_ROOT);
    let mut corpus: Vec<String> = walk(&root);
    corpus.sort();
    let mut args: Vec<&str> = vec!["--deps", "--root", DEPS_ROOT];
    args.extend(corpus.iter().map(String::as_str));
    let deps_stdout = run(&args, true);
    let deps_lines: Vec<&str> = deps_stdout.lines().collect();

    // THE GOLDEN. Nine edges: the edge key is (src, dst, kind), so
    // `./lib/bare` carries a named row and a namespace row and `./lib/util.ts`
    // carries three, and `symbols` counts the distinct bound names OF THAT
    // KIND exactly as the SCIP fold counts distinct symbols. Hand-derived from
    // the fixture, never copied from the binary.
    let edges: Vec<&str> = deps_lines
        .iter()
        .filter(|line| line.contains(r#""record":"file_edge""#))
        .copied()
        .collect();
    let mut expected = [
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/bare.ts","kind":"named","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/bare.ts","kind":"namespace","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/helper.ts","kind":"named","symbols":2}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/mapped.ts","kind":"named","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/util.ts","kind":"default","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/util.ts","kind":"named","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/util.ts","kind":"reexport","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"side.ts","kind":"side_effect","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"widget/index.ts","kind":"named","symbols":1}"#,
    ];
    expected.sort();
    assert_eq!(edges, expected, "the --deps edge golden");

    // The three NON-edges: `rxjs` stops at the node_modules boundary,
    // `/lib/util.ts` is filesystem-absolute, `./gone.ts` names nothing.
    for line in &edges {
        for stopped in ["rxjs", "gone", "/lib/util.ts"] {
            assert!(!line.contains(stopped), "{stopped} minted an edge: {line}");
        }
    }

    // EVERY STOP IS A ROW; a dropped stop is indistinguishable from an import
    // that was never written.
    let unresolved: Vec<&str> = deps_lines
        .iter()
        .filter(|line| line.contains(r#""record":"file_unresolved""#))
        .copied()
        .collect();
    let mut expected_stops = [
        r#"{"record":"file_unresolved","src_path":"app.ts","module":"./gone.ts","reason":"relative_unresolved"}"#,
        r#"{"record":"file_unresolved","src_path":"app.ts","module":"/lib/util.ts","reason":"absolute_path"}"#,
        r#"{"record":"file_unresolved","src_path":"app.ts","module":"rxjs","reason":"node_modules_boundary"}"#,
    ];
    expected_stops.sort();
    assert_eq!(unresolved, expected_stops, "every stop is a row");

    // Specifier rows: the source module and the imported name.
    let kinds_stdout = run(&["--kinds", "call", &format!("{DEPS_ROOT}/app.ts")], true);
    let specifiers: Vec<&str> = kinds_stdout
        .lines()
        .filter(|line| line.contains(r#""record":"specifier""#))
        .collect();
    for expected in [
        r#""name":"exact","kind":"named","module":"./lib/util.ts","imported":null"#,
        r#""name":"boxed","kind":"named","module":"./widget","imported":null"#,
        r#""name":"of","kind":"named","module":"rxjs","imported":null"#,
        r#""name":"reexported","kind":"reexport","module":"./lib/util.ts","imported":null"#,
        r#""name":"./side.ts","kind":"side_effect","module":"./side.ts","imported":null"#,
        r#""name":"outer","kind":"named","module":"./lib/helper.js","imported":"inner""#,
        r#""name":"defaults","kind":"default","module":"./lib/util.ts","imported":"default""#,
        r#""name":"everything","kind":"namespace","module":"./lib/bare","imported":null"#,
    ] {
        assert!(
            specifiers.iter().any(|line| line.contains(expected)),
            "missing {expected}"
        );
    }

    // Each policy asserted directly, including the two stops and the
    // unresolved case: the resolver says WHY.
    let universe: BTreeSet<String> = [
        "app.ts",
        "lib/util.ts",
        "lib/helper.ts",
        "lib/bare.ts",
        "lib/mapped.ts",
        "widget/index.ts",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let tsconfig = TsconfigPaths::read(std::path::Path::new(DEPS_ROOT));
    let cases: &[(&str, Option<&str>, Policy)] = &[
        ("./lib/util.ts", Some("lib/util.ts"), Policy::RelativeExact),
        ("./lib/helper.js", Some("lib/helper.ts"), Policy::RelativeEmittedRewrite),
        ("./lib/bare", Some("lib/bare.ts"), Policy::RelativeExtensionInferred),
        ("./widget", Some("widget/index.ts"), Policy::RelativeIndexFile),
        ("@app/mapped", Some("lib/mapped.ts"), Policy::TsconfigPaths),
        ("rxjs", None, Policy::NodeModulesBoundary),
        ("node:fs", None, Policy::NodeModulesBoundary),
        ("/etc/passwd", None, Policy::AbsolutePath),
        ("./gone.ts", None, Policy::RelativeUnresolved),
    ];
    let mut policy_rows = Vec::new();
    for (specifier, target, policy) in cases {
        let (hit, applied) = resolve_specifier("app.ts", specifier, &universe, &tsconfig);
        assert_eq!(hit.as_deref(), *target, "target for {specifier}");
        assert_eq!(&applied, policy, "policy for {specifier}");
        policy_rows.push(json!([specifier, hit, applied_name(&applied)]));
    }

    // `..` climbs lexically; climbing past the root clamps rather than escapes.
    let small: BTreeSet<String> = ["a/b/c.ts", "a/shared.ts", "top.ts"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let default_tsconfig = TsconfigPaths::default();
    let parent_cases: &[(&str, &str, Option<&str>)] = &[
        ("a/b/c.ts", "../shared.ts", Some("a/shared.ts")),
        ("a/b/c.ts", "../../top.ts", Some("top.ts")),
        ("a/b/c.ts", "../../../../top.ts", Some("top.ts")),
        ("a/b/c.ts", "./nope.ts", None),
    ];
    let mut parent_rows = Vec::new();
    for (from, specifier, target) in parent_cases {
        let (hit, _) = resolve_specifier(from, specifier, &small, &default_tsconfig);
        assert_eq!(hit.as_deref(), *target, "{from} + {specifier}");
        parent_rows.push(json!([from, specifier, hit]));
    }

    // The tsconfig reader survives comments and trailing commas and degrades
    // to EMPTY, never to wrong.
    assert_eq!(tsconfig.base_url.as_deref(), Some(""));
    assert_eq!(
        tsconfig.paths.get("@app/*").map(Vec::as_slice),
        Some(["lib/*".to_string()].as_slice())
    );
    let broken = TsconfigPaths::parse("{ this is not json ");
    assert_eq!(broken, TsconfigPaths::default());
    let one: BTreeSet<String> = ["lib/mapped.ts"].into_iter().map(str::to_string).collect();
    let (hit, policy) = resolve_specifier("app.ts", "@app/mapped", &one, &broken);
    assert_eq!(hit, None);
    assert_eq!(policy, Policy::NodeModulesBoundary);

    // `--deps` without `--root` is a named error: no answer without a root.
    let refused = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["--deps", &format!("{DEPS_ROOT}/app.ts")])
        .output()
        .expect("extract binary runs");
    assert!(!refused.status.success());
    let message = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(
        message.contains("--root"),
        "the error must name what is missing, got: {message}"
    );

    json!({
        "file_edges": edges,
        "file_unresolved": unresolved,
        "specifier_rows": specifiers,
        "policy_rows": policy_rows,
        "parent_traversal": parent_rows,
        "tsconfig": {
            "base_url": tsconfig.base_url,
            "paths": tsconfig.paths,
            "broken_parses_to_default": true,
            "broken_resolve": [hit, applied_name(&policy)],
        },
        "no_root_error": message,
    })
}

fn applied_name(policy: &Policy) -> String {
    format!("{policy:?}")
}

fn run(args: &[&str], expect_success: bool) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(args)
        .output()
        .expect("extract binary runs");
    assert_eq!(
        output.status.success(),
        expect_success,
        "{args:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn walk(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("ts") {
                out.push(path.to_string_lossy().to_string());
            }
        }
    }
    out
}
