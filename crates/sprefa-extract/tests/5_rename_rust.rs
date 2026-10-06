use std::path::Path;
use std::process::Command;

#[test]
fn whole_output() {
    crate::fixture_runner::run("rename_rust", |case| {
        crate::fixture_runner::commands(case, crate::fixture_runner::editing_api)
    });
}

fn scratch(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("extract_rename_rust_{label}_"))
        .tempdir()
        .expect("create scratch dir")
}

fn run_rename(
    root: &Path,
    state: &Path,
    target: &str,
    new: &str,
    extra: &[&str],
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(root)
        .arg("rename")
        .arg(target)
        .arg(new)
        .arg("--root")
        .arg(root)
        .arg("--state")
        .arg(state)
        .args(extra)
        .output()
        .expect("extract binary runs")
}

/// The verb run against this crate's own tree, judged by rustc, not by an
/// assertion. MEASURED 2026-08-27: 25.2 s, over the 10-second cap, so it runs by
/// hand: `cargo test --features cli --test all -- t_5_rename_rust --ignored`.
/// @comment-ok: fail-first/measured receipt, repo law keeps these on the test
#[test]
#[ignore]
fn self_rename_is_judged_by_rustc() {
    let scratch = scratch("self");
    let base = scratch.path().to_path_buf();
    let root = base.join("sprefa-extract");
    let state = base.join("state");
    std::fs::create_dir_all(&state).expect("create state dir");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    copy_crate(source, &root);
    re_aim_path_deps(source, &root.join("Cargo.toml"));
    let root = root.canonicalize().expect("canonicalize crate copy");

    let output = run_rename(
        &root,
        &state,
        "src/edit/_1_rename_cx.rs#RenameCx",
        "SymbolCx",
        &["--commit"],
    );
    assert!(
        output.status.success(),
        "extract rename exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let lib = std::fs::read_to_string(root.join("src/lib.rs")).expect("read the copy's lib.rs");
    assert!(
        lib.contains("pub use rename_cx::{SymbolCx, RenameRequest};"),
        "the re-export moved:\n{lib}"
    );

    let check = Command::new("cargo")
        .args(["check", "--features", "cli", "--offline"])
        .current_dir(&root)
        .output()
        .expect("cargo runs");
    assert!(
        check.status.success(),
        "cargo check on the renamed tree: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

/// The crate's sources, minus the build output and the git store.
fn copy_crate(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read crate dir") {
        let entry = entry.expect("crate entry");
        let name = entry.file_name();
        if matches!(name.to_string_lossy().as_ref(), ".git" | "target") {
            continue;
        }
        let to = target.join(&name);
        if entry.file_type().expect("file type").is_dir() {
            copy_crate(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy crate file");
        }
    }
}

/// Resolve the manifest's sibling dependencies before moving its directory.
fn re_aim_path_deps(source: &Path, manifest: &Path) {
    let text = std::fs::read_to_string(manifest).expect("read manifest");
    let mut out = text.clone();
    for line in text.lines() {
        let Some((_, tail)) = line.split_once("path = \"") else { continue };
        let Some((rel, _)) = tail.split_once('"') else { continue };
        if rel.starts_with("../") {
            let absolute = source.join(rel).canonicalize().expect("sibling dependency");
            out = out.replace(rel, &absolute.to_string_lossy());
        }
    }
    std::fs::write(manifest, out).expect("write manifest");
}
