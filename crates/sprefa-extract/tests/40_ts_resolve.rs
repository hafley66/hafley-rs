//! TS path resolution (arc 3), graded on `tests/fixtures/ts_move`. Every case
//! the issue names gets a row: relative-with-extension, extensionless, `index`,
//! the `.js` written for a `.ts`, a tsconfig `paths` alias, a package `exports`
//! entry, a re-export, and the two runtime forms.

use std::path::{Path, PathBuf};

use sprefa_extract::{respell, ts_specifiers, TsResolver};

/// The fixture, staged in a temp root with `vendor/` renamed to
/// `node_modules/`: this repository gitignores that name at any depth
/// (`.gitignore:107`), so the package cannot be committed under it.
fn stage(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("sprefa-ts-resolve-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_move");
    copy_tree(&source, &root);
    std::fs::rename(root.join("vendor"), root.join("node_modules")).expect("stage node_modules");
    root.canonicalize().expect("canonicalize staged root")
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read fixture dir") {
        let entry = entry.expect("fixture entry");
        let to = target.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy fixture file");
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("resolved inside root")
        .to_string_lossy()
        .replace('\\', "/")
}

/// SABOTAGE RECEIPT: dropping the `.js -> .ts` entry from `EXTENSION_ALIAS`
/// leaves `./b.js` unresolved; dropping `TsconfigDiscovery::Auto` leaves
/// `@app/b` unresolved; dropping `condition_names` leaves `pkg-exports` on its
/// `main` fallback, which does not exist, so all three rows change.
#[test]
fn every_specifier_in_the_fixture_resolves() {
    let root = stage("all");
    let from = root.join("src/index.ts");
    let source = std::fs::read_to_string(&from).expect("read fixture importer");
    let resolver = TsResolver::new(&root).expect("build resolver");

    let scanned = ts_specifiers("index.ts", &source).expect("fixture parses");
    let rows: Vec<(&str, String)> = scanned
        .iter()
        .map(|row| {
            let target = resolver
                .resolve(&from, &row.module)
                .unwrap_or_else(|| panic!("{} did not resolve", row.module));
            (row.kind.as_str(), relative(&root, &target))
        })
        .collect();

    assert_eq!(
        rows,
        [
            ("named", "src/b.ts".to_string()),
            ("named", "src/b.ts".to_string()),
            ("named", "src/dir/index.ts".to_string()),
            ("named", "src/b.ts".to_string()),
            ("named", "src/b.ts".to_string()),
            ("named", "node_modules/pkg-exports/lib/main.js".to_string()),
            ("reexport", "src/reexport.ts".to_string()),
            ("reexport", "src/dir/index.ts".to_string()),
            ("dynamic_import", "src/b.ts".to_string()),
            ("require", "src/b.ts".to_string()),
            ("require", "src/b.ts".to_string()),
        ]
    );

    std::fs::remove_dir_all(&root).ok();
}

/// The move-target gate: a package resolves, and is still not a file this root
/// may move. An alias that lands back inside the tree is.
#[test]
fn only_paths_inside_the_root_are_move_targets() {
    let root = stage("in-root");
    let from = root.join("src/index.ts");
    let resolver = TsResolver::new(&root).expect("build resolver");

    assert!(resolver.resolve(&from, "pkg-exports").is_some());
    assert_eq!(
        resolver
            .resolve_in_root(&from, "pkg-exports")
            .map(|path| relative(&root, &path)),
        Some("node_modules/pkg-exports/lib/main.js".to_string())
    );

    let outside = TsResolver::new(&root.join("src")).expect("build resolver");
    assert_eq!(outside.resolve_in_root(&from, "pkg-exports"), None);
    assert_eq!(
        outside
            .resolve_in_root(&from, "@app/b")
            .map(|path| relative(outside.root(), &path)),
        Some("b.ts".to_string())
    );

    std::fs::remove_dir_all(&root).ok();
}

/// A specifier naming nothing on disk resolves to nothing, and says so by
/// returning `None` rather than a path that does not exist.
#[test]
fn missing_and_builtin_specifiers_resolve_to_nothing() {
    let root = stage("missing");
    let from = root.join("src/index.ts");
    let resolver = TsResolver::new(&root).expect("build resolver");

    assert_eq!(resolver.resolve(&from, "./nope"), None);
    assert_eq!(resolver.resolve(&from, "not-installed"), None);
    assert_eq!(resolver.resolve(&from, "node:fs"), None);

    std::fs::remove_dir_all(&root).ok();
}

/// The re-aim keeps the way the original spelled itself: extension style,
/// directory form, and quote all survive the move.
#[test]
fn respell_keeps_the_written_style() {
    assert_eq!(respell("sub/b.ts", "./b.ts", '\''), "'./sub/b.ts'");
    assert_eq!(respell("sub/b.ts", "./b.js", '\''), "'./sub/b.js'");
    assert_eq!(respell("sub/b.ts", "./b", '"'), "\"./sub/b\"");
    assert_eq!(respell("../b.ts", "./b.ts", '\''), "'../b.ts'");
    assert_eq!(respell("sub/dir/index.ts", "./dir", '\''), "'./sub/dir'");
    assert_eq!(
        respell("sub/dir/index.ts", "./dir/index", '\''),
        "'./sub/dir/index'"
    );
    assert_eq!(respell("sub/b.mts", "./b.mjs", '\''), "'./sub/b.mjs'");
    assert_eq!(respell("sub/b.ts", "./b.mjs", '\''), "'./sub/b.ts'");
}

/// SABOTAGE RECEIPT: dropping the `./` prefix in `respell` turns `sub/b.ts`
/// into a bare package specifier and this test's resolve returns `None`.
#[test]
fn a_respelled_specifier_resolves_to_the_file_it_names() {
    let root = stage("respell");
    let from = root.join("src/index.ts");
    let resolver = TsResolver::new(&root).expect("build resolver");

    for (relative_path, original) in [
        ("dir/index.ts", "./dir"),
        ("b.ts", "./b.js"),
        ("b.ts", "./b"),
        ("reexport.ts", "./reexport"),
    ] {
        let text = respell(relative_path, original, '\'');
        let module = text.trim_matches('\'');
        let resolved = resolver
            .resolve(&from, module)
            .unwrap_or_else(|| panic!("{text} did not resolve"));
        assert_eq!(
            relative(&root, &resolved),
            format!("src/{relative_path}"),
            "{text}"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

/// One resolver serves a parallel prescan, which needs it to cross threads.
#[test]
fn the_resolver_crosses_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TsResolver>();
}

/// Workspace manifests resolve without node_modules links or built dist files.
/// A declaration map wins over paths; paths win over the tsconfig outDir -> rootDir map.
#[test]
fn workspace_exports_resolve_to_sources_in_rung_order() {
    let root = stage("workspace-exports");
    let package = root.join("packages/lib");
    std::fs::create_dir_all(package.join("src")).unwrap();
    std::fs::create_dir_all(package.join("dist")).unwrap();
    std::fs::write(
        package.join("package.json"),
        r#"{
      "name":"@fixture/lib",
      "exports":{
        ".":{"types":"./dist/index.d.ts","import":"./dist/index.js"},
        "./mapped":{"types":"./dist/mapped.d.ts"},
        "./aliased":{"types":"./dist/aliased.d.ts"},
        "./wild/*":{"types":"./dist/*.d.ts"},
        "./blocked":null
      }
    }"#,
    )
    .unwrap();
    for name in [
        "index",
        "actual",
        "mapped",
        "aliased",
        "path-target",
        "leaf",
    ] {
        std::fs::write(
            package.join(format!("src/{name}.ts")),
            "export const value = 1;\n",
        )
        .unwrap();
    }
    std::fs::write(
        package.join("tsconfig.json"),
        r#"{"compilerOptions":{"rootDir":"./src","outDir":"./dist"}}"#,
    )
    .unwrap();
    // No tsconfig declares where this package builds from: its export abstains.
    let bare = root.join("packages/bare");
    std::fs::create_dir_all(bare.join("src")).unwrap();
    std::fs::write(bare.join("package.json"), r#"{"name":"@fixture/bare","exports":{".":{"types":"./dist/index.d.ts"}}}"#).unwrap();
    std::fs::write(bare.join("src/index.ts"), "export const value = 1;\n").unwrap();
    std::fs::write(
        package.join("dist/mapped.d.ts"),
        "export declare const value: number;\n",
    )
    .unwrap();
    std::fs::write(
        package.join("dist/mapped.d.ts.map"),
        r#"{"version":3,"sourceRoot":"../src","sources":["actual.ts"],"mappings":""}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{
      "@fixture/lib/aliased":["packages/lib/src/path-target.ts"],
      "@fixture/lib/mapped":["packages/lib/src/path-target.ts"]
    }}}"#,
    )
    .unwrap();
    let resolver = TsResolver::new(&root).unwrap();
    let from = root.join("src/index.ts");
    let rows: Vec<_> = [
        "@fixture/lib",
        "@fixture/lib/mapped",
        "@fixture/lib/aliased",
        "@fixture/lib/wild/leaf",
        "@fixture/lib/blocked",
        "@fixture/bare",
    ]
    .into_iter()
    .map(|module| {
        (
            module,
            resolver
                .resolve_with_rung(&from, module)
                .map(|(path, rung)| (relative(&root, &path), rung)),
        )
    })
    .collect();
    assert_eq!(
        rows,
        vec![
            (
                "@fixture/lib",
                Some(("packages/lib/src/index.ts".to_string(), "out_dir_map"))
            ),
            (
                "@fixture/lib/mapped",
                Some(("packages/lib/src/actual.ts".to_string(), "declaration_map"))
            ),
            (
                "@fixture/lib/aliased",
                Some(("packages/lib/src/path-target.ts".to_string(), "tsconfig_paths"))
            ),
            (
                "@fixture/lib/wild/leaf",
                Some(("packages/lib/src/leaf.ts".to_string(), "out_dir_map"))
            ),
            ("@fixture/lib/blocked", None),
            ("@fixture/bare", None),
        ]
    );
    std::fs::remove_dir_all(root).unwrap();
}
