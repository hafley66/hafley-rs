//! Rust call-site and call-metadata projections over the shared CST.

use std::collections::HashSet;
use std::ops::Range;

use super::call_metadata_rows::{CallCfgRow, CallOwnerRow};
use super::call_site_rows::{CallSiteRow, CallSiteRows, ConstInitRow};

pub fn call_site_rows_from_tree(
    tree: &tree_sitter::Tree,
    source: &[u8],
    def_ranges: &[Range<u32>],
) -> CallSiteRows {
    let mut out = CallSiteRows::default();
    walk_sites(tree.root_node(), source, None, false, def_ranges, &mut out);
    let shipped: HashSet<&str> = out
        .sites
        .iter()
        .filter(|site| site.cfg.is_none())
        .map(|site| site.callee.as_str())
        .collect();
    let mut seen = HashSet::new();
    out.test_only_calls = out
        .sites
        .iter()
        .filter_map(|site| site.cfg.as_ref().map(|cfg| (&site.callee, cfg)))
        .filter(|(callee, _)| !shipped.contains(callee.as_str()))
        .filter(|(callee, _)| seen.insert(callee.as_str()))
        .map(|(callee, cfg)| (callee.clone(), cfg.clone()))
        .collect();
    out
}

fn walk_sites(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    inherited_cfg: Option<String>,
    in_block: bool,
    defs: &[Range<u32>],
    out: &mut CallSiteRows,
) {
    let cfg = inherited_cfg.or_else(|| cfg_predicate(node, source));
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
                        cfg: cfg.clone(),
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
                        cfg: cfg.clone(),
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
        walk_sites(child, source, cfg.clone(), item_block, defs, out);
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
) -> (Vec<CallCfgRow>, Vec<CallOwnerRow>) {
    let mut cfgs = Vec::new();
    let mut owners = Vec::new();
    walk_metadata(
        tree.root_node(),
        source,
        None,
        None,
        def_ranges,
        &mut cfgs,
        &mut owners,
    );
    (cfgs, owners)
}

fn walk_metadata(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    inherited_cfg: Option<String>,
    inherited_owner: Option<(Option<String>, Option<String>)>,
    defs: &[(u32, u32)],
    cfgs: &mut Vec<CallCfgRow>,
    owners: &mut Vec<CallOwnerRow>,
) {
    let cfg = inherited_cfg.or_else(|| cfg_predicate(node, source));
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
                if let Some(predicate) = cfg.as_ref() {
                    cfgs.push(CallCfgRow {
                        start: range.0,
                        end: range.1,
                        predicate: predicate.clone(),
                    });
                }
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
    if node.kind() == "enum_variant" {
        if let (Some(name), Some(predicate)) = (node.child_by_field_name("name"), cfg.as_ref()) {
            let start = name.start_byte() as u32;
            let end = name.end_byte() as u32;
            if defs.contains(&(start, end)) {
                cfgs.push(CallCfgRow {
                    start,
                    end,
                    predicate: predicate.clone(),
                });
            }
        }
    }
    if matches!(node.kind(), "const_item" | "static_item") {
        if let (Some(name), Some(value), Some(predicate)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("value"),
            cfg.as_ref(),
        ) {
            let range = (name.start_byte() as u32, value.end_byte() as u32);
            if defs.contains(&range) {
                cfgs.push(CallCfgRow {
                    start: range.0,
                    end: range.1,
                    predicate: predicate.clone(),
                });
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_metadata(
            child,
            source,
            cfg.clone(),
            owner.clone(),
            defs,
            cfgs,
            owners,
        );
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
    let path = path_text(current, source);
    let callee = path.rsplit("::").next()?.trim().to_owned();
    Some((span(current), callee, path.contains("::").then_some(path)))
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

fn cfg_predicate(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let mut previous = node.prev_named_sibling();
    while let Some(attribute) = previous {
        if attribute.kind() != "attribute_item" {
            break;
        }
        let attr = text(attribute, source).trim();
        let body = attr.strip_prefix("#[cfg(")?.strip_suffix(")]")?;
        if body
            .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
            .any(|word| word == "test")
        {
            return Some(normalize_tokens(body));
        }
        previous = attribute.prev_named_sibling();
    }
    None
}

fn normalize_tokens(body: &str) -> String {
    let mut out = String::new();
    let mut chars = body.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
        } else if ch == '(' {
            if out
                .chars()
                .last()
                .is_some_and(|prev| prev.is_alphanumeric() || prev == '_')
            {
                out.push(' ');
            }
            out.push(ch);
        } else if ch == ',' || ch == '=' {
            while out.ends_with(' ') {
                out.pop();
            }
            out.push(' ');
            out.push(ch);
            out.push(' ');
            while chars.peek().is_some_and(|next| next.is_whitespace()) {
                chars.next();
            }
        } else {
            out.push(ch);
        }
    }
    out
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
