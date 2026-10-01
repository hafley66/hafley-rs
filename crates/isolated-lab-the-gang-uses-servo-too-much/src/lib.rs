mod _0_node;
mod _1_udf;

use _0_node::Node;
use _1_udf::{Impl, P};
use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::parser::ParseRelative;
use selectors::SelectorList;

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

pub fn select(source: &str, css: &str) -> Result<Vec<Hit>, String> {
    let list = SelectorList::<Impl>::parse(&P, &mut cssparser::Parser::new(css), ParseRelative::No)
        .map_err(|e| format!("{e:?}"))?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into()).map_err(|e| e.to_string())?;
    let tree = parser.parse(source, None).ok_or("parse failed")?;
    let mut caches = SelectorCaches::default();
    let mut cx = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );
    let mut hits = vec![];
    let mut stack = vec![tree.root_node()];
    while let Some(ts) = stack.pop() {
        let node = Node { ts, src: source };
        if selectors::matching::matches_selector_list(&list, &node, &mut cx) {
            hits.push(Hit {
                kind: ts.kind().to_string(),
                start: ts.start_byte(),
                end: ts.end_byte(),
                text: node.text().to_string(),
            });
        }
        let mut cursor = ts.walk();
        stack.extend(ts.named_children(&mut cursor).collect::<Vec<_>>().into_iter().rev());
    }
    Ok(hits)
}
