use std::path::{Path, PathBuf};
use std::process::Command;

fn copy_tree(source: &Path, dest: &Path) {
    std::fs::create_dir_all(dest).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let to = dest.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), to).unwrap();
        }
    }
}

fn fixture(kind: &str) -> (PathBuf, PathBuf) {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target"));
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = target.join(format!(
        "cleave-ts-oracle-{kind}-{}-{stamp}",
        std::process::id()
    ));
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cleave_ts_oracle")
        .join(kind);
    let root = base.join("repo");
    copy_tree(&source, &root);
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    let git = Command::new("git")
        .args(["init", "-q", "."])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        git.status.success(),
        "{}",
        String::from_utf8_lossy(&git.stderr)
    );
    (root.canonicalize().unwrap(), state)
}

fn cleave(kind: &str, item: &str, dest: &str) -> (PathBuf, String) {
    cleave_with(kind, item, dest, false)
}

fn cleave_with(kind: &str, item: &str, dest: &str, slow: bool) -> (PathBuf, String) {
    let (root, state) = fixture(kind);
    let mut command = Command::new(env!("CARGO_BIN_EXE_ryii"));
    command.args(["cleave", &format!("source.ts#{item}"), dest, "--commit"]);
    if slow {
        command.arg("--slow");
    }
    let output = command
        .arg("--root")
        .arg(&root)
        .arg("--state")
        .arg(&state)
        .current_dir(&root)
        .env("RUST_LOG", "error")
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{kind}: {log}");
    (root, log)
}

#[test]
fn slow_type_alias_uses_lsp_items_and_diagnostics() {
    let (root, _) = cleave_with("declaration", "Alias", "alias.ts", true);
    assert!(read(&root, "alias.ts").contains("export type Alias"));
}

#[test]
fn slow_preserves_default_namespace_and_type_imports() {
    let (root, _) = cleave_with("import_kind", "inspect", "dest.ts", true);
    let dest = read(&root, "dest.ts");
    assert!(dest.contains("import fs from \"node:fs\""));
    assert!(dest.contains("import * as path from \"node:path\""));
    assert!(dest.contains("import type { Node as TreeNode } from \"tree\""));
}

#[test]
fn slow_diagnostic_stops_before_writing() {
    let (root, state) = fixture("diagnostic");
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["cleave", "source.ts#value", "dest.ts", "--slow", "--commit"])
        .arg("--root")
        .arg(&root)
        .arg("--state")
        .arg(&state)
        .current_dir(&root)
        .env("RUST_LOG", "error")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cleave --slow diagnostic"), "{stderr}");
    assert_eq!(
        read(&root, "source.ts"),
        "export function value(): number { return 1; }\n"
    );
    assert_eq!(read(&root, "dest.ts"), "export const value = 2;\n");
}

#[test]
fn fast_stops_when_the_destination_declares_the_name() {
    let (root, state) = fixture("diagnostic");
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["cleave", "source.ts#value", "dest.ts", "--commit"])
        .arg("--root")
        .arg(&root)
        .arg("--state")
        .arg(&state)
        .current_dir(&root)
        .env("RUST_LOG", "error")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("dest.ts already declares value"), "{stderr}");
    assert_eq!(read(&root, "dest.ts"), "export const value = 2;\n");
}

fn read(root: &Path, file: &str) -> String {
    std::fs::read_to_string(root.join(file)).unwrap()
}

#[test]
fn interface_declaration_moves_with_type_imports() {
    let (root, _) = cleave("declaration", "Shape", "shape.ts");
    assert!(read(&root, "shape.ts").contains("export interface Shape"));
    assert!(read(&root, "source.ts").contains("import type { Shape } from"));
    assert!(read(&root, "caller.ts").contains("import type { Shape } from"));
}

#[test]
fn type_alias_is_a_moveable_declaration() {
    let (root, _) = cleave("declaration", "Alias", "alias.ts");
    assert!(read(&root, "alias.ts").contains("export type Alias"));
}

#[test]
fn source_use_exports_the_moved_function() {
    let (root, _) = cleave("export", "value", "dest.ts");
    assert!(read(&root, "dest.ts").contains("export function value"));
    assert!(read(&root, "source.ts").contains("import { value } from"));
}

#[test]
fn destination_keeps_default_namespace_alias_and_type_imports() {
    let (root, _) = cleave("import_kind", "inspect", "dest.ts");
    let dest = read(&root, "dest.ts");
    assert!(dest.contains("import fs from \"node:fs\""));
    assert!(dest.contains("import * as path from \"node:path\""));
    assert!(dest.contains("import type { Node as TreeNode } from \"tree\""));
}

#[test]
fn source_dependencies_are_imported_into_destination() {
    let (root, _) = cleave("dependencies", "run", "dest.ts");
    let dest = read(&root, "dest.ts");
    assert!(dest.contains("import { helper } from"), "{dest}");
    assert!(dest.contains("import type { Dependency } from"), "{dest}");
}

#[test]
fn multiline_destination_import_stays_complete() {
    let (root, _) = cleave("corrupt", "ExtractionResult", "dest.ts");
    let dest = read(&root, "dest.ts");
    assert!(dest.contains("} from './types';"), "{dest}");
    assert!(dest.contains("import type { Thing } from"), "{dest}");
    assert!(!dest.contains("import type { Thing } from \"./types\";\n} from"));
}

#[test]
fn source_discards_import_used_only_by_moved_item() {
    let (root, _) = cleave("unused", "work", "dest.ts");
    assert!(!read(&root, "source.ts").contains("import { magic }"));
    assert!(read(&root, "dest.ts").contains("import { magic }"));
}
