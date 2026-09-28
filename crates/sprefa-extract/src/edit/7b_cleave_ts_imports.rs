use super::{inside, line_span, FileFacts, Respell, SpecifierRow};
use hafley_scm::span::Span;

fn line(row: &SpecifierRow, module: &str) -> String {
    let imported = row.imported.as_deref().unwrap_or(&row.name);
    let clause = match row.kind.as_str() {
        "default" => row.name.clone(),
        "namespace" => format!("* as {}", row.name),
        _ if imported == row.name => format!("{{ {} }}", row.name),
        _ => format!("{{ {imported} as {} }}", row.name),
    };
    let type_head = if row.type_only { " type" } else { "" };
    format!("import{type_head} {clause} from \"{module}\";\n")
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
    let mut moved = None;
    for statement in &facts.import_statements {
        let old = facts.specifiers.iter().find(|row| {
            row.name == item && row.module == old_module && inside(row.span, *statement)
        });
        let Some(old) = old else { continue };
        moved = Some(old);
        let replacement = facts
            .specifiers
            .iter()
            .filter(|row| {
                inside(row.span, *statement)
                    && row.name != item
                    && matches!(row.kind.as_str(), "named" | "default" | "namespace")
            })
            .map(|row| line(row, &row.module))
            .collect();
        out.push(Respell {
            file: rel.to_string(),
            span: line_span(&facts.text, *statement),
            text: replacement,
            receipt: Some(format!("caller {rel}: {old_module} loses {item}")),
        });
    }
    if let Some(old) = moved {
        let mut landed = old.clone();
        landed.kind = "named".to_string();
        landed.imported = None;
        landed.type_only |= type_only;
        out.push(Respell {
            file: rel.to_string(),
            span: Span::anchor(facts.import_end),
            text: line(&landed, new_module),
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
    for name in names {
        if dest.is_some_and(|facts| {
            facts.specifiers.iter().any(|row| {
                row.name == *name && matches!(row.kind.as_str(), "named" | "default" | "namespace")
            })
        }) {
            continue;
        }
        let binding = source.specifiers.iter().find(|row| row.name == *name);
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        if let Some(binding) = binding {
            out.push_str(&line(binding, module));
        } else {
            let type_head = if source
                .decls
                .iter()
                .any(|row| row.name == *name && row.type_only)
            {
                " type"
            } else {
                ""
            };
            out.push_str(&format!(
                "import{type_head} {{ {name} }} from \"{module}\";\n"
            ));
        }
    }
    out
}
