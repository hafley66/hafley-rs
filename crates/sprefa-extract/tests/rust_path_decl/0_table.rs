#![cfg(feature = "cli")]

use sprefa_extract::{Cleave, Rehome, Rename};
use sprefa_extract::rename_cx::{RenameCx, RenameRequest};
use sprefa_extract::lang::rust::RustSource;
use sprefa_extract::move_cx::MoveCx;
use std::path::Path;
use std::process::Command;

fn copy(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let dest = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}

#[test]
fn declaration_table_compiles_and_matches_text() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust_path_decl");
    let mut cases: Vec<_> = std::fs::read_dir(fixtures).unwrap().map(Result::unwrap).collect();
    cases.sort_by_key(|case| case.file_name());
    let mut failures = Vec::new();
    for case in cases {
        let scratch = tempfile::tempdir().unwrap();
        copy(&case.path(), scratch.path());
        let root = scratch.path().canonicalize().unwrap();
        let description = std::fs::read_to_string(root.join("case.txt")).unwrap();
        let row: Vec<_> = description.lines().collect();
        let [mode, declarer, source, dest] = row.as_slice() else { panic!("invalid row") };
        let cx = MoveCx::open_with_untracked(&root, true).unwrap();
        let mut edits = Vec::new();
        if *mode == "rename" {
            let cx = RenameCx::open_with_untracked(&root, true).unwrap();
            let request = RenameRequest { anchor: declarer.to_string(), old: "round".into(), new: dest.to_string(), at: None };
            for reference in RustSource.symbol_refs(&cx, &request).unwrap() {
                if let Some(replacement) = RustSource.respell_symbol(&cx, &request, &reference) {
                    edits.push((replacement.file, replacement.span, replacement.text));
                }
            }
        } else if *mode == "cleave" {
            if let Some((parent, edit)) = RustSource.declare_new_file(&cx, source, dest, Some(declarer)) {
                edits.push((parent, edit.span, edit.text));
            }
            let path = root.join(dest);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "pub fn value() -> u32 { 7 }\n").unwrap();
        } else {
            let cx = cx.with_batch([(source.to_string(), dest.to_string())].into(), false);
            for reference in RustSource.import_refs(&cx) {
                if let Some(replacement) = RustSource.respell(&cx, &reference) {
                    edits.push((reference.importer, reference.literal, replacement.text));
                }
            }
        }
        edits.sort_by(|a, b| (&b.0, b.1.start).cmp(&(&a.0, a.1.start)));
        for (file, span, replacement) in edits {
            let path = root.join(file);
            let mut text = std::fs::read_to_string(&path).unwrap();
            text.replace_range(span.start as usize..span.end() as usize, &replacement);
            std::fs::write(path, text).unwrap();
        }
        if *mode == "move" {
            let path = root.join(dest);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::rename(root.join(source), path).unwrap();
        }
        let actual = std::fs::read_to_string(root.join(declarer)).unwrap();
        let expected = std::fs::read_to_string(root.join("expected.rs.txt")).unwrap();
        let check = Command::new("cargo").args(["check", "--offline"])
            .env("KACHE_DISABLED", "1").env("CARGO_TARGET_DIR", root.join("target"))
            .current_dir(&root).output().unwrap();
        if actual != expected || !check.status.success() {
            failures.push(format!("{}: expected {expected:?}, actual {actual:?}; cargo {}\n{}",
                case.file_name().to_string_lossy(), check.status, String::from_utf8_lossy(&check.stderr)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
