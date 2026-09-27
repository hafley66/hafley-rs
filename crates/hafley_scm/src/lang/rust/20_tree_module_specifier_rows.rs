//! Rust module and `use` leaves projected from the shared tree-sitter parse.

use super::module_specifier_rows::{ModuleSpecifierKind, ModuleSpecifierRow};

pub fn module_specifier_rows_from_tree(
    tree: &tree_sitter::Tree,
    source: &[u8],
) -> Vec<ModuleSpecifierRow> {
    let mut rows = Vec::new();
    collect_items(tree.root_node(), source, &mut Vec::new(), &mut rows);
    rows
}

fn collect_items(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    pending_attrs: &mut Vec<(String, u32)>,
    rows: &mut Vec<ModuleSpecifierRow>,
) {
    for child in named_children(node) {
        match child.kind() {
            "attribute_item" | "inner_attribute_item" => {
                pending_attrs.push((text(child, source).to_owned(), child.start_byte() as u32));
            }
            "use_declaration" => {
                let reexport = named_children(child)
                    .into_iter()
                    .any(|item| item.kind() == "visibility_modifier");
                if let Some(clause) = named_children(child)
                    .into_iter()
                    .find(|item| item.kind() != "visibility_modifier")
                {
                    use_node(clause, reexport, &mut Vec::new(), source, rows);
                }
                pending_attrs.clear();
            }
            "mod_item" => {
                if let Some(body) = child.child_by_field_name("body") {
                    collect_items(body, source, &mut Vec::new(), rows);
                } else if let Some(name_node) = child.child_by_field_name("name") {
                    let name = text(name_node, source).to_owned();
                    let path = pending_attrs
                        .iter()
                        .find_map(|(attr, _)| path_attribute(attr));
                    let module = path.clone().unwrap_or_else(|| name.clone());
                    let start = pending_attrs
                        .first()
                        .map_or(child.start_byte() as u32, |(_, start)| *start);
                    rows.push(ModuleSpecifierRow {
                        range: start..child.end_byte() as u32,
                        name,
                        kind: if path.is_some() {
                            ModuleSpecifierKind::ModulePath
                        } else {
                            ModuleSpecifierKind::Module
                        },
                        module,
                    });
                    pending_attrs.clear();
                }
            }
            _ => pending_attrs.clear(),
        }
    }
}

fn use_node(
    node: tree_sitter::Node<'_>,
    reexport: bool,
    prefix: &mut Vec<String>,
    source: &[u8],
    rows: &mut Vec<ModuleSpecifierRow>,
) {
    match node.kind() {
        "scoped_use_list" => {
            let children = named_children(node);
            if let Some(path) = children.first() {
                prefix.extend(path_segments(*path, source));
            }
            for child in children.iter().skip(1) {
                if child.kind() == "use_list" {
                    for leaf in named_children(*child) {
                        use_node(leaf, reexport, prefix, source, rows);
                    }
                } else {
                    use_node(*child, reexport, prefix, source, rows);
                }
            }
            prefix.clear();
        }
        "use_list" => {
            for child in named_children(node) {
                use_node(child, reexport, prefix, source, rows);
            }
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let mut segments = path_segments(node, source);
            if segments.last().map(String::as_str) == Some("self") {
                segments.pop();
            }
            if !segments.is_empty() {
                let name_node = named_children(node).last().copied().unwrap_or(node);
                rows.push(ModuleSpecifierRow {
                    range: name_node.start_byte() as u32..name_node.end_byte() as u32,
                    name: segments.last().cloned().unwrap_or_default(),
                    kind: if reexport {
                        ModuleSpecifierKind::Reexport
                    } else {
                        ModuleSpecifierKind::Named
                    },
                    module: segments.join("::"),
                });
            }
        }
        "use_as_clause" => {
            let children = named_children(node);
            let Some(path) = children.first() else { return };
            let mut path_segments = path_segments(*path, source);
            if path_segments.last().map(String::as_str) == Some("self") {
                path_segments.pop();
            }
            let Some(target) = path_segments.last().cloned() else {
                return;
            };
            let mut segments = prefix.clone();
            segments.extend(path_segments);
            let row_start = named_children(*path)
                .last()
                .map_or(path.start_byte(), |item| item.start_byte())
                as u32;
            let name = children
                .get(1)
                .map(|alias| text(*alias, source).to_owned())
                .unwrap_or(target);
            rows.push(ModuleSpecifierRow {
                range: row_start..node.end_byte() as u32,
                name,
                kind: if reexport {
                    ModuleSpecifierKind::Reexport
                } else {
                    ModuleSpecifierKind::Named
                },
                module: segments.join("::"),
            });
        }
        "self" => {
            if let Some(name) = prefix.last() {
                rows.push(ModuleSpecifierRow {
                    range: node.start_byte() as u32..node.end_byte() as u32,
                    name: name.clone(),
                    kind: if reexport {
                        ModuleSpecifierKind::Reexport
                    } else {
                        ModuleSpecifierKind::Named
                    },
                    module: prefix.join("::"),
                });
            }
        }
        "use_wildcard" => {
            let scoped_prefix = named_children(node)
                .into_iter()
                .flat_map(|path| path_segments(path, source))
                .collect::<Vec<_>>();
            let path = if scoped_prefix.is_empty() {
                prefix.as_slice()
            } else {
                scoped_prefix.as_slice()
            };
            if let Some(name) = path.last() {
                let star = (0..node.child_count())
                    .filter_map(|index| node.child(index))
                    .find(|child| child.kind() == "*")
                    .unwrap_or(node);
                rows.push(ModuleSpecifierRow {
                    range: star.start_byte() as u32..star.end_byte() as u32,
                    name: name.clone(),
                    kind: if reexport {
                        ModuleSpecifierKind::Reexport
                    } else {
                        ModuleSpecifierKind::Namespace
                    },
                    module: path.join("::"),
                });
            }
        }
        "identifier" | "type_identifier" => {
            let name = text(node, source).to_owned();
            let mut segments = prefix.clone();
            segments.push(name.clone());
            rows.push(ModuleSpecifierRow {
                range: node.start_byte() as u32..node.end_byte() as u32,
                name,
                kind: if reexport {
                    ModuleSpecifierKind::Reexport
                } else {
                    ModuleSpecifierKind::Named
                },
                module: segments.join("::"),
            });
        }
        _ => {
            for child in named_children(node) {
                use_node(child, reexport, prefix, source, rows);
            }
        }
    }
}

fn path_segments(node: tree_sitter::Node<'_>, source: &[u8]) -> Vec<String> {
    text(node, source)
        .split("::")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

fn path_attribute(attribute: &str) -> Option<String> {
    let inner = attribute.strip_prefix("#[")?.strip_suffix(']')?;
    let (key, value) = inner.split_once('=')?;
    if key.trim() != "path" {
        return None;
    }
    let value = value.trim().strip_prefix('"')?.strip_suffix('"')?;
    Some(value.replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn named_children(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).unwrap_or_default()
}
