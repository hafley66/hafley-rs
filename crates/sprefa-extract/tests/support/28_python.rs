use serde_json::{json, Value};

use sprefa_extract::{FamilyMask, PythonSource, Source};

const PATH: &str = "tests/fixtures/python/sample.py";
const SOURCE: &[u8] = include_bytes!("../fixtures/python/sample.py");
const DOCS_PATH: &str = "tests/fixtures/python/docs.py";
const DOCS_SOURCE: &[u8] = include_bytes!("../fixtures/python/docs.py");

/// Python front-end parity over `PythonSource` driven directly. Expected
/// values are hand-derived from the fixtures, never copied from the
/// extractor's output; every old assert runs as code BEFORE the snapshot
/// freezes the whole tables.
pub fn evaluate(_case: &Value) -> Value {
    let sample = PythonSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let types = sample.types.as_ref().expect("types family");
    let call = sample.call.as_ref().expect("call family");

    let entities: Vec<Value> = types
        .nodes
        .iter()
        .map(|node| {
            json!([
                node.kind.as_str(),
                node.name.map(|id| sample.strings.lookup(id)),
                node.span.start,
            ])
        })
        .collect();
    assert_eq!(
        entities,
        [
            json!(["module", Some("<module>"), 0]),
            json!(["function", Some("add"), 11]),
            json!(["class", Some("Animal"), 71]),
            json!(["method", Some("speak"), 89]),
            json!(["class", Some("Dog"), 136]),
            json!(["method", Some("bark"), 159]),
            json!(["function", Some("inner"), 191]),
            json!(["function", Some("main"), 263]),
        ]
    );

    let sigs: Vec<Value> = types
        .aux
        .sigs
        .iter()
        .map(|sig| {
            json!([
                sig.owner.start,
                sig.slot.as_str(),
                sig.pos,
                sample.strings.lookup(sig.ty),
            ])
        })
        .collect();
    assert_eq!(
        sigs,
        [
            json!([11, "param", 0, "Number"]),
            json!([11, "param", 1, "Number"]),
            json!([11, "ret", 0, "Number"]),
            json!([89, "ret", 0, "Text"]),
            json!([159, "ret", 0, "Text"]),
            json!([191, "ret", 0, "Text"]),
            json!([263, "ret", 0, "Result"]),
        ]
    );

    let defs: Vec<Value> = call
        .nodes
        .iter()
        .map(|node| {
            json!([
                node.kind.as_str(),
                node.name.map(|id| sample.strings.lookup(id)),
            ])
        })
        .collect();
    assert_eq!(
        defs,
        [
            // The module-as-caller def: a nameless whole-file cover so a
            // module-level site has a caller under Resolve<CallF>. Not a
            // call_def wire row (skipped in flatten_call, v5 parity).
            json!(["module", None::<&str>]),
            json!(["function", Some("add")]),
            json!(["method", Some("speak")]),
            json!(["method", Some("bark")]),
            json!(["function", Some("inner")]),
            json!(["function", Some("main")]),
        ]
    );

    let sites: Vec<Value> = call
        .aux
        .sites
        .iter()
        .map(|site| json!(sample.strings.lookup(site.callee)))
        .collect();
    assert_eq!(sites, ["inner", "add", "Dog", "bark", "join"]);

    let cst = sample.cst.as_ref().expect("cst family");
    assert!(!cst.nodes.is_empty());
    assert_eq!(sample.strings.lookup(cst.nodes[0].kind), "module");

    // A masked-off family stays None.
    let masked = PythonSource.extract(
        PATH,
        SOURCE,
        FamilyMask {
            cst: true,
            types: false,
            call: false,
            df: false,
            data: false,
        },
    );
    assert!(masked.cst.is_some());
    assert!(masked.types.is_none());
    assert!(masked.call.is_none());
    assert!(masked.df.is_none());

    // One row per imported name; the path-only form keeps the path in `name`,
    // an alias moves the path to `module`, a `from` form always sets `module`
    // and sets `imported` only when the local name differs from the source
    // name.
    let docs = PythonSource.extract(DOCS_PATH, DOCS_SOURCE, FamilyMask::ALL);
    let docs_call = docs.call.as_ref().expect("docs call family");
    let specifiers: Vec<Value> = docs_call
        .aux
        .specifiers
        .iter()
        .map(|specifier| {
            json!([
                specifier.kind.as_str(),
                docs.strings.lookup(specifier.name),
                specifier.module.map(|id| docs.strings.lookup(id)),
                specifier.imported.map(|id| docs.strings.lookup(id)),
            ])
        })
        .collect();
    assert_eq!(
        specifiers,
        [
            json!(["named", "Optional", Some("typing"), None::<&str>]),
            json!(["named", "osp", Some("os.path"), None::<&str>]),
            json!(["named", "sibling", Some("."), None::<&str>]),
            json!(["named", "alias", Some(".pkg.sub"), Some("thing")]),
            json!(["named", "other", Some(".pkg.sub"), None::<&str>]),
        ]
    );
    let sample_call = sample.call.as_ref().expect("sample call family");
    let plain: Vec<Value> = sample_call
        .aux
        .specifiers
        .iter()
        .map(|specifier| {
            json!([
                sample.strings.lookup(specifier.name),
                specifier.module.map(|id| sample.strings.lookup(id)),
            ])
        })
        .collect();
    assert_eq!(plain, [json!(["os", None::<&str>])]);

    // Sphinx field tags off the class and method docstrings: `:param name:`
    // keeps the name as `arg`, `:returns:`/`:return:` normalize to `returns`
    // with no arg.
    let docs_types = docs.types.as_ref().expect("docs types family");
    let strings = &docs.strings;
    let mut tags: Vec<Value> = Vec::new();
    for doc in &docs_types.aux.docs {
        for tag in &doc.tags {
            tags.push(json!([
                doc.owner.start,
                strings.lookup(tag.tag),
                tag.arg.map(|id| strings.lookup(id)),
                strings.lookup(tag.text),
            ]));
        }
    }
    assert_eq!(
        tags,
        [
            json!([0, "author", None::<&str>, "fixture"]),
            json!([160, "param", Some("name"), "the engine name"]),
            json!([160, "returns", None::<&str>, "nothing"]),
            json!([305, "param", Some("mode"), "how to run"]),
            json!([305, "returns", None::<&str>, "the outcome"]),
        ]
    );
    let texts: Vec<Value> = docs_types
        .aux
        .docs
        .iter()
        .map(|doc| json!(docs.strings.lookup(doc.text).lines().next().unwrap_or("")))
        .collect();
    assert_eq!(
        texts,
        [
            json!("Module docstring."),
            json!("An engine."),
            json!("Run it."),
            json!("single-quoted doc"),
        ]
    );

    json!({
        "type_entities": entities,
        "type_sigs": sigs,
        "call_defs": defs,
        "call_sites": sites,
        "cst_root": sample.strings.lookup(cst.nodes[0].kind),
        "docs_specifiers": specifiers,
        "sample_specifiers": plain,
        "doc_tags": tags,
        "doc_texts": texts,
    })
}
