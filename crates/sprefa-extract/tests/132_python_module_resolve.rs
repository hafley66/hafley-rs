//! Python's resolve arms read `PyModuleIndex`
//! (`src/lang/python/_2_modules.rs`): the import binding leg (named, aliased,
//! and star through the `__all__` gate), for calls and type references both.
//!
//! Fixtures: `tests/fixtures/python_module_resolve` (kept OUT of
//! `tests/fixtures/python/`, which is the scip ratchet corpus).

use std::process::Command;

use serde_json::Value;

const FILES: &[&str] = &["main.py", "pkg/__init__.py", "pkg/mod.py"];

fn rows() -> Vec<serde_json::Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call,type".to_string(),
    ];
    args.extend(
        FILES
            .iter()
            .map(|name| format!("tests/fixtures/python_module_resolve/{name}")),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(&args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
        .collect()
}

type Call = (String, String, String, String);

/// (caller, callee, target file stem, origin) per `resolved_edge`.
fn calls() -> Vec<Call> {
    rows()
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or("").to_string();
            (
                text("caller_name"),
                text("callee_name"),
                text("callee_path")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string(),
                text("resolution_origin"),
            )
        })
        .collect()
}

fn has_call(calls: &[Call], callee: &str, target: &str, origin: &str) -> bool {
    calls
        .iter()
        .any(|(_, callee_name, target_file, origin_)| {
            callee_name == callee && target_file == target && origin_ == origin
        })
}

/// The import binding leg: a named import (`secret as sneaky`) and a star
/// import bound by `__all__` (`exported`) name the def in the target file.
/// `exported` rides the corpus leg (its def name IS its local name and the
/// name-match answers first there is nothing to prefer); the ALIASED binding
/// is the observable one: `sneaky` matches no def name anywhere, so only the
/// module plane can answer, and it names `secret`'s def.
#[test]
fn an_aliased_import_binds_through_the_module_plane() {
    let calls = calls();
    assert!(
        has_call(&calls, "secret", "mod.py", "module_plane"),
        "{calls:?}"
    );
}

/// An `__all__`-excluded name still resolves through the corpus leg (it is a
/// declared def), but only when no import binding of the file names it. The
/// fixture's bare `secret()` call rides `corpus_unique`; the ALIASED
/// `sneaky` call rides the plane because its own clause names the def.
#[test]
fn an_all_excluded_name_never_binds_through_the_module_plane() {
    let calls = calls();
    // The bare `secret()` site: corpus leg, never the plane.
    assert!(
        has_call(&calls, "secret", "mod.py", "corpus_unique"),
        "{calls:?}"
    );
    // Exactly one module_plane row for `secret`: the aliased import's.
    assert_eq!(
        calls
            .iter()
            .filter(|(_, callee, _, origin)| callee == "secret" && origin == "module_plane")
            .count(),
        1,
        "{calls:?}"
    );
}

/// A type reference through a named import (`widget: Widget`) rides the same
/// plane: the field edge stamps `module_plane` at the declaring file.
#[test]
fn a_type_reference_binds_through_the_module_plane() {
    let types: Vec<(String, String, String)> = rows()
        .iter()
        .filter(|row| row["record"] == "resolved_type_edge")
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or("").to_string();
            (text("owner_name"), text("target_name"), text("resolution_origin"))
        })
        .collect();
    assert!(
        types
            .iter()
            .any(|(owner, target, origin)| owner == "Panel"
                && target == "Widget"
                && origin == "module_plane"),
        "{types:?}"
    );
}
