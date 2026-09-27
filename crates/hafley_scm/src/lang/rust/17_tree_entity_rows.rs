//! Rust type entities and callable signatures projected from the shared CST.

use std::ops::Range;

use super::type_entity_rows::{
    DocRow, DocSectionRow, ImplSelfHeadRow, SignatureRef, SignatureSlot, TypeEntityKind,
    TypeEntityRow, TypeEntityRows,
};
use super::tree_nodes::named_children;

pub fn type_entity_rows_from_tree(tree: &tree_sitter::Tree, source: &[u8]) -> TypeEntityRows {
    let mut rows = TypeEntityRows::default();
    collect_items(tree.root_node(), source, None, &mut rows);
    rows
}

fn collect_items(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    parent: Option<&str>,
    rows: &mut TypeEntityRows,
) {
    let mut doc_lines = Vec::new();
    for child in named_children(node) {
        if let Some(line) = doc_line(child, source) {
            doc_lines.push(line);
            continue;
        }
        match child.kind() {
            "struct_item" => push_named(child, source, TypeEntityKind::Struct, &doc_lines, rows),
            "enum_item" => push_named(child, source, TypeEntityKind::Enum, &doc_lines, rows),
            "union_item" => push_named(child, source, TypeEntityKind::Struct, &doc_lines, rows),
            "type_item" => push_named(child, source, TypeEntityKind::Alias, &doc_lines, rows),
            "trait_item" => {
                push_named(child, source, TypeEntityKind::Trait, &doc_lines, rows);
                collect_trait_items(child, source, rows);
            }
            "function_item" => push_callable(
                child,
                source,
                TypeEntityKind::Function,
                &doc_lines,
                parent,
                rows,
            ),
            "impl_item" => collect_impl_items(child, source, rows),
            "mod_item" => {
                if let Some(body) = child.child_by_field_name("body") {
                    collect_items(body, source, parent, rows);
                }
            }
            _ => {
                doc_lines.clear();
                continue;
            }
        }
        doc_lines.clear();
    }
}

fn collect_trait_items(node: tree_sitter::Node<'_>, source: &[u8], rows: &mut TypeEntityRows) {
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let parent = node
        .child_by_field_name("name")
        .map(|name| node_text(name, source).to_owned());
    let mut doc_lines = Vec::new();
    for child in named_children(body) {
        if let Some(line) = doc_line(child, source) {
            doc_lines.push(line);
            continue;
        }
        match child.kind() {
            "associated_type" => push_named(child, source, TypeEntityKind::Alias, &doc_lines, rows),
            "function_item" if child.child_by_field_name("body").is_some() => push_callable(
                child,
                source,
                TypeEntityKind::Method,
                &doc_lines,
                parent.as_deref(),
                rows,
            ),
            _ => {}
        }
        doc_lines.clear();
    }
}

fn collect_impl_items(node: tree_sitter::Node<'_>, source: &[u8], rows: &mut TypeEntityRows) {
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    if let Some(ty) = node.child_by_field_name("type") {
        let head = match ty.kind() {
            "type_identifier" => Some(ty),
            "generic_type" => ty
                .child_by_field_name("type")
                .filter(|head| head.kind() == "type_identifier"),
            _ => None,
        };
        if let Some(head) = head {
            rows.impl_self_heads.push(ImplSelfHeadRow {
                range: span(head),
                name: node_text(head, source).to_owned(),
            });
        }
    }
    let parent = node
        .child_by_field_name("type")
        .map(|ty| node_text(ty, source).to_owned());
    let mut doc_lines = Vec::new();
    for child in named_children(body) {
        if let Some(line) = doc_line(child, source) {
            doc_lines.push(line);
            continue;
        }
        if child.kind() == "function_item" {
            push_callable(
                child,
                source,
                TypeEntityKind::Method,
                &doc_lines,
                parent.as_deref(),
                rows,
            );
        }
        doc_lines.clear();
    }
}

fn push_named(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    kind: TypeEntityKind,
    docs: &[String],
    rows: &mut TypeEntityRows,
) {
    let Some(name) = node.child_by_field_name("name") else {
        return;
    };
    let range = span(name);
    rows.entities.push(TypeEntityRow {
        range: range.clone(),
        name: node_text(name, source).to_owned(),
        kind,
        sigs: Vec::new(),
    });
    push_doc(rows, range, docs, None);
}

fn push_callable(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    kind: TypeEntityKind,
    docs: &[String],
    parent: Option<&str>,
    rows: &mut TypeEntityRows,
) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let mut row = TypeEntityRow {
        range: span(name_node),
        name: node_text(name_node, source).to_owned(),
        kind,
        sigs: Vec::new(),
    };
    if let Some(parameters) = node.child_by_field_name("parameters") {
        let mut pos = 0;
        for parameter in named_children(parameters) {
            if parameter.kind() != "parameter" {
                continue;
            }
            if let Some(ty) = parameter.child_by_field_name("type") {
                append_type_refs(ty, source, SignatureSlot::Param, pos, &mut row.sigs);
            }
            pos += 1;
        }
    }
    if let Some(ty) = node.child_by_field_name("return_type") {
        append_type_refs(ty, source, SignatureSlot::Ret, 0, &mut row.sigs);
    }
    let range = row.range.clone();
    rows.entities.push(row);
    push_doc(rows, range, docs, parent);
}

fn doc_line(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    if !matches!(node.kind(), "line_comment" | "block_comment") {
        return None;
    }
    let text = node_text(node, source).trim();
    let line = text.strip_prefix("///")?;
    Some(line.strip_prefix(' ').unwrap_or(line).to_owned())
}

fn push_doc(rows: &mut TypeEntityRows, range: Range<u32>, lines: &[String], parent: Option<&str>) {
    if lines.is_empty() {
        return;
    }
    let text = lines.join("\n");
    rows.docs.push(DocRow {
        range,
        parent: parent.map(str::to_owned),
        sections: doc_sections(&text),
        text,
    });
}

fn doc_sections(text: &str) -> Vec<DocSectionRow> {
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("# ") {
            sections.push((rest.trim().to_owned(), Vec::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push(line);
        }
    }
    sections
        .into_iter()
        .map(|(heading, body)| DocSectionRow {
            heading,
            body: body.join("\n").trim().to_owned(),
        })
        .collect()
}

fn append_type_refs(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    slot: SignatureSlot,
    pos: u32,
    out: &mut Vec<SignatureRef>,
) {
    if matches!(node.kind(), "type_identifier" | "scoped_type_identifier") {
        let name = node_text(node, source);
        if !matches!(
            name,
            "Self"
                | "bool"
                | "char"
                | "str"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
                | "f32"
                | "f64"
        ) {
            out.push(SignatureRef {
                slot,
                pos,
                name: name.to_owned(),
            });
        }
    }
    for child in named_children(node) {
        append_type_refs(child, source, slot, pos, out);
    }
    out.sort_by(|a, b| {
        let slot = |slot| match slot {
            SignatureSlot::Param => 0,
            SignatureSlot::Ret => 1,
        };
        (slot(a.slot), a.pos, &a.name).cmp(&(slot(b.slot), b.pos, &b.name))
    });
    out.dedup_by(|a, b| a.slot == b.slot && a.pos == b.pos && a.name == b.name);
}

fn span(node: tree_sitter::Node<'_>) -> Range<u32> {
    node.start_byte() as u32..node.end_byte() as u32
}

fn node_text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).expect("Rust identifiers are UTF-8")
}
