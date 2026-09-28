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
                let start = leading_trivia_start(text, &root_children, *decl_start, *decl_end);
                let span = line_span(text, span_of(start, *decl_end));
                if !top_level.contains(&(*decl_start, *decl_end)) {
                    continue;
                }
                let name = declared(symbol);
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
