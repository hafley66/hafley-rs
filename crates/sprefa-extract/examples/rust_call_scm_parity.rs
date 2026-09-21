// In-process parity harness for queries/rust/call.scm against the syn walker
// (`call_defs_in_items`). Run: cargo run --example rust_call_scm_parity -- PATH...
use std::collections::BTreeMap;

use sprefa_extract::{FamilyMask, RustSource, Source};

const CALL_SCM: &str = include_str!("../queries/rust/call.scm");

/// The def kinds the `.scm` claims. `const_init` is not among them: it is minted by
/// `initializer_defs` from call-site coverage, which no query expresses.
const CLAIMED: [&str; 3] = ["function", "method", "lambda"];

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let q = hafley_scm::build(&language, CALL_SCM).expect("call.scm builds");
    let (mut files, mut rows, mut bad, mut skipped) = (0, 0, 0, 0);

    for path in &paths {
        let Ok(src) = std::fs::read(path) else { continue };
        let walker = walker_defs(path, &src);
        let scm = scm_defs(&q, &language, &src);
        files += 1;
        rows += walker.len();
        let only_walker: Vec<_> = walker.iter().filter(|(k, _)| !scm.contains_key(*k)).collect();
        let only_scm: Vec<_> = scm.iter().filter(|(k, _)| !walker.contains_key(*k)).collect();
        skipped += only_walker.len();
        if !only_walker.is_empty() || !only_scm.is_empty() {
            bad += 1;
            println!("--- {path}");
            for ((start, end), (kind, name)) in only_walker {
                println!("  walker only  {start}..{end}\t{kind}\t{name}");
            }
            for ((start, end), (kind, name)) in only_scm {
                println!("  scm only     {start}..{end}\t{kind}\t{name}");
            }
        }
    }
    println!("files={files} mismatched={bad} walker_rows={rows} walker_only_rows={skipped}");
}

/// The syn walker's def rows, macro-spliced and `const_init` rows dropped: the first
/// come from `splice_macro_expansions`, the second from `initializer_defs`.
fn walker_defs(path: &str, src: &[u8]) -> BTreeMap<(u32, u32), (String, String)> {
    let output = RustSource.extract(path, src, FamilyMask::ALL);
    let Some(call) = output.call.as_ref() else {
        return BTreeMap::new();
    };
    call.nodes
        .iter()
        .filter_map(|node| {
            let kind = node.kind.as_str().to_string();
            if !CLAIMED.contains(&kind.as_str()) {
                return None;
            }
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
            let kind = if at("def.method").is_some() { "method" } else { "function" };
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
