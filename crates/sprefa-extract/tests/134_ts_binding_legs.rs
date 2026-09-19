//! The three binding-typed receiver legs, ts: a field read whose type is
//! declared (`this.f.m()` on `class S { f: T }`), a constructor-return
//! binding (`const x = makeFoo(); x.bar()` where `function makeFoo(): Foo`),
//! and a generic bound (`function f<P extends Proj>(p: P) { p.project() }`
//! where the type-parameter constraint names the interface). Each binds the
//! declared member with origin `receiver`, never a corpus name match.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.

use std::process::Command;

const DIR: &str = "tests/fixtures/ts_binding_legs";

fn resolve_files(names: &[&str]) -> String {
    let root = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("--resolve")
        .args(names.iter().map(|name| format!("{root}/{DIR}/{name}")))
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8 wire")
}

fn resolve() -> String {
    resolve_files(&["lib.ts", "use.ts"])
}

/// `(caller_name, callee_name, callee_path, origin)` per resolved edge.
fn resolved_edges() -> Vec<(String, String, String, String)> {
    resolve()
        .lines()
        .filter_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).ok()?;
            (row["record"] == "resolved_edge").then(|| {
                (
                    row["caller_name"].as_str().unwrap_or("").to_string(),
                    row["callee_name"].as_str().unwrap_or("").to_string(),
                    row["callee_path"].as_str().unwrap_or("").to_string(),
                    row["resolution_origin"].as_str().unwrap_or("").to_string(),
                )
            })
        })
        .collect()
}

fn bind<'a>(
    edges: &'a [(String, String, String, String)],
    caller: &str,
    callee: &str,
) -> Option<&'a (String, String, String, String)> {
    edges
        .iter()
        .find(|(c, m, path, _)| c == caller && m == callee && path.ends_with("lib.ts"))
}

/// `this.f.m()`: the field's declared type T carries m, not a name match.
#[test]
fn a_field_read_binds_the_member_on_the_field_type() {
    let edges = resolved_edges();
    let edge = bind(&edges, "run", "m").expect("run -> m");
    assert_eq!(edge.3, "receiver", "field leg must bind through the receiver");
}

/// `const x = makeFoo(); x.bar()`: the declared return type Foo carries bar.
#[test]
fn a_constructor_return_binds_the_member_on_the_result_type() {
    let edges = resolved_edges();
    let edge = bind(&edges, "ctorCase", "bar").expect("ctorCase -> bar");
    assert_eq!(edge.3, "receiver", "ctor-return leg must bind through the receiver");
}

/// `function f<P extends Proj>(p: P)`: the constraint names the interface the
/// member binds on.
#[test]
fn a_generic_bound_binds_the_member_on_the_constraint_interface() {
    let edges = resolved_edges();
    let edge = bind(&edges, "ifaceCase", "project").expect("ifaceCase -> project");
    assert_eq!(edge.3, "receiver", "generic-bound leg must bind through the receiver");
}

/// The ctor-return leg fires cross-file through an import.
#[test]
fn the_ctor_return_leg_fires_cross_file() {
    let edges = resolved_edges();
    let edge = bind(&edges, "crossFileCtor", "bar").expect("crossFileCtor -> bar");
    assert_eq!(edge.3, "receiver", "cross-file ctor leg must bind through the receiver");
}

/// `ifaceCase<Impl>(new Impl())` in use.ts: the explicit type argument leaves
/// the call bound to the imported generic fn, and `Impl.project` is never a
/// callee of the caller (the type argument is not a receiver).
#[test]
fn a_generic_call_binds_the_imported_fn_across_files() {
    let edges = resolved_edges();
    let edge = bind(&edges, "crossFileGeneric", "ifaceCase").expect("crossFileGeneric -> ifaceCase");
    assert_eq!(edge.3, "module_plane", "the import leg binds the generic fn");
    assert!(
        !edges
            .iter()
            .any(|(caller, callee, _, _)| caller == "crossFileGeneric" && callee == "project"),
        "{edges:?}"
    );
}

const SHADOW: [&str; 2] = ["free.ts", "shadow.ts"];

/// One resolve run over the shadow universe: `(caller, callee, callee_path,
/// origin)` edges plus `(detail, reason, span start)` unresolved call rows.
fn shadow_run() -> (
    Vec<(String, String, String, String)>,
    Vec<(String, String, u64)>,
) {
    let (mut edges, mut drops) = (Vec::new(), Vec::new());
    for line in resolve_files(&SHADOW).lines() {
        let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match row["record"].as_str() {
            Some("resolved_edge") => edges.push((
                row["caller_name"].as_str().unwrap_or("").to_string(),
                row["callee_name"].as_str().unwrap_or("").to_string(),
                row["callee_path"].as_str().unwrap_or("").to_string(),
                row["resolution_origin"].as_str().unwrap_or("").to_string(),
            )),
            Some("unresolved") if row["family"] == "call" => drops.push((
                row["detail"].as_str().unwrap_or("").to_string(),
                row["reason"].as_str().unwrap_or("").to_string(),
                row["span"]["start"].as_u64().expect("drop span start"),
            )),
            _ => {}
        }
    }
    (edges, drops)
}

/// The `(detail, reason)` drops whose call span sits inside `caller`'s
/// declaration in shadow.ts (its `function` keyword to the closing brace at
/// column 0), so a drop from another caller cannot satisfy the assert.
fn drops_of(drops: &[(String, String, u64)], caller: &str) -> Vec<(String, String)> {
    let src = std::fs::read_to_string(format!("{}/{DIR}/shadow.ts", env!("CARGO_MANIFEST_DIR")))
        .expect("fixture readable");
    let start = src
        .find(&format!("function {caller}("))
        .expect("caller declared in shadow.ts") as u64;
    let end = start + src[start as usize..].find("\n}").expect("closing brace") as u64;
    drops
        .iter()
        .filter(|(_, _, at)| (start..end).contains(at))
        .map(|(detail, reason, _)| (detail.clone(), reason.clone()))
        .collect()
}

fn names_free_project(edges: &[(String, String, String, String)]) -> bool {
    edges
        .iter()
        .any(|(_, _, path, _)| path.ends_with("free.ts"))
}

/// C.6: the param `project` owns the name inside `run`, so the plain call
/// names the local, never the corpus-unique free fn in free.ts; the drop row
/// records the policy as `inferred`.
#[test]
fn a_param_shadow_kills_the_name_match() {
    let (edges, drops) = shadow_run();
    assert!(!names_free_project(&edges), "{edges:?}");
    assert_eq!(
        drops_of(&drops, "run"),
        vec![("project".to_string(), "inferred".to_string())],
        "{drops:?}"
    );
}

/// A `const` binding owns its name for the rest of its callable.
#[test]
fn a_const_binding_shadow_kills_the_name_match() {
    let (edges, drops) = shadow_run();
    assert!(
        !edges
            .iter()
            .any(|(caller, callee, path, _)| caller == "constCase"
                && callee == "project"
                && path.ends_with("free.ts")),
        "{edges:?}"
    );
    assert_eq!(
        drops_of(&drops, "constCase"),
        vec![("project".to_string(), "inferred".to_string())],
        "{drops:?}"
    );
}

/// A closure param owns the name inside the arrow body only.
#[test]
fn an_arrow_param_shadow_kills_the_name_match() {
    let (edges, drops) = shadow_run();
    assert!(
        !edges
            .iter()
            .any(|(caller, callee, path, _)| caller == "closureCase"
                && callee == "project"
                && path.ends_with("free.ts")),
        "{edges:?}"
    );
    assert_eq!(
        drops_of(&drops, "closureCase"),
        vec![("project".to_string(), "inferred".to_string())],
        "{drops:?}"
    );
}