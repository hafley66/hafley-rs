//! Rust module-resolution syntax projected from the shared tree-sitter parse.

use super::module_resolution_rows::{
    EnumVariantsRow, ImplMethodsRow, ModuleResolutionRows, StarImportRow, TraitMethodRow,
    TraitMethodsRow, UseBindingRow,
};

pub fn module_resolution_rows_from_tree(
    tree: &tree_sitter::Tree,
    source: &[u8],
) -> ModuleResolutionRows {
    let mut rows = ModuleResolutionRows::default();
    collect_items(tree.root_node(), source, &mut rows, &[]);
    rows
}

fn collect_items(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    rows: &mut ModuleResolutionRows,
    scope: &[(String, Option<String>)],
) {
    let mut pending_attrs = Vec::new();
    for item in named_children(node) {
        if matches!(item.kind(), "attribute_item" | "inner_attribute_item") {
            pending_attrs.push(text(item, source).to_owned());
            continue;
        }
        match item.kind() {
            "use_declaration" => {
                let children = named_children(item);
                let reexport = children
                    .iter()
                    .any(|child| child.kind() == "visibility_modifier");
                if let Some(tree) = children
                    .into_iter()
                    .find(|child| child.kind() != "visibility_modifier")
                {
                    collect_use(tree, &mut Vec::new(), reexport, source, rows);
                }
            }
            "trait_item" => {
                let Some(name) = item.child_by_field_name("name") else {
                    continue;
                };
                let methods = item
                    .child_by_field_name("body")
                    .map(named_children)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|child| {
                        matches!(child.kind(), "function_item" | "function_signature_item")
                    })
                    .filter_map(|method| {
                        let method_name = method.child_by_field_name("name")?;
                        let body = method.child_by_field_name("body");
                        let end = body.map_or_else(
                            || {
                                let end = method.end_byte();
                                if end > method_name.start_byte()
                                    && source.get(end - 1) == Some(&b';')
                                {
                                    end - 1
                                } else {
                                    end
                                }
                            },
                            |body| body.end_byte(),
                        );
                        Some(TraitMethodRow {
                            name: text(method_name, source).to_owned(),
                            range: method_name.start_byte() as u32..end as u32,
                            default: body.is_some(),
                        })
                    })
                    .collect();
                rows.traits.push(TraitMethodsRow {
                    name: text(name, source).to_owned(),
                    methods,
                });
            }
            "mod_item" => {
                let Some(name_node) = item.child_by_field_name("name") else {
                    continue;
                };
                let name = text(name_node, source).to_owned();
                if let Some(body) = item.child_by_field_name("body") {
                    rows.inline_mods.push(name.clone());
                    let mut nested = scope.to_vec();
                    nested.push((name, path_attribute(&pending_attrs)));
                    collect_items(body, source, rows, &nested);
                } else {
                    let name = scope
                        .iter()
                        .map(|(name, _)| name.as_str())
                        .chain(std::iter::once(name.as_str()))
                        .collect::<Vec<_>>()
                        .join("::");
                    rows.mod_scopes.insert(name.clone(), scope.to_vec());
                    rows.mod_decls.push((name, path_attribute(&pending_attrs)));
                }
            }
            "enum_item" => {
                let (Some(name), Some(body)) = (
                    item.child_by_field_name("name"),
                    item.child_by_field_name("body"),
                ) else {
                    continue;
                };
                let variants = named_children(body)
                    .into_iter()
                    .filter(|variant| variant.kind() == "enum_variant")
                    .filter_map(|variant| {
                        let name = variant.child_by_field_name("name")?;
                        Some((
                            text(name, source).to_owned(),
                            name.start_byte() as u32..name.end_byte() as u32,
                        ))
                    })
                    .collect();
                rows.enums.push(EnumVariantsRow {
                    name: text(name, source).to_owned(),
                    variants,
                });
            }
            "type_item" => {
                if let Some(name) = item.child_by_field_name("name") {
                    rows.aliases
                        .push(name.start_byte() as u32..name.end_byte() as u32);
                }
            }
            "impl_item" => {
                let (Some(ty), Some(self_type)) = (
                    item.child_by_field_name("type"),
                    item.child_by_field_name("type")
                        .and_then(|ty| principal_ty(ty, source)),
                ) else {
                    continue;
                };
                let trait_name = item
                    .child_by_field_name("trait")
                    .and_then(|trait_ty| path_name(trait_ty, source));
                let methods = item
                    .child_by_field_name("body")
                    .map(named_children)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|child| child.kind() == "function_item")
                    .filter_map(|method| {
                        let name = method.child_by_field_name("name")?;
                        Some((
                            text(name, source).to_owned(),
                            name.start_byte() as u32..method.end_byte() as u32,
                        ))
                    })
                    .collect();
                let _ = ty;
                rows.impls.push(ImplMethodsRow {
                    self_type,
                    trait_name,
                    methods,
                });
            }
            _ => {}
        }
        pending_attrs.clear();
    }
}

fn collect_use(
    node: tree_sitter::Node<'_>,
    prefix: &mut Vec<String>,
    reexport: bool,
    source: &[u8],
    rows: &mut ModuleResolutionRows,
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
                        collect_use(leaf, prefix, reexport, source, rows);
                    }
                } else {
                    collect_use(*child, prefix, reexport, source, rows);
                }
            }
            prefix.clear();
        }
        "use_list" => {
            for child in named_children(node) {
                collect_use(child, prefix, reexport, source, rows);
            }
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let mut segments = path_segments(node, source);
            if segments.last().map(String::as_str) == Some("self") {
                segments.pop();
                if let Some((asked, qualifier)) = segments.split_last() {
                    rows.uses.push(UseBindingRow {
                        local: asked.clone(),
                        qualifier: qualifier.to_vec(),
                        asked: asked.clone(),
                        reexport,
                    });
                }
            } else if let Some((asked, qualifier)) = segments.split_last() {
                rows.uses.push(UseBindingRow {
                    local: asked.clone(),
                    qualifier: qualifier.to_vec(),
                    asked: asked.clone(),
                    reexport,
                });
            }
        }
        "use_as_clause" => {
            let children = named_children(node);
            let clause = text(node, source);
            let mut segments = prefix.clone();
            segments.extend(
                clause
                    .split_once(" as ")
                    .map_or(clause, |(path, _)| path)
                    .trim()
                    .split("::")
                    .map(str::trim)
                    .filter(|segment| !segment.is_empty())
                    .map(str::to_owned),
            );
            if segments.last().map(String::as_str) == Some("self") {
                segments.pop();
            }
            let Some((asked, qualifier)) = segments.split_last() else {
                return;
            };
            let local = children
                .get(1)
                .map_or_else(|| asked.clone(), |alias| text(*alias, source).to_owned());
            rows.uses.push(UseBindingRow {
                local,
                qualifier: qualifier.to_vec(),
                asked: asked.clone(),
                reexport,
            });
        }
        "use_wildcard" => {
            let qualifier = if prefix.is_empty() {
                named_children(node)
                    .into_iter()
                    .flat_map(|path| path_segments(path, source))
                    .collect()
            } else {
                prefix.clone()
            };
            rows.stars.push(StarImportRow {
                qualifier,
                reexport,
            });
        }
        "identifier" | "self" => push_use_leaf(prefix, text(node, source), None, reexport, rows),
        _ => {}
    }
}

fn push_use_leaf(
    prefix: &[String],
    segment: &str,
    alias: Option<String>,
    reexport: bool,
    rows: &mut ModuleResolutionRows,
) {
    let (qualifier, asked) = if segment == "self" {
        let Some((last, rest)) = prefix.split_last() else {
            return;
        };
        (rest.to_vec(), last.clone())
    } else {
        (prefix.to_vec(), segment.to_owned())
    };
    rows.uses.push(UseBindingRow {
        local: alias.unwrap_or_else(|| asked.clone()),
        qualifier,
        asked,
        reexport,
    });
}

fn principal_ty(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "reference_type" | "pointer_type" | "parenthesized_type" => named_children(node)
            .into_iter()
            .find_map(|child| principal_ty(child, source)),
        "generic_type" => {
            let name = node
                .child_by_field_name("type")
                .and_then(|ty| path_name(ty, source))?;
            if matches!(name.as_str(), "Result" | "Option") {
                let args = node.child_by_field_name("type_arguments")?;
                named_children(args)
                    .into_iter()
                    .find(|child| is_type_node(child.kind()))
                    .and_then(|arg| principal_ty(arg, source))
            } else {
                Some(name)
            }
        }
        "trait_object" | "abstract_type" => {
            let bounds = named_children(node);
            (bounds.len() == 1)
                .then(|| path_name(bounds[0], source))
                .flatten()
        }
        _ => path_name(node, source),
    }
}

fn path_name(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let segments = path_segments(node, source);
    segments.last().cloned()
}

fn path_segments(node: tree_sitter::Node<'_>, source: &[u8]) -> Vec<String> {
    match node.kind() {
        "generic_type" => node
            .child_by_field_name("type")
            .map(|ty| path_segments(ty, source))
            .unwrap_or_default(),
        _ => text(node, source)
            .split("::")
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .map(str::to_owned)
            .collect(),
    }
}

fn path_attribute(attributes: &[String]) -> Option<String> {
    for attribute in attributes {
        let Some(inner) = attribute
            .strip_prefix("#[")
            .and_then(|s| s.strip_suffix(']'))
        else {
            continue;
        };
        let Some((key, value)) = inner.split_once('=') else {
            continue;
        };
        if key.trim() != "path" {
            continue;
        }
        let Some(value) = value
            .trim()
            .strip_prefix('\"')
            .and_then(|s| s.strip_suffix('\"'))
        else {
            continue;
        };
        return Some(value.replace("\\\"", "\"").replace("\\\\", "\\"));
    }
    None
}

fn is_type_node(kind: &str) -> bool {
    matches!(
        kind,
        "type_identifier"
            | "scoped_type_identifier"
            | "generic_type"
            | "reference_type"
            | "pointer_type"
            | "tuple_type"
            | "array_type"
            | "slice_type"
            | "function_type"
            | "trait_object"
            | "abstract_type"
    )
}

fn named_children(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).expect("Rust source is UTF-8")
}
