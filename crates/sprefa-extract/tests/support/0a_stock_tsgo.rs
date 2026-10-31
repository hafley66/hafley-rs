//! Stock executable selection and portable paths in complete-stream snapshots.
use serde_json::Value;

pub fn executable() -> String {
    sprefa_extract::lang::ts_lib::typescript_executable(None)
        .expect("npm ci in ts7 installs the stock native compiler")
        .to_string_lossy()
        .into_owned()
}

pub fn normalize(value: Value) -> Value {
    let mut serialized = serde_json::to_string(&value).unwrap();
    if let Some(executable) = sprefa_extract::lang::ts_lib::typescript_executable(None) {
        serialized = serialized.replace(executable.parent().unwrap().to_str().unwrap(), "$TS_LIB");
    }
    serialized = serialized.replace(env!("CARGO_MANIFEST_DIR"), "$CRATE");
    serde_json::from_str(&serialized).unwrap()
}

/// Test-only: a scip-typescript 0.4.0 index of `root` (PATH first, then the pinned
/// npx form), for fixtures that check SCIP-verify against a TypeScript index.
/// ryi builds no TypeScript index itself. A root without tsconfig.json is indexed
/// from a copy: `--infer-tsconfig` writes one, and fixtures are committed.
pub fn scip_typescript(root: &std::path::Path) -> std::path::PathBuf {
    use std::process::Command;
    let staging = tempfile::tempdir().unwrap().keep();
    let work = if root.join("tsconfig.json").is_file() {
        root.to_path_buf()
    } else {
        copy_tree(root, &staging.join("src"));
        staging.join("src")
    };
    let out = staging.join("index.scip");
    let on_path = Command::new("scip-typescript")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    let mut command = if on_path {
        Command::new("scip-typescript")
    } else {
        let mut npx = Command::new("npx");
        npx.args(["-y", "@sourcegraph/scip-typescript@0.4.0"]);
        npx
    };
    let status = command
        .args(["index", "--infer-tsconfig", "--output"])
        .arg(&out)
        .current_dir(&work)
        .status()
        .expect("scip-typescript runs");
    assert!(status.success(), "scip-typescript index {}", work.display());
    out
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
