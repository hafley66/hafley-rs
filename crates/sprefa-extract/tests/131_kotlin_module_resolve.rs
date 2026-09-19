//! Kotlin's resolve arms read `KtModuleIndex` (`src/lang/kotlin_modules.rs`):
//! the import leg (named, aliased, or wildcard), the same-package leg over a
//! package-keyed name index, and the same legs for `Resolve<TypeF>`.
//!
//! Fixtures: `tests/fixtures/kotlin_module_resolve` (kept OUT of
//! `tests/fixtures/kotlin/`, which is the scip ratchet corpus).

use std::process::Command;

use serde_json::Value;

const FILES: &[&str] = &[
    "app/Main.kt",
    "app/Helper.kt",
    "app/App.kt",
    "model/Widget.kt",
    "model/Gadget.kt",
    "model/Sibling.kt",
];

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

type Type = (String, String, String, String);

/// (owner, target, kind, origin) per `resolved_type_edge`.
fn type_edges() -> Vec<Type> {
    rows()
        .iter()
        .filter(|row| row["record"] == "resolved_type_edge")
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or("").to_string();
            (
                text("owner_name"),
                text("target_name"),
                text("kind"),
                text("resolution_origin"),
            )
        })
        .collect()
}
fn rows() -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call,type".to_string(),
    ];
    args.extend(
        FILES
            .iter()
            .map(|name| format!("tests/fixtures/kotlin_module_resolve/{name}")),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
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

fn has_call(
    calls: &[Call],
    caller: &str,
    callee: &str,
    target: &str,
    origin: &str,
) -> bool {
    calls.iter().any(|(c, callee_name, target_file, origin_)| {
        c == caller && callee_name == callee && target_file == target && origin_ == origin
    })
}

/// The import leg: a named import (`Widget`), an aliased import (`build`
/// binds makeWidget), and a wildcard (`makeWidget` again, `lone` through
/// `com.acme.model.*`) all stamp `module_plane` at the declaring file.
#[test]
fn imports_bind_through_the_module_plane() {
    let calls = calls();
    assert!(
        has_call(&calls, "main", "makeWidget", "Widget.kt", "module_plane"),
        "{calls:?}"
    );
    // The alias `build` names the same def as its path's last segment: two
    // call sites, both module_plane.
    assert_eq!(
        calls
            .iter()
            .filter(|(_, callee, target, origin)| callee == "makeWidget"
                && target == "Widget.kt"
                && origin == "module_plane")
            .count(),
        2,
        "{calls:?}"
    );
    assert!(
        has_call(&calls, "main", "lone", "Gadget.kt", "module_plane"),
        "{calls:?}"
    );
}

/// The same-package leg: a bare name declared only in another file of the
/// referring file's own package binds there.
#[test]
fn a_same_package_name_binds_through_the_module_plane() {
    let calls = calls();
    assert!(
        has_call(&calls, "main", "appHelper", "Helper.kt", "module_plane"),
        "{calls:?}"
    );
}

/// Two files of one package both declaring the name is ambiguous: the module
/// plane binds nothing and no other leg may guess, so no edge exists.
#[test]
fn an_ambiguous_same_package_name_binds_nothing() {
    let calls = calls();
    assert!(
        !calls.iter().any(|(_, callee, _, _)| callee == "dupName"),
        "{calls:?}"
    );
}

/// Lane K1: `spin` is reached as `Gadget.spin()` - a member call whose
/// receiver the receiver plane names - so it binds through the (T, m) owner
/// table at origin `receiver`, never through the corpus name-match.
#[test]
fn the_member_call_binds_through_the_receiver_plane() {
    let calls = calls();
    assert!(
        has_call(&calls, "main", "spin", "Gadget.kt", "receiver"),
        "{calls:?}"
    );
}

/// The type plane's field and impl candidates ride the same legs: a class in
/// Main.kt referencing imported types stamps `module_plane` at the declaring
/// files.
#[test]
fn type_candidates_bind_through_the_module_plane() {
    let types = type_edges();
    assert!(
        types
            .iter()
            .any(|(owner, target, kind, origin)| owner == "Panel"
                && target == "Widget"
                && kind == "field"
                && origin == "module_plane"),
        "{types:?}"
    );
    assert!(
        types
            .iter()
            .any(|(owner, target, kind, origin)| owner == "Panel"
                && target == "Gadget"
                && kind == "field"
                && origin == "module_plane"),
        "{types:?}"
    );
    assert!(
        types
            .iter()
            .any(|(owner, target, kind, origin)| owner == "Panel"
                && target == "Widget"
                && kind == "impl"
                && origin == "module_plane"),
        "{types:?}"
    );
}
