use super::{inside, line_span, FileFacts, Respell, SpecifierRow};
use sprefa_extract::edit::ts_mutate::{import_style, styled_module};

fn lines(rows: &[SpecifierRow], module: &str, style_text: &str) -> String {
    let (quote, semicolon, _) = import_style(style_text);
    let end = if semicolon { ";" } else { "" };
    let mut out = String::new();
    for reexport in [false, true] {
        for type_only in [false, true] {
            let group: Vec<_> = rows.iter().filter(|row| (row.kind == "reexport") == reexport && row.type_only == type_only).collect();
            if group.is_empty() {
                continue;
            }
            let mut clauses = Vec::new();
            for row in group.iter().filter(|row| row.kind == "default" || row.kind == "namespace") {
                clauses.push(if row.kind == "default" { row.name.clone() } else { format!("* as {}", row.name) });
            }
            let named: Vec<_> = group.iter().filter(|row| row.kind != "default" && row.kind != "namespace").map(|row| {
                let imported = row.imported.as_deref().unwrap_or(&row.name);
                if imported == row.name { row.name.clone() } else { format!("{imported} as {}", row.name) }
            }).collect();
            if !named.is_empty() {
                clauses.push(format!("{{ {} }}", named.join(", ")));
            }
            let keyword = if reexport { "export" } else { "import" };
            let type_head = if type_only { " type" } else { "" };
            out.push_str(&format!("{keyword}{type_head} {} from {quote}{module}{quote}{end}\n", clauses.join(", ")));
        }
    }
    out
}

pub(super) fn caller(
    facts: &FileFacts,
    rel: &str,
    item: &str,
    old_module: &str,
    new_module: &str,
    type_only: bool,
) -> Vec<Respell> {
    let mut out = Vec::new();
    for statement in &facts.import_statements {
        let old = facts.specifiers.iter().find(|row| {
            row.imported.as_deref().unwrap_or(&row.name) == item && row.module == old_module && inside(row.span, *statement)
        });
        let Some(old) = old else { continue };
        let kept: Vec<_> = facts.specifiers.iter().filter(|row| {
            inside(row.span, *statement) && row.span != old.span
                && matches!(row.kind.as_str(), "named" | "default" | "namespace" | "reexport")
        }).cloned().collect();
        let mut landed = old.clone();
        landed.type_only |= type_only;
        let new_module = styled_module(new_module, old_module);
        let span = line_span(&facts.text, *statement);
        let style_text = facts.slice(span);
        let replacement = format!("{}{}", lines(&kept, old_module, style_text), lines(&[landed], &new_module, style_text));
        out.push(Respell {
            file: rel.to_string(),
            span,
            text: replacement,
            receipt: Some(format!("caller {rel}: {item} -> {new_module}")),
        });
    }
    out
}

pub(super) fn add(
    source: &FileFacts,
    dest: Option<&FileFacts>,
    block: &str,
    module: &str,
    names: &[String],
) -> String {
    let mut out = block.to_string();
    let mut rows = Vec::new();
    let style = dest.filter(|facts| !facts.specifiers.is_empty()).unwrap_or(source);
    for name in names {
        if dest.is_some_and(|facts| facts.specifiers.iter().any(|row| row.name == *name && matches!(row.kind.as_str(), "named" | "default" | "namespace"))) {
            continue;
        }
        let row = source.specifiers.iter().find(|row| row.name == *name && matches!(row.kind.as_str(), "named" | "default" | "namespace"));
        rows.push(row.cloned().unwrap_or_else(|| SpecifierRow {
            name: name.clone(),
            module: module.to_string(),
            span: hafley_scm::span::Span::anchor(0),
            glob: false,
            kind: "named".to_string(),
            imported: None,
            type_only: source.decls.iter().any(|row| row.name == *name && row.type_only),
        }));
    }
    if !rows.is_empty() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&lines(&rows, module, &style.text));
    }
    out
}
