//! Rust call-site and call-metadata projections over the shared CST.

use std::ops::Range;

use super::call_metadata_rows::CallOwnerRow;
use super::call_site_rows::{CallSiteRow, CallSiteRows, ConstInitRow};

pub fn call_site_rows_from_tree(
    tree: &tree_sitter::Tree,
    source: &[u8],
    def_ranges: &[Range<u32>],
) -> CallSiteRows {
    let mut out = CallSiteRows::default();
    walk_sites(tree.root_node(), source, false, def_ranges, &mut out);
    out
}

fn walk_sites(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    in_block: bool,
    defs: &[Range<u32>],
    out: &mut CallSiteRows,
) {
    let item_block = in_block || node.kind() == "block";
    let const_item = if !in_block && matches!(node.kind(), "const_item" | "static_item") {
        node.child_by_field_name("name")
            .zip(node.child_by_field_name("value"))
            .map(|(name, value)| (name, value))
    } else {
        None
    };
    let mark = out.sites.len();
    match node.kind() {
        "call_expression" => {
            if let Some(function) = node.child_by_field_name("function") {
                if let Some((range, callee, callee_path)) = call_target(function, source) {
                    out.sites.push(CallSiteRow {
                        range,
                        callee,
                        callee_path,
                    });
                }
            }
        }
        "struct_expression" => {
            if let Some(path) = node.child_by_field_name("name") {
                if let Some((range, callee, callee_path)) = struct_target(path, source) {
                    out.sites.push(CallSiteRow {
                        range,
                        callee,
                        callee_path,
                    });
                }
            }
        }
        "let_declaration" => {
            if let (Some(ty), Some(value)) = (
                node.child_by_field_name("type"),
                node.child_by_field_name("value"),
            ) {
                if is_default_call(value, source) {
                    if let Some(function) = value.child_by_field_name("function") {
                        out.expected_types
                            .push((span(function), text(ty, source).to_owned()));
                    }
                }
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_sites(child, source, item_block, defs, out);
    }
    if let Some((name, value)) = const_item {
        let start = name.start_byte() as u32;
        let end = value.end_byte() as u32;
        if out.sites[mark..]
            .iter()
            .filter(|site| start <= site.range.start && site.range.end <= end)
            .any(|site| {
                !defs
                    .iter()
                    .any(|range| range.start <= site.range.start && site.range.end <= range.end)
            })
        {
            out.const_inits.push(ConstInitRow {
                range: start..end,
                name: text(name, source).to_owned(),
            });
        }
    }
}

pub fn call_metadata_rows_from_tree(
    tree: &tree_sitter::Tree,
    source: &[u8],
    def_ranges: &[(u32, u32)],
) -> Vec<CallOwnerRow> {
    let mut owners = Vec::new();
    walk_metadata(tree.root_node(), source, None, def_ranges, &mut owners);
    owners
}

fn walk_metadata(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    inherited_owner: Option<(Option<String>, Option<String>)>,
    defs: &[(u32, u32)],
    owners: &mut Vec<CallOwnerRow>,
) {
    let owner = match node.kind() {
        "impl_item" => Some((
            node.child_by_field_name("type")
                .and_then(|ty| primary_type(ty, source)),
            node.child_by_field_name("trait")
                .and_then(|ty| Some(path_text(ty, source))),
        )),
        "trait_item" => Some((
            None,
            node.child_by_field_name("name")
                .map(|name| text(name, source).to_owned()),
        )),
        _ => inherited_owner,
    };
    if matches!(node.kind(), "function_item" | "function_signature_item") {
        if let Some(name) = node.child_by_field_name("name") {
            let end = node.child_by_field_name("body").map_or_else(
                || {
                    if node.kind() == "function_signature_item" {
                        node.end_byte().saturating_sub(1)
                    } else {
                        node.end_byte()
                    }
                },
                |body| body.end_byte(),
            );
            let range = (name.start_byte() as u32, end as u32);
            if defs.contains(&range) {
                if let Some((self_type, trait_name)) = owner.clone() {
                    owners.push(CallOwnerRow {
                        start: range.0,
                        end: range.1,
                        self_type,
                        trait_name,
                    });
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_metadata(child, source, owner.clone(), defs, owners);
    }
}

fn call_target(
    node: tree_sitter::Node<'_>,
    source: &[u8],
) -> Option<(Range<u32>, String, Option<String>)> {
    let mut current = node;
    while current.kind() == "parenthesized_expression" {
        current = current.named_child(0)?;
    }
    if current.kind() == "field_expression" {
        let field = current.child_by_field_name("field")?;
        return Some((span(field), text(field, source).to_owned(), None));
    }
    let path = call_path_text(current, source);
    let callee = path.rsplit("::").next()?.trim().to_owned();
    Some((span(current), callee, path.contains("::").then_some(path)))
}

fn call_path_text(node: tree_sitter::Node<'_>, source: &[u8]) -> String {
    let path = path_text(node, source);
    let Some(qualified) = path.strip_prefix('<') else {
        return path;
    };
    let Some((self_or_trait, suffix)) = qualified.split_once(">::") else {
        return path;
    };
    let suffix = suffix.trim();
    if let Some((_, trait_name)) = self_or_trait.split_once(" as ") {
        format!("{}::{suffix}", trait_name.trim())
    } else {
        suffix.to_owned()
    }
}

fn struct_target(
    node: tree_sitter::Node<'_>,
    source: &[u8],
) -> Option<(Range<u32>, String, Option<String>)> {
    let path = path_text(node, source);
    let segments: Vec<&str> = path.split("::").collect();
    let callee = (*segments.last()?).trim();
    if segments.len() >= 2
        && segments[segments.len() - 2]
            .chars()
            .next()
            .is_some_and(char::is_uppercase)
    {
        return None;
    }
    Some((
        span(node),
        callee.to_owned(),
        (segments.len() > 1).then_some(path),
    ))
}

fn is_default_call(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    node.child_by_field_name("function")
        .is_some_and(|function| path_text(function, source) == "Default::default")
}

fn primary_type(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let mut current = node;
    while matches!(
        current.kind(),
        "reference_type" | "pointer_type" | "generic_type"
    ) {
        current = current
            .child_by_field_name("type")
            .or_else(|| current.named_child(0))?;
    }
    Some(path_text(current, source).rsplit("::").next()?.to_owned())
}

fn path_text(node: tree_sitter::Node<'_>, source: &[u8]) -> String {
    text(node, source)
        .trim()
        .trim_start_matches("::")
        .to_owned()
}

fn span(node: tree_sitter::Node<'_>) -> Range<u32> {
    node.start_byte() as u32..node.end_byte() as u32
}

fn text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).expect("Rust call syntax is UTF-8")
}
