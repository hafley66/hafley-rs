use serde_json::{json, Value};
use std::process::Command;

const DIR: &str = "tests/fixtures/ts_binding_legs";

/// One `--resolve` run over the binding-leg universe and one over the shadow
/// universe; the whole `(caller, callee, callee_path, origin)` edge tables and
/// the shadow universe's unresolved call rows, with every old assert running
/// as code BEFORE the snapshot freezes them.
pub fn evaluate(_case: &Value) -> Value {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let resolve = |names: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .arg("--resolve")
            .args(names.iter().map(|name| format!("{manifest}/{DIR}/{name}")))
            .output()
            .expect("extract binary runs");
        assert!(
            out.status.success(),
            "resolve failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).expect("utf8 wire")
    };

    let mut main_edges = Vec::new();
    let mut shadow_edges = Vec::new();
    let mut shadow_drops = Vec::new();
    let collect = |stdout: String, edges: &mut Vec<Value>, mut drops: Option<&mut Vec<Value>>| {
        for line in stdout.lines() {
            let Ok(row) = serde_json::from_str::<Value>(&line) else { continue };
            match row["record"].as_str() {
                Some("resolved_edge") => edges.push(json!([
                    row["caller_name"],
                    row["callee_name"],
                    row["callee_path"],
                    row["resolution_origin"],
                ])),
                Some("unresolved") if row["family"] == "call" => {
                    if let Some(drops) = drops.as_deref_mut() {
                        drops.push(json!([
                            row["detail"],
                            row["reason"],
                            row["span"]["start"],
                        ]));
                    }
                }
                _ => {}
            }
        }
    };
    collect(resolve(&["lib.ts", "use.ts"]), &mut main_edges, None);
    collect(resolve(&["free.ts", "shadow.ts"]), &mut shadow_edges, Some(&mut shadow_drops));

    // The three binding-typed receiver legs plus the cross-file ctor leg each
    // bind the declared member with origin `receiver`, never a corpus name
    // match; the explicit type argument leaves the generic call on the import
    // leg and never mints a project edge for the type argument.
    let bind = |caller: &str, callee: &str| {
        main_edges
            .iter()
            .find(|edge| edge[0] == *caller && edge[1] == *callee && edge[2].as_str().unwrap().ends_with("lib.ts"))
            .unwrap_or_else(|| panic!("{caller} -> {callee} missing among {main_edges:?}"))
            .clone()
    };
    for (caller, callee) in [("run", "m"), ("ctorCase", "bar"), ("ifaceCase", "project"), ("crossFileCtor", "bar")] {
        assert_eq!(bind(caller, callee)[3], "receiver", "{caller} -> {callee} leg");
    }
    let generic = bind("crossFileGeneric", "ifaceCase");
    assert_eq!(generic[3], "module_plane", "the import leg binds the generic fn");
    assert!(
        !main_edges
            .iter()
            .any(|edge| edge[0] == "crossFileGeneric" && edge[1] == "project"),
        "{main_edges:?}"
    );

    // C.6: the param `project` owns the name inside `run`, so the plain call
    // names the local, never the corpus-unique free fn in free.ts; the drop
    // row records the policy as `inferred`.
    let names_free = |caller: &str| {
        shadow_edges
            .iter()
            .any(|edge| edge[0] == *caller && edge[2].as_str().unwrap().ends_with("free.ts"))
    };
    // The (detail, reason) drops whose call span sits inside `caller`'s
    // declaration in shadow.ts (its `function` keyword to the closing brace at
    // column 0), so a drop from another caller cannot satisfy the assert.
    let drops_of = |caller: &str| {
        let src = std::fs::read_to_string(format!("{manifest}/{DIR}/shadow.ts"))
            .expect("fixture readable");
        let start = src
            .find(&format!("function {caller}("))
            .expect("caller declared in shadow.ts") as u64;
        let end = start + src[start as usize..].find("\n}").expect("closing brace") as u64;
        let mut hits: Vec<Value> = shadow_drops
            .iter()
            .filter(|drop| {
                let at = drop[2].as_u64().unwrap();
                (start..end).contains(&at)
            })
            .map(|drop| json!([drop[0], drop[1]]))
            .collect();
        hits.sort_by_key(|hit| serde_json::to_string(hit).expect("hit serializes"));
        hits
    };
    assert!(!names_free("run"), "{shadow_edges:?}");
    assert_eq!(drops_of("run"), vec![json!(["project", "inferred"])]);

    // D9: a callable `const` resolves to its same-file lexical definition.
    assert!(
        !shadow_edges
            .iter()
            .any(|edge| edge[0] == "constCase" && edge[1] == "project" && edge[2].as_str().unwrap().ends_with("free.ts")),
        "{shadow_edges:?}"
    );
    assert!(
        shadow_edges.iter().any(|edge| edge[0] == "constCase"
            && edge[1] == "project"
            && edge[2].as_str().unwrap().ends_with("shadow.ts")
            && edge[3] == "same_file"),
        "{shadow_edges:?}"
    );
    assert_eq!(drops_of("constCase"), Vec::<Value>::new());

    // A closure param owns the name inside the arrow body only.
    assert!(
        !shadow_edges
            .iter()
            .any(|edge| edge[0] == "closureCase" && edge[1] == "project" && edge[2].as_str().unwrap().ends_with("free.ts")),
        "{shadow_edges:?}"
    );
    assert_eq!(drops_of("closureCase"), vec![json!(["project", "inferred"])]);
    // A binding owns its name only after its initializer: in
    // `const project = project()` the call still denotes the outer fn.
    assert!(names_free("selfInitCase"));

    json!({
        "main_edges": main_edges,
        "shadow_edges": shadow_edges,
        "shadow_drops": shadow_drops,
    })
}
