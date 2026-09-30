use super::*;

/// The scope rows one file contributes: its top-level declarations, line
/// aligned, every free name each carries, and each Rust `impl` by self type.
#[allow(clippy::type_complexity)]
pub(super) fn scope_rows(
    cx: &MoveCx,
    rel: &str,
    text: &str,
) -> Result<(Vec<Decl>, Vec<(String, Span)>, Vec<(String, Span)>), String> {
    let path = cx.materialize(rel, &overlay_scratch())?;
    let facts = scm_facts(&[path]).map_err(|error| format!("scope rows for {rel}: {error}"))?;
    let (top_level, root_children, kinds) = root_item_spans(rel, text)?;
    let mut decls: Vec<Decl> = Vec::new();
    let mut free = Vec::new();
    for fact in &facts {
        match fact {
            FlatFact::OccurrenceRow {
                role,
                exported,
                decl_start,
                decl_end,
                symbol,
                ..
            } if role == "def" => {
                if !top_level.contains(&(*decl_start, *decl_end)) {
                    continue;
                }
                let name = declared(symbol);
                let first = overloads_start(text, &root_children, &kinds, &name, *decl_start, *decl_end);
                let start = leading_trivia_start(text, &root_children, first, *decl_end);
                let span = line_span(text, span_of(start, *decl_end));
                if decls.iter().any(|held| held.name == name) {
                    continue;
                }
                decls.push(Decl {
                    name,
                    span,
                    exported: *exported,
                    type_only: matches!(
                        kinds.get(&(*decl_start, *decl_end)).map(String::as_str),
                        Some("interface_declaration" | "type_alias_declaration")
                    ),
                });
            }
            // `std::collections::HashMap` names HashMap through its path, not
            // through a `use HashMap`: only a path's first segment is free.
            FlatFact::FreeNameRow {
                name, start, end, ..
            } if !text
                .get(..*start as usize)
                .is_some_and(|before| before.trim_end().ends_with("::")) =>
            {
                free.push((name.clone(), span_of(*start, *end)))
            }
            _ => {}
        }
    }
    decls.sort_by_key(|decl| decl.span.start);
    let impls = root_children
        .iter()
        .filter(|(_, _, kind)| kind == "impl_item")
        .filter_map(|(start, end, _)| {
            let self_ty = impl_self(text.get(*start as usize..*end as usize)?)?;
            let from = leading_trivia_start(text, &root_children, *start, *end);
            Some((self_ty, line_span(text, span_of(from, *end))))
        })
        .collect();
    Ok((decls, free, impls))
}

/// A TS overload signature (`function f(a: A): A;`) has no def row of its own;
/// the root items right before the implementation that are signatures of the
/// same name travel with it.
fn overloads_start(
    text: &str,
    root_children: &[(u32, u32, String)],
    kinds: &BTreeMap<(u32, u32), String>,
    name: &str,
    decl_start: u32,
    decl_end: u32,
) -> u32 {
    let Some(index) = root_children
        .iter()
        .position(|(start, end, _)| *start <= decl_start && decl_end <= *end)
    else {
        return decl_start;
    };
    let mut first = decl_start;
    for (start, end, _) in root_children[..index].iter().rev() {
        let signature = kinds
            .range((*start, *start)..)
            .take_while(|((node_start, _), _)| node_start < end)
            .any(|((_, node_end), kind)| kind == "function_signature" && node_end <= end);
        let named = text.get(*start as usize..*end as usize).is_some_and(|item| {
            item.split_once(&format!("function {name}"))
                .is_some_and(|(_, rest)| rest.trim_start().starts_with(['(', '<']))
        });
        if !(signature && named) {
            break;
        }
        first = *start;
    }
    first
}
