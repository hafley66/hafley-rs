//! The kotlin receiver legs (lane K1): every `recv.m()` navigation call binds
//! through the corpus (T, m) owner table at origin `receiver`, the one-hop
//! `a.f` chain upgrades cross-file, `this` inside a class names the class,
//! and a receiver the plane cannot type (a ctor-return through a fn's
//! declared return type, a scope-shadowed param) declines with reason
//! `inferred`. Fixtures live under `tests/fixtures/kotlin_receivers/`,
//! outside `tests/fixtures/kotlin/` (the scip ratchet corpus).

use std::process::Command;

use serde_json::Value;

const FILES: &[&str] = &[
    "tests/fixtures/kotlin_receivers/lib.kt",
    "tests/fixtures/kotlin_receivers/use.kt",
];

fn rows() -> Vec<Value> {
    let mut args: Vec<String> = vec!["--resolve".to_string(), "--family".to_string(), "call".to_string()];
    args.extend(FILES.iter().map(|name| name.to_string()));
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

/// (caller, callee, target file stem, callee start, origin).
fn edges() -> Vec<(String, String, String, u32, String)> {
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
                row["callee_start"].as_u64().unwrap_or(0) as u32,
                text("resolution_origin"),
            )
        })
        .collect()
}

/// (path stem, start, end, reason, detail) per dropped site.
fn drops() -> Vec<(String, u32, u32, String, String)> {
    rows()
        .iter()
        .filter(|row| row["record"] == "unresolved")
        .map(|row| {
            (
                row["path"].as_str().unwrap_or("").rsplit('/').next().unwrap_or("").to_string(),
                row["span"]["start"].as_u64().unwrap_or(0) as u32,
                row["span"]["end"].as_u64().unwrap_or(0) as u32,
                row["reason"].as_str().unwrap_or("").to_string(),
                row["detail"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

/// The byte offset of a def's `fun` keyword in its fixture, hand-derived from
/// the written file (the def node's start byte is the resolve target's
/// `callee_start`).
fn def_start(stem: &str, marker: &str) -> u32 {
    let path = format!("tests/fixtures/kotlin_receivers/{stem}");
    let content = std::fs::read_to_string(&path).expect("fixture reads");
    let at = content.find(marker).unwrap_or_else(|| panic!("{marker:?} in {path}"));
    at as u32
}

/// The `w.run()` inside the named function: the `run` of the FIRST
/// `w.run()` whose enclosing block the fixture places after `marker`.
fn nav_site(stem: &str, marker: &str) -> (u32, u32) {
    let content = std::fs::read_to_string(format!("tests/fixtures/kotlin_receivers/{stem}"))
        .expect("fixture reads");
    let block = &content[content.find(marker).expect("marker")..];
    let at = block.find("w.run()").expect("w.run() after marker") + block.as_ptr() as usize
        - content.as_ptr() as usize;
    (at as u32, at as u32 + "w.run".len() as u32)
}

#[test]
fn typed_param_and_ctor_and_field_and_bound_and_object_legs_bind_at_receiver() {
    let widget_run = def_start("lib.kt", "fun run(): Int = id");
    let gadget_spin = def_start("lib.kt", "fun spin(): Int = 1");
    let proj_project = def_start("lib.kt", "fun project(): Int");
    let edges = edges();
    for caller in ["paramLeg", "ctorLeg", "fieldLeg"] {
        assert!(
            edges
                .iter()
                .any(|(c, callee, target, start, origin)| c == caller
                    && callee == "run"
                    && target == "lib.kt"
                    && *start == widget_run
                    && origin == "receiver"),
            "{caller} run must bind Widget.run at {widget_run}, origin receiver: {edges:?}"
        );
    }
    assert!(
        edges
            .iter()
            .any(|(c, callee, target, start, origin)| c == "boundLeg"
                && callee == "project"
                && target == "lib.kt"
                && *start == proj_project
                && origin == "receiver"),
        "{edges:?}"
    );
    assert!(
        edges
            .iter()
            .any(|(c, callee, target, start, origin)| c == "objectLeg"
                && callee == "spin"
                && target == "lib.kt"
                && *start == gadget_spin
                && origin == "receiver"),
        "{edges:?}"
    );
}

#[test]
fn the_receiver_leg_never_lands_on_the_decoy_or_the_inner_member() {
    let widget_run = def_start("lib.kt", "fun run(): Int = id");
    let decoy_run = def_start("lib.kt", "fun run(): Int = 2");
    let edges = edges();
    assert!(
        !edges
            .iter()
            .any(|(_, callee, _, start, _)| callee == "run" && *start == decoy_run),
        "Decoy.run must never answer a Widget receiver: {edges:?}"
    );
    let inner_run = def_start("use.kt", "fun run() = 0");
    assert!(
        edges
            .iter()
            .any(|(c, callee, target, start, origin)| c == "self"
                && callee == "run"
                && target == "use.kt"
                && *start == inner_run
                && origin == "receiver"),
        "implicit this binds Inner.run, not a corpus guess: {edges:?}"
    );
    assert_eq!(
        widget_run != inner_run,
        true,
        "the two member defs must sit at different spans"
    );
}

#[test]
fn untyped_receiver_legs_decline_with_reason_inferred() {
    let return_site = nav_site("use.kt", "fun returnLeg()");
    let shadow_site: (u32, u32) = {
        let content = std::fs::read_to_string("tests/fixtures/kotlin_receivers/use.kt").unwrap();
        let block = &content[content.find("fun shadow").unwrap()..];
        let at = block.find("= run()").unwrap() + "= ".len();
        let abs = block.as_ptr() as usize - content.as_ptr() as usize + at;
        (abs as u32, abs as u32 + 3)
    };
    let drops = drops();
    assert!(
        drops
            .iter()
            .any(|(path, start, end, reason, detail)| path == "use.kt"
                && *start == return_site.0
                && *end == return_site.1
                && reason == "inferred"
                && detail == "run"),
        "the ctor-return leg declines inferred in K1: {drops:?}"
    );
    assert!(
        drops
            .iter()
            .any(|(path, start, end, reason, detail)| path == "use.kt"
                && *start == shadow_site.0
                && *end == shadow_site.1
                && reason == "inferred"
                && detail == "run"),
        "the scope-shadowed param run() declines inferred: {drops:?}"
    );
    assert!(
        !edges()
            .iter()
            .any(|(c, callee, _, _, _)| (c == "returnLeg" || c == "shadow") && callee == "run"),
        "no run edge may exist for the two declining callers"
    );
}
