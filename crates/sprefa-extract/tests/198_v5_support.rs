use serde_json::Value;
use sprefa_extract::{dispatch, flatten_jsonl, FamilyMask, RyiOutput};

pub(super) fn facts(path: &str, source: &[u8], call: bool) -> std::sync::Arc<RyiOutput> {
    dispatch(
        path,
        source,
        FamilyMask {
            df: true,
            call,
            types: false,
            cst: false,
            data: false,
        },
    )
    .unwrap()
}

pub(super) fn rows(path: &str, source: &str, call: bool) -> Vec<Value> {
    flatten_jsonl(&facts(path, source.as_bytes(), call))
        .into_iter()
        .map(|row| serde_json::from_str(&row).unwrap())
        .collect()
}

pub(super) fn snapshots() -> impl Drop {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"));
    settings.set_prepend_module_to_snapshot(false);
    settings.bind_to_scope()
}

#[cfg(feature = "cli")]
pub(super) fn run(arguments: &[&str], path: impl AsRef<std::ffi::OsStr>) -> std::process::Output {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(arguments)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
