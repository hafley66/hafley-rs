//! `build` path: a top-level pattern with any non-native, non-`#emit!` predicate compiles through
//! `compile`, and those predicates are cut from the text tree-sitter sees; other patterns stay byte-identical.
use std::ops::Range;

use tree_sitter::Language;

use super::_0_types::{Compiled, ScmppError};
use super::_1_parens::{cut, top_items, Pred};
use super::_2_compile::{compile, TEXT_BUILTINS};

const EMIT: &str = "emit!";

pub struct Routed {
    /// Text for `Query::new`.
    pub text: String,
    /// Byte range in `text` of each routed pattern, with its compiled form.
    pub items: Vec<(Range<usize>, Compiled)>,
}

fn routed(op: &str) -> bool {
    op != EMIT && !TEXT_BUILTINS.contains(&op)
}

/// `item` with every predicate `drop` names removed.
fn without(scm: &str, item: &Range<usize>, preds: &[Pred], drop: fn(&str) -> bool) -> String {
    let mut out = String::with_capacity(item.len());
    let mut at = item.start;
    for pred in preds.iter().filter(|pred| drop(&pred.op)) {
        out.push_str(&scm[at..pred.offset]);
        at = pred.offset + pred.text.len();
    }
    out.push_str(&scm[at..item.end]);
    out
}

pub fn route(lang: &Language, scm: &str) -> Result<Routed, ScmppError> {
    let mut text = String::with_capacity(scm.len());
    let mut items = Vec::new();
    let mut copied = 0;
    for item in top_items(scm)? {
        let (_, _, preds) = cut(&scm[item.clone()], item.start)?;
        if !preds.iter().any(|pred| routed(&pred.op)) {
            continue;
        }
        let compiled = compile(lang, &without(scm, &item, &preds, |op| op == EMIT))?;
        let exported = super::_3_lower::exports(&compiled.plan, &compiled.patterns)?;
        if let Some((_, name)) = exported
            .iter()
            .find(|(pattern, _)| *pattern != compiled.plan.pattern)
        {
            return Err(ScmppError::Unsupported(format!(
                "@{name} is a rows: each capture; query --scmpp returns those rows"
            )));
        }
        text.push_str(&scm[copied..item.start]);
        let start = text.len();
        text.push_str(&without(scm, &item, &preds, routed));
        items.push((start..text.len(), compiled));
        copied = item.end;
    }
    text.push_str(&scm[copied..]);
    Ok(Routed { text, items })
}
