use serde_json::{json, Value};
use std::path::PathBuf;

use sprefa_extract::{rehome_for, ts_specifiers, FamilyMask, MoveCx, PrologSource, Source, TsSource};

const PATH: &str = "tests/fixtures/ts_move/src/index.ts";
const SOURCE: &str = include_str!("../fixtures/ts_move/src/index.ts");

/// The TS corpus walk (arc 1) and the module-specifier rows the move rewrites
/// by (arc 2). Expected values are hand-derived from the fixture text, never
/// copied from the extractor's output; every old assert runs as code BEFORE
/// the snapshot freezes the tables.
pub fn evaluate(_case: &Value) -> Value {
    // SABOTAGE RECEIPT: dropping the `visit_ts_import_equals_declaration` arm
    // leaves row 11 missing; taking `named.span` instead of
    // `import.source.span` for `module_span` makes row 1's slice read `thing`
    // instead of `'./b.ts'`.
    let scanned = ts_specifiers(PATH, SOURCE).expect("fixture parses");
    let rows: Vec<Value> = scanned
        .iter()
        .map(|row| {
            let start = row.module_span.start as usize;
            let end = row.module_span.end() as usize;
            json!([row.kind.as_str(), row.module.as_str(), &SOURCE[start..end]])
        })
        .collect();
    assert_eq!(
        rows,
        [
            json!(["named", "./b.ts", "'./b.ts'"]),
            json!(["named", "./b", "'./b'"]),
            json!(["named", "./dir", "'./dir'"]),
            json!(["named", "./b.js", "'./b.js'"]),
            json!(["named", "@app/b", "'@app/b'"]),
            json!(["named", "pkg-exports", "'pkg-exports'"]),
            json!(["reexport", "./reexport", "'./reexport'"]),
            json!(["reexport", "./dir", "'./dir'"]),
            json!(["dynamic_import", "./b.js", "'./b.js'"]),
            json!(["require", "./b", "'./b'"]),
            json!(["require", "./b", "'./b'"]),
        ]
    );

    // Every row's quote is the one the literal was written with, and every
    // span starts on it: what a re-aim needs to replace the literal whole.
    let double = "import a from \"./b\";\n";
    let double_rows = ts_specifiers("x.ts", double).expect("parses");
    assert_eq!(double_rows.len(), 1);
    assert_eq!(double_rows[0].quote, '"');
    let text =
        &double[double_rows[0].module_span.start as usize..double_rows[0].module_span.end() as usize];
    assert_eq!(text, "\"./b\"");
    assert!(scanned.iter().all(|row| row.quote == '\''));

    // A computed path has none to record, so it stays out of the specifier
    // rows (it is `CallFAux.unresolved`'s row); a plain string is not a
    // specifier.
    let computed = "const name = './b';\nconst m = import(name);\nconst r = require(name);\n\
                    const s = 'hello';\nreadFile('./b');\n";
    assert!(
        ts_specifiers("x.ts", computed).expect("parses").is_empty(),
        "computed paths and plain strings are not specifiers"
    );

    // A runtime module reference nested in a function body is still a row:
    // the scan visits the whole tree, not just top-level statements.
    let nested = "async function load() {\n  const m = await import('./deep');\n\
                  \n  return require('./other');\n}\n";
    let nested_rows: Vec<Value> = nested_scan(nested);
    assert_eq!(
        nested_rows,
        [json!(["dynamic_import", "./deep"]), json!(["require", "./other"])]
    );

    // The extractor's own `CallFAux.specifiers` carry the same set through
    // `TsSource`: the move and the fact plane read one scan.
    let output = TsSource.extract(PATH, SOURCE.as_bytes(), FamilyMask::ALL);
    let call = output.call.as_ref().expect("call family");
    let extractor_rows: Vec<Value> = call
        .aux
        .specifiers
        .iter()
        .map(|specifier| {
            json!([
                specifier.kind.as_str(),
                specifier
                    .module
                    .map(|id| output.strings.lookup(id))
                    .unwrap_or_default(),
            ])
        })
        .collect();
    assert_eq!(extractor_rows, rows.iter().map(|row| json!([row[0], row[1]])).collect::<Vec<_>>());

    // `.kts` is Kotlin, which `ends_with(".ts")` cannot say, so the roster's
    // own first-match law is the only thing that may hand a path to a
    // `Rehome` arm.
    let arm = |path: &str| rehome_for(path).map(|arm| arm.name());
    let roster: Vec<Value> = [
        ("a/b.ts", "ts"),
        ("a/b.d.ts", "ts"),
        ("a/b.mjs", "ts"),
        ("a/b.tsx", "ts"),
        ("a/b.cjs", "ts"),
        ("a/b.pl", "prolog"),
        ("a/b.plt", "prolog"),
        ("a/b.kts", "kotlin"),
        ("a/b.kt", "kotlin"),
        ("a/ts", ""),
        ("a.ts/b", ""),
    ]
    .iter()
    .map(|(path, want)| {
        let got = arm(path).unwrap_or_default();
        assert_eq!(got, *want, "{path} rehome arm");
        json!([path, got])
    })
    .collect();

    // SABOTAGE RECEIPT: dropping `node_modules` from `SKIP_DIRS` adds
    // `node_modules/dep/index.js` to the corpus and the length claim below
    // fails.
    let root = temp_root("corpus-walk");
    for rel in [
        "a.ts",
        "ui/a.tsx",
        "m.mts",
        "c.cts",
        "legacy.js",
        "legacy.jsx",
        "esm.mjs",
        "cjs.cjs",
        "rule.pl",
        "rule.plt",
        "notes.md",
        "script.kts",
        "node_modules/dep/index.js",
        ".boop-worktrees/lane/x.ts",
        "target/debug/build.ts",
        "dist/a.js",
        ".git/hooks/x.ts",
    ] {
        write(&root, rel, "export const x = 1;\n");
    }
    let cx = MoveCx::open(&root).expect("walk the corpus");
    let corpus_files: Vec<Value> = cx.files().iter().map(|f| json!(f)).collect();
    assert_eq!(
        cx.files(),
        [
            "a.ts",
            "c.cts",
            "cjs.cjs",
            "dist/a.js",
            "esm.mjs",
            "legacy.js",
            "legacy.jsx",
            "m.mts",
            "notes.md",
            "rule.pl",
            "rule.plt",
            "script.kts",
            "ui/a.tsx",
        ]
    );
    // `dist` stays in the walk (a `dist/package.json` is still a manifest);
    // the TS arm drops it from its own parse.
    let ts_files: Vec<Value> = cx.files_of(&TsSource).iter().map(|f| json!(f)).collect();
    assert_eq!(
        cx.files_of(&TsSource),
        [
            "a.ts",
            "c.cts",
            "cjs.cjs",
            "dist/a.js",
            "esm.mjs",
            "legacy.js",
            "legacy.jsx",
            "m.mts",
            "ui/a.tsx",
        ]
    );
    assert_eq!(cx.files_of(&PrologSource), ["rule.pl", "rule.plt"]);
    std::fs::remove_dir_all(&root).ok();

    // A monorepo root reaches every package: the walk descends `packages/*`
    // and drops each package's own `node_modules`.
    let root = temp_root("corpus-monorepo");
    for rel in [
        "tsconfig.json",
        "packages/one/src/a.ts",
        "packages/two/src/b.ts",
        "packages/two/node_modules/dep/index.ts",
    ] {
        write(&root, rel, "export const x = 1;\n");
    }
    let cx = MoveCx::open(&root).expect("walk the corpus");
    let monorepo_files: Vec<Value> = cx.files_of(&TsSource).iter().map(|f| json!(f)).collect();
    assert_eq!(
        cx.files_of(&TsSource),
        ["packages/one/src/a.ts", "packages/two/src/b.ts"]
    );
    std::fs::remove_dir_all(&root).ok();

    fn nested_scan(source: &str) -> Vec<Value> {
        ts_specifiers("x.ts", source)
            .expect("parses")
            .iter()
            .map(|row| json!([row.kind.as_str(), row.module.as_str()]))
            .collect()
    }

    json!({
        "specifier_rows": rows,
        "nested_rows": nested_rows,
        "extractor_rows": extractor_rows,
        "rehome_roster": roster,
        "corpus_files": corpus_files,
        "corpus_ts_files": ts_files,
        "monorepo_ts_files": monorepo_files,
        "double_quote_row": [double_rows[0].quote.to_string(), text],
    })
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "sprefa-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("create temp root");
    root
}

fn write(root: &std::path::Path, rel: &str, body: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    std::fs::write(path, body).expect("write fixture file");
}
