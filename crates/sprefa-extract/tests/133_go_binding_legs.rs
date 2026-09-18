//! The three binding-typed receiver legs, go: a field read whose type is
//! declared (`s.f.M()` on `type S struct{ f T }`), a constructor-return
//! binding (`x := NewFoo(); x.Bar()` where `func NewFoo() *Foo` declares its
//! result), and an interface-typed parameter (`func f(p Proj)` where `Proj`
//! is a corpus interface). Each member name is shared with a decoy type, so a
//! bare name match is ambiguous: a receiver-typed leg is the only answer that
//! binds, and the origin must be `receiver`.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.

use std::process::Command;

const DIR: &str = "tests/fixtures/go_binding_legs";

fn resolve() -> String {
    let root = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .arg(format!("{root}/{DIR}/go.mod"))
        .arg(format!("{root}/{DIR}/lib.go"))
        .arg(format!("{root}/{DIR}/callers.go"))
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
        .find(|(c, m, path, _)| c == caller && m == callee && path.ends_with("lib.go"))
}

/// `s.f.M()`: the field's declared type T carries Method, not the decoy.
#[test]
fn a_field_read_binds_the_member_on_the_field_type() {
    let edges = resolved_edges();
    let edge = bind(&edges, "fieldCase", "Method").expect("fieldCase -> Method");
    assert_eq!(edge.3, "receiver", "field leg must bind through the receiver");
}

/// `x := NewFoo(); x.Bar()`: the declared result type *Foo carries Bar.
#[test]
fn a_constructor_return_binds_the_member_on_the_result_type() {
    let edges = resolved_edges();
    let edge = bind(&edges, "ctorCase", "Bar").expect("ctorCase -> Bar");
    assert_eq!(edge.3, "receiver", "ctor-return leg must bind through the receiver");
}

/// `func f(p Proj) { p.Project() }`: the interface parameter binds the
/// interface's own method, not an implementer or a name twin.
#[test]
fn an_interface_parameter_binds_the_interface_method() {
    let edges = resolved_edges();
    let edge = bind(&edges, "ifaceCase", "Project").expect("ifaceCase -> Project");
    assert_eq!(edge.3, "receiver", "interface leg must bind through the receiver");
}

/// The same three legs fire cross-file: same package directory, no import.
#[test]
fn the_legs_fire_cross_file_without_an_import() {
    let edges = resolved_edges();
    assert!(
        bind(&edges, "callField", "Method").is_some(),
        "cross-file field leg must bind to lib.go: {edges:?}"
    );
    assert_eq!(
        bind(&edges, "callCtor", "Bar").expect("callCtor -> Bar").3,
        "receiver",
        "cross-file ctor-return leg must bind through the receiver"
    );
    assert_eq!(
        bind(&edges, "callIface", "Project").expect("callIface -> Project").3,
        "receiver",
        "cross-file interface leg must bind through the receiver"
    );
}