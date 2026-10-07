use serde_json::{json, Value};

use hafley_scm::lang::rust::expand_file;
use sprefa_extract::{FamilyMask, RustSource, Source};

const MBE_DIR: &str = "tests/fixtures/rust_findings/mbe";

/// One `expand_file` + one `RustSource.extract` per mbe fixture; the node/site
/// counts, budget flags, macro-site rows and span mappings land in the tables
/// and every old assert runs as code BEFORE the snapshot freezes them.
pub fn evaluate(_case: &Value) -> Value {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let read = |name: &str| {
        std::fs::read_to_string(format!("{manifest}/{MBE_DIR}/{name}")).expect(name)
    };
    let mut files = Vec::new();
    for name in [
        "f1_local_call_in_body.rs",
        "f2_cross_file.rs",
        "f3_nested.rs",
        "f4_builtins.rs",
        "f5_derive.rs",
        "f6_attr_proc_macro.rs",
        "f7_mints_fn.rs",
        "f8_include.rs",
        "f9_recursive.rs",
    ] {
        let src = read(name);
        let expanded = expand_file(&src);
        let budget_hit = expanded.as_ref().map(|expanded| expanded.budget_hit);
        let output = RustSource.extract("f.rs", src.as_bytes(), FamilyMask::ALL);
        let call = output.call.as_ref().expect("syn parse must succeed");
        let (nodes, sites) = (call.nodes.len(), call.aux.sites.len());
        let macro_sites: Vec<Value> = call
            .aux
            .macro_sites
            .iter()
            .map(|site| {
                json!({
                    "macro_name": output.strings.lookup(site.macro_name),
                    "source": format!("{:?}", site.source),
                    "span_start": site.span.start,
                    "span_end": site.span.end(),
                    "covers_invocation": src[site.span.start as usize..site.span.end() as usize].contains("mkfn"),
                })
            })
            .collect();
        files.push(json!({
            "file": name,
            "expand_file": expanded.is_some(),
            "budget_hit": budget_hit,
            "nodes": nodes,
            "sites": sites,
            "macro_sites": macro_sites,
        }));
    }

    // SABOTAGE RECEIPT: dropping the `collect_calls` name filter makes
    // f2/f4/f5/f8 fail their is_none() check, since include!/attribute macros
    // would then be mistaken for local invocations.
    for name in ["f2_cross_file.rs", "f4_builtins.rs", "f5_derive.rs", "f6_attr_proc_macro.rs", "f8_include.rs"] {
        let row = files.iter().find(|row| row["file"] == name).unwrap();
        assert_eq!(row["expand_file"], false, "{name} should stay unexpanded");
    }
    // End-to-end counts ARE the lab's "expanded" column.
    let counts_of = |name: &str| {
        let row = files.iter().find(|row| row["file"] == name).unwrap();
        (row["nodes"].as_u64().unwrap(), row["sites"].as_u64().unwrap())
    };
    assert_eq!(counts_of("f1_local_call_in_body.rs"), (2, 2));
    assert_eq!(counts_of("f3_nested.rs"), (2, 1));
    assert_eq!(counts_of("f7_mints_fn.rs"), (3, 2));
    for name in ["f1_local_call_in_body.rs", "f3_nested.rs", "f7_mints_fn.rs"] {
        let row = files.iter().find(|row| row["file"] == name).unwrap();
        assert_eq!(row["budget_hit"], false, "{name} expands under the budget");
    }
    // A macro that keeps re-minting itself never terminates a fixpoint; the
    // pass cap is what stops it, not a growing byte count.
    let recursive = files.iter().find(|row| row["file"] == "f9_recursive.rs").unwrap();
    assert_eq!(recursive["budget_hit"], true);

    // The wire's macro_site row names the invocation that minted the gained
    // facts, tagged source: mbe.
    let f7 = files.iter().find(|row| row["file"] == "f7_mints_fn.rs").unwrap();
    let macro_sites = f7["macro_sites"].as_array().unwrap();
    assert_eq!(macro_sites.len(), 1, "one macro_site row for mkfn");
    assert_eq!(macro_sites[0]["macro_name"], "mkfn");
    assert_eq!(macro_sites[0]["source"], "Mbe");
    assert_eq!(macro_sites[0]["covers_invocation"], true);

    // The extract hook maps a gained site all the way to ORIGINAL file
    // coordinates: the inner_call site sits inside the mkfn! invocation.
    let src = read("f7_mints_fn.rs");
    let mkfn_start = src.find("mkfn!").expect("fixture invokes mkfn!") as u32;
    let mkfn_end = mkfn_start + src[mkfn_start as usize..].find('}').unwrap() as u32 + 1;
    let output = RustSource.extract("f.rs", src.as_bytes(), FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();
    let inner = call
        .aux
        .sites
        .iter()
        .find(|site| output.strings.lookup(site.callee) == "inner_call")
        .expect("mkfn!'s expansion calls inner_call()");
    assert!(
        inner.span.start >= mkfn_start && inner.span.end() <= mkfn_end,
        "gained site {inner:?} outside mkfn! {mkfn_start}..{mkfn_end}"
    );

    // Every def/site gained by expansion reports the ORIGINAL invocation's
    // span, never a spliced-text offset.
    let expanded = expand_file(&src).expect("mkfn! is local");
    let mapped_output = RustSource.extract("f.rs", expanded.text.as_bytes(), FamilyMask::ALL);
    let mapped_call = mapped_output.call.as_ref().unwrap();
    let mut saw_macro_site = false;
    let mut mapped_text = String::new();
    for site in &mapped_call.aux.sites {
        let range = site.span.start..site.span.start + site.span.len;
        if expanded.is_macro_span(range.clone()) {
            saw_macro_site = true;
            let original = expanded.map_span(range).expect("macro span always maps");
            let text = &src[original.start as usize..original.end as usize];
            assert!(
                text.contains("mkfn"),
                "mapped span {original:?} does not cover the mkfn! invocation: {text:?}"
            );
            mapped_text = text.to_string();
        }
    }
    assert!(saw_macro_site, "f7's generated() call should be macro-origin");

    json!({
        "files": files,
        "f7_inner_call_site": {"start": inner.span.start, "end": inner.span.end()},
        "f7_mapped_invocation_text": mapped_text,
    })
}
