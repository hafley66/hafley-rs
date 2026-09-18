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

const DIR: &str = "tests/fixtures/ts/binding_legs";

fn resolve() -> String {
    let root = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .arg(format!("{root}/{DIR}/lib.ts"))
        .arg(format!("{root}/{DIR}/use.ts"))
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8 wire")
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