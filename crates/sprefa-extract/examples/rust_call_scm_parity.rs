// Inventory of direct Rust CallF query rows and production CallF rows. This example
// deliberately does not compare the production path with its implementation.
// Run: cargo run --example rust_call_scm_parity -- PATH...
use std::collections::BTreeMap;

use hafley_scm::lang::rust::RUST_CALL_QUERY;
use sprefa_extract::{FamilyMask, RustSource, Source};

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let q = hafley_scm::build(&language, RUST_CALL_QUERY).expect("rust call query builds");
    let (mut files, mut direct_scm_rows, mut production_claimed_rows, mut macro_minted_rows) =
        (0, 0, 0, 0);
    let mut const_init_rows = 0;

    for path in &paths {
        let Ok(src) = std::fs::read(path) else {
            continue;
        };
        let scm = scm_defs(&q, &language, &src);
        let production = production_defs(path, &src);
        let production_claimed: BTreeMap<_, _> = production
            .iter()
            .filter(|(_, (kind, _))| kind != "const_init")
            .map(|(key, value)| (*key, value.clone()))
            .collect();
        let missing_from_direct = production_claimed
            .keys()
            .filter(|key| !scm.contains_key(key))
            .count();
        files += 1;
        direct_scm_rows += scm.len();
        production_claimed_rows += production_claimed.len();
        macro_minted_rows += missing_from_direct;
        const_init_rows += production
            .values()
            .filter(|(kind, _)| kind == "const_init")
            .count();
    }
    assert_eq!(direct_scm_rows, 6420);
    assert_eq!(production_claimed_rows, 6423);
    assert_eq!(macro_minted_rows, 3);
    assert_eq!(const_init_rows, 49);
    println!(
        "files={files} direct_scm={direct_scm_rows} production_claimed={production_claimed_rows} macro_minted={macro_minted_rows} const_init={const_init_rows} production_rows={}",
        production_claimed_rows + const_init_rows
    );
}

fn production_defs(path: &str, src: &[u8]) -> BTreeMap<(u32, u32), (String, String)> {
    let output = RustSource.extract(path, src, FamilyMask::ALL);
    let Some(call) = output.call.as_ref() else {
        return BTreeMap::new();
    };
    call.nodes
        .iter()
        .filter_map(|node| {
            let kind = node.kind.as_str().to_string();
            let name = node.name.map(|id| output.strings.lookup(id)).unwrap_or("");
            Some(((node.span.start, node.span.end()), (kind, name.to_string())))
        })
        .collect()
}

fn scm_defs(
    q: &hafley_scm::QueryExt,
    language: &tree_sitter::Language,
    src: &[u8],
) -> BTreeMap<(u32, u32), (String, String)> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(language).expect("grammar");
    let Some(tree) = parser.parse(src, None) else {
        return BTreeMap::new();
    };
    let mut arena = hafley_scm::MatchArena::default();
    hafley_scm::run(q, "probe", src, &tree, u32::MAX, &mut arena).expect("run");

    let mut defs = BTreeMap::new();
    for row in &arena.rows {
        let spans = &arena.spans[row.spans.start as usize..row.spans.end as usize];
        let at = |label: &str| {
            spans
                .iter()
                .find(|s| q.names[s.name as usize].as_ref() == label)
                .map(|s| s.bytes.clone())
        };
        let name = at("def.name")
            .map(|b| String::from_utf8_lossy(&src[b.start as usize..b.end as usize]).to_string())
            .unwrap_or_default();
        let (kind, span) = if let Some(lambda) = at("def.lambda") {
            ("lambda", lambda)
        } else if let Some(variant) = at("def.variant") {
            ("function", variant)
        } else if let (Some(n), Some(b)) = (at("def.name"), at("def.body")) {
            let kind = if at("def.method").is_some() {
                "method"
            } else {
                "function"
            };
            (kind, n.start..b.end)
        } else if let (Some(n), Some(s)) = (at("def.name"), at("def.sig")) {
            let mut end = s.end - 1;
            while end > n.end && src[end as usize - 1].is_ascii_whitespace() {
                end -= 1;
            }
            ("method", n.start..end)
        } else {
            continue;
        };
        defs.entry((span.start, span.end))
            .or_insert((kind.to_string(), name));
    }
    defs
}
