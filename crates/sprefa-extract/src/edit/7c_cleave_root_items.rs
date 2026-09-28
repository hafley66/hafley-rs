use super::{dispatch, flatten_each, FamilyMask, FlatFact};
use std::collections::{BTreeMap, BTreeSet};

/// Declaration spans at the CST root, including declarations wrapped by a TS
/// `export_statement`, beside every root child and its kind in byte order.
#[allow(clippy::type_complexity)]
pub(super) fn root_item_spans(
    rel: &str,
    text: &str,
) -> Result<
    (
        BTreeSet<(u32, u32)>,
        Vec<(u32, u32, String)>,
        BTreeMap<(u32, u32), String>,
    ),
    String,
> {
    let mask = FamilyMask {
        cst: true,
        ..FamilyMask::NONE
    };
    let out = dispatch(rel, text.as_bytes(), mask)
        .ok_or_else(|| format!("no CST fact arm owns {rel}"))?;
    let mut exports = BTreeSet::new();
    let mut variable_lists = BTreeSet::new();
    let mut children = Vec::new();
    let mut kinds: BTreeMap<(u32, u32), String> = BTreeMap::new();
    flatten_each(&out, None, &mut |fact: FlatFact| -> Result<(), ()> {
        if let FlatFact::Node {
            family: sprefa_extract::FamilyTag::Cst,
            kind,
            span,
            ..
        } = &fact
        {
            kinds
                .entry((span.start, span.end))
                .or_insert_with(|| kind.clone());
        }
        match fact {
            FlatFact::Node {
                family: sprefa_extract::FamilyTag::Cst,
                kind,
                span,
                ..
            } if kind == "export_statement" => {
                exports.insert((span.start, span.end));
            }
            FlatFact::Node {
                family: sprefa_extract::FamilyTag::Cst,
                kind,
                span,
                ..
            } if matches!(
                kind.as_str(),
                "lexical_declaration" | "variable_declaration"
            ) =>
            {
                variable_lists.insert((span.start, span.end));
            }
            FlatFact::Edge {
                family: sprefa_extract::FamilyTag::Cst,
                kind,
                from,
                to,
                ..
            } if kind == "child" => children.push(((from.start, from.end), (to.start, to.end))),
            _ => {}
        }
        Ok(())
    })
    .map_err(|_| format!("flatten CST {rel}"))?;
    let root = (0, text.len() as u32);
    let mut root_children: Vec<(u32, u32, String)> = children
        .iter()
        .filter(|(from, _)| *from == root)
        .map(|(_, to)| (to.0, to.1, kinds.get(to).cloned().unwrap_or_default()))
        .collect();
    root_children.sort();
    root_children.dedup();
    let direct: BTreeSet<(u32, u32)> = children
        .iter()
        .filter(|(from, _)| *from == root)
        .map(|(_, to)| *to)
        .collect();
    let mut items = direct.clone();
    items.extend(
        children.iter().filter_map(|(from, to)| {
            (direct.contains(from) && exports.contains(from)).then_some(*to)
        }),
    );
    let visible_lists: BTreeSet<(u32, u32)> = items
        .iter()
        .filter(|span| variable_lists.contains(span))
        .copied()
        .collect();
    items.extend(
        children
            .into_iter()
            .filter_map(|(from, to)| visible_lists.contains(&from).then_some(to)),
    );
    Ok((items, root_children, kinds))
}
