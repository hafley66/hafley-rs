//! `--entry`: the files an entry reaches over `resolved_import`, per language.

use std::path::PathBuf;

use sprefa_extract::reach_files;

fn files(dir: &str) -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    let mut pending = vec![root.join(dir)];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(std::fs::read_dir(&path).unwrap().map(|entry| entry.unwrap().path()));
        } else if matches!(path.extension().and_then(|ext| ext.to_str()), Some("rs" | "ts")) {
            out.push(path.strip_prefix(&root).unwrap().to_path_buf());
        }
    }
    out.sort();
    out
}

fn reach(dir: &str, entry: &[&str], depth: Option<u32>) -> Vec<String> {
    std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).unwrap();
    let universe = files(dir);
    let entry: Vec<PathBuf> = entry.iter().map(|path| PathBuf::from(dir).join(path)).collect();
    reach_files(std::path::Path::new(dir), &universe, &entry, depth)
        .unwrap()
        .into_iter()
        .map(|path| path.strip_prefix(dir).unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn rust_entries_follow_mod_path_attr_and_own_crate_paths() {
    let dir = "tests/fixtures/reach_rust";
    assert_eq!(
        (
            reach(dir, &["src/lib.rs"], None),
            reach(dir, &["src/main.rs"], None),
            reach(dir, &["src/bin/tool.rs"], None),
            reach(dir, &["src/lib.rs"], Some(1)),
        ),
        (
            vec!["gen/made.rs", "src/a/inner.rs", "src/a.rs", "src/b/mod.rs", "src/lib.rs"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>(),
            vec!["gen/made.rs", "src/a/inner.rs", "src/a.rs", "src/b/mod.rs", "src/lib.rs", "src/main.rs"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>(),
            vec!["src/bin/helper.rs", "src/bin/tool.rs"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>(),
            vec!["gen/made.rs", "src/a.rs", "src/b/mod.rs", "src/lib.rs"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>(),
        )
    );
}

#[test]
fn soopy_lib_reaches_every_lib_file() {
    let dir = "tests/fixtures/ratchet_soopy";
    let everything: Vec<String> = files(dir)
        .into_iter()
        .map(|path| path.strip_prefix(dir).unwrap().to_string_lossy().into_owned())
        .filter(|path| path != "src/main.rs")
        .collect();
    assert_eq!(reach(dir, &["src/lib.rs"], None), everything);
}

#[test]
fn ts_entry_follows_every_specifier_form() {
    let dir = "tests/fixtures/reach_ts";
    assert_eq!(
        reach(dir, &["index.ts"], None),
        [
            "effect.ts", "index.ts", "lazy.ts", "legacy.ts", "lib/deep.ts", "named.ts", "relay.ts",
            "star.ts",
        ]
        .map(String::from)
    );
}
