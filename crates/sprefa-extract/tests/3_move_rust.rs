use std::path::Path;
use std::process::Command;

#[test]
fn whole_output() {
    crate::fixture_runner::run("move_rust_outputs", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git stdout is UTF-8")
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|error| panic!("read {rel}: {error}"))
}

/// The verb run against this crate's own tree, judged by rustc, not by an
/// assertion. MEASURED 2026-08-26: 20.2 s, over the 10-second cap, so it runs by
/// hand: `cargo test --features cli --test 3_move_rust -- --ignored`.
#[test]
#[ignore]
fn moving_this_crates_own_module_leaves_it_compiling() {
    let base = std::env::temp_dir().join(format!("extract_move_rust_self_{}", std::process::id()));
    let root = base.join("sprefa-extract");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    copy_crate(source, &root);
    crate::fixture_runner::re_aim_path_deps(source, &root.join("Cargo.toml"));
    git(&root, &["init", "-q", "."]);
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "-c",
            "user.email=extract@move.test",
            "-c",
            "user.name=extract-move",
            "commit",
            "-qm",
            "self move oracle",
        ],
    );
    let root = root.canonicalize().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("move")
        .arg(root.join("src/edit/ts_rehome.rs"))
        .arg(root.join("src/edit/ts/rehome.rs"))
        .arg("--root")
        .arg(&root)
        .arg("--state")
        .arg(&state)
        .arg("--commit")
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "extract move exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        read(&root, "src/edit.rs").contains("#[path = \"ts/rehome.rs\"] pub mod ts_rehome;"),
        "the roster's decl re-aims:\n{}",
        read(&root, "src/edit.rs")
    );

    let check = Command::new("cargo")
        .args(["check", "--features", "cli"])
        .current_dir(&root)
        .output()
        .expect("cargo runs");
    assert!(
        check.status.success(),
        "cargo check on the moved tree: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// The crate's sources, minus the build output and the git store the copy mints
/// for itself.
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
