//! Rust type-edge candidates projected from the shared CST.

use std::collections::BTreeSet;
use std::ops::Range;

use super::type_candidate_rows::{
    TypeCandidateGroup, TypeCandidateKind as Kind, TypeCandidateOwner, TypeCandidateRow,
};

pub fn type_candidate_rows_from_tree(
    tree: &tree_sitter::Tree,
    source: &[u8],
) -> Vec<TypeCandidateGroup> {
    let mut groups = Vec::new();
    let file_types = declared_type_names(tree.root_node(), source);
    collect_items(tree.root_node(), source, &file_types, &[], &mut groups);
    groups
}

fn collect_items(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    file_types: &[String],
    shadowed: &[String],
    groups: &mut Vec<TypeCandidateGroup>,
) {
    let mut cursor = node.walk();
    for item in node.named_children(&mut cursor) {
        let first_group = groups.len();
        match item.kind() {
            "struct_item" => {
                let mut candidates = generic_candidates(item, source);
                if let Some(fields) = item.child_by_field_name("body") {
                    field_candidates(fields, source, &mut candidates);
                }
                retain_generics(item, source, &mut candidates);
                push_declared(item, source, candidates, groups);
            }
            "enum_item" => {
                let mut candidates = generic_candidates(item, source);
                let Some(name) = item.child_by_field_name("name") else {
                    continue;
                };
                let enum_name = text(name, source).to_owned();
                let mut variant_groups = Vec::new();
                if let Some(body) = item.child_by_field_name("body") {
                    let mut variants_cursor = body.walk();
                    for variant in body.named_children(&mut variants_cursor) {
                        if variant.kind() != "enum_variant" {
                            continue;
                        }
                        if let Some(variant_name) = variant.child_by_field_name("name") {
                            let variant_name = text(variant_name, source);
                            candidates.push(TypeCandidateRow {
                                to: format!("{enum_name}::{variant_name}"),
                                kind: Kind::Variant,
                            });
                        }
                        if let Some(fields) = variant.child_by_field_name("body") {
                            field_candidates(fields, source, &mut candidates);
                            let mut variant_candidates = Vec::new();
                            field_candidates(fields, source, &mut variant_candidates);
                            variant_candidates.retain(|candidate| candidate.to.contains("::"));
                            retain_generics(item, source, &mut variant_candidates);
                            if !variant_candidates.is_empty() {
                                if let Some(variant_name) = variant.child_by_field_name("name") {
                                    variant_groups.push(TypeCandidateGroup {
                                        owner: TypeCandidateOwner::Synthetic {
                                            range: span(variant_name),
                                            name: text(variant_name, source).to_owned(),
                                        },
                                        candidates: variant_candidates,
                                    });
                                }
                            }
                        }
                    }
                }
                retain_generics(item, source, &mut candidates);
                push_declared(item, source, candidates, groups);
                groups.extend(variant_groups);
            }
            "union_item" => {
                let mut candidates = generic_candidates(item, source);
                if let Some(fields) = item.child_by_field_name("body") {
                    field_candidates(fields, source, &mut candidates);
                }
                retain_generics(item, source, &mut candidates);
                push_declared(item, source, candidates, groups);
            }
            "type_item" => {
                let mut candidates = generic_candidates(item, source);
                if let Some(ty) = item.child_by_field_name("type") {
                    extend_type_refs(ty, source, Kind::Uses, &mut candidates);
                }
                retain_generics(item, source, &mut candidates);
                push_declared(item, source, candidates, groups);
            }
            "function_item" => {
                let mut candidates = generic_candidates(item, source);
                signature_candidates(item, source, &mut candidates);
                if let Some(body) = item.child_by_field_name("body") {
                    body_candidates(body, source, &mut candidates);
                }
                retain_generics(item, source, &mut candidates);
                push_declared(item, source, candidates, groups);
            }
            "trait_item" => {
                let mut candidates = generic_candidates(item, source);
                if let Some(bounds) = item.child_by_field_name("bounds") {
                    extend_type_refs(bounds, source, Kind::Generic, &mut candidates);
                }
                retain_generics(item, source, &mut candidates);
                push_declared(item, source, candidates, groups);
                if let Some(body) = item.child_by_field_name("body") {
                    let mut cursor = body.walk();
                    for method in body.named_children(&mut cursor) {
                        if !matches!(method.kind(), "function_item" | "function_signature_item") {
                            continue;
                        }
                        let mut method_candidates = generic_candidates(method, source);
                        signature_candidates(method, source, &mut method_candidates);
                        if let Some(block) = method.child_by_field_name("body") {
                            body_candidates(block, source, &mut method_candidates);
                        }
                        retain_generics(item, source, &mut method_candidates);
                        retain_generics(method, source, &mut method_candidates);
                        if method.kind() == "function_signature_item" {
                            if let Some(name) = method.child_by_field_name("name") {
                                groups.push(TypeCandidateGroup {
                                    owner: TypeCandidateOwner::Synthetic {
                                        range: span(name),
                                        name: text(name, source).to_owned(),
                                    },
                                    candidates: method_candidates,
                                });
                            }
                        } else {
                            push_declared(method, source, method_candidates, groups);
                        }
                    }
                }
            }
            "impl_item" => collect_impl(item, source, groups),
            "mod_item" => {
                if let Some(body) = item.child_by_field_name("body") {
                    let mut inner_shadowed = shadowed.to_vec();
                    for imported in external_imported_locals(body, source) {
                        if file_types.contains(&imported) && !inner_shadowed.contains(&imported) {
                            inner_shadowed.push(imported);
                        }
                    }
                    collect_items(body, source, file_types, &inner_shadowed, groups);
                }
            }
            _ => {}
        }
        for group in &mut groups[first_group..] {
            group
                .candidates
                .retain(|candidate| !shadowed.contains(&candidate.to));
        }
    }
}

fn declared_type_names(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if matches!(
            node.kind(),
            "struct_item" | "enum_item" | "union_item" | "type_item" | "trait_item"
        ) {
            if let Some(name) = node.child_by_field_name("name") {
                names.push(text(name, source).to_owned());
            }
        }
        stack.extend(named_children(node));
    }
    names.sort();
    names.dedup();
    names
}

fn external_imported_locals(module: tree_sitter::Node<'_>, source: &[u8]) -> Vec<String> {
    let mut locals = Vec::new();
    for item in named_children(module) {
        if item.kind() != "use_declaration" {
            continue;
        }
        let use_text = text(item, source).trim();
        let use_text = use_text
            .strip_prefix("pub ")
            .unwrap_or(use_text)
            .strip_prefix("use ")
            .unwrap_or(use_text)
            .trim_end_matches(';')
            .trim();
        if use_text.starts_with("crate::")
            || use_text.starts_with("self::")
            || use_text.starts_with("super::")
        {
            continue;
        }
        for imported in use_text.split(',') {
            let imported = imported.trim().trim_matches(['{', '}']);
            let local = imported
                .rsplit_once(" as ")
                .map(|(_, alias)| alias.trim())
                .unwrap_or_else(|| imported.rsplit("::").next().unwrap_or(imported).trim());
            if !local.is_empty() && local != "*" {
                locals.push(local.to_owned());
            }
        }
    }
    locals
}

fn collect_impl(item: tree_sitter::Node<'_>, source: &[u8], groups: &mut Vec<TypeCandidateGroup>) {
    let Some(self_ty) = item.child_by_field_name("type") else {
        return;
    };
    let Some(primary_name) = primary_type(self_ty, source) else {
        return;
    };
    let bare_head = bare_path_head(self_ty, source);
    let mut candidates = generic_candidates(item, source);
    if let Some(trait_ty) = item.child_by_field_name("trait") {
        if let Some(name) = path_name(trait_ty, source) {
            candidates.push(TypeCandidateRow {
                to: name,
                kind: Kind::Impl,
            });
        }
        extend_type_args(trait_ty, source, &mut candidates);
    }
    extend_type_args(self_ty, source, &mut candidates);
    retain_generics(item, source, &mut candidates);
    groups.push(TypeCandidateGroup {
        owner: TypeCandidateOwner::Impl {
            primary_name: primary_name.clone(),
            bare_head,
        },
        candidates,
    });
    let Some(body) = item.child_by_field_name("body") else {
        return;
    };
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        match child.kind() {
            "function_item" => {
                let mut candidates = generic_candidates(child, source);
                signature_candidates(child, source, &mut candidates);
                if let Some(block) = child.child_by_field_name("body") {
                    body_candidates(block, source, &mut candidates);
                    if contains_self_type(block, source) {
                        candidates.push(TypeCandidateRow {
                            to: primary_name.clone(),
                            kind: Kind::Uses,
                        });
                    }
                }
                if let Some(params) = child.child_by_field_name("parameters") {
                    let mut params_cursor = params.walk();
                    for param in params.named_children(&mut params_cursor) {
                        if param.child_by_field_name("self").is_some() {
                            continue;
                        }
                        if let Some(ty) = param.child_by_field_name("type") {
                            if contains_self_type(ty, source) {
                                candidates.push(TypeCandidateRow {
                                    to: primary_name.clone(),
                                    kind: Kind::Param,
                                });
                            }
                        }
                    }
                }
                if let Some(ret) = child.child_by_field_name("return_type") {
                    if contains_self_type(ret, source) {
                        candidates.push(TypeCandidateRow {
                            to: primary_name.clone(),
                            kind: Kind::Returns,
                        });
                    }
                }
                retain_generics(item, source, &mut candidates);
                retain_generics(child, source, &mut candidates);
                push_declared(child, source, candidates, groups);
            }
            "associated_type" | "type_item" => {
                let mut candidates = Vec::new();
                if let Some(ty) = child.child_by_field_name("type") {
                    extend_type_refs(ty, source, Kind::Uses, &mut candidates);
                }
                retain_generics(item, source, &mut candidates);
                groups.push(TypeCandidateGroup {
                    owner: TypeCandidateOwner::Synthetic {
                        range: child
                            .child_by_field_name("name")
                            .map(span)
                            .unwrap_or_else(|| span(child)),
                        name: child
                            .child_by_field_name("name")
                            .map(|name| text(name, source).to_owned())
                            .unwrap_or_default(),
                    },
                    candidates,
                });
            }
            _ => {}
        }
    }
}

fn signature_candidates(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    candidates: &mut Vec<TypeCandidateRow>,
) {
    let projections = projection_bounds(node, source);
    if let Some(params) = node.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            if let Some(ty) = param.child_by_field_name("type") {
                extend_projected_type_refs(ty, source, Kind::Param, &projections, candidates);
            }
        }
    }
    if let Some(ret) = node.child_by_field_name("return_type") {
        extend_projected_type_refs(ret, source, Kind::Returns, &projections, candidates);
    }
}

fn field_candidates(
    fields: tree_sitter::Node<'_>,
    source: &[u8],
    candidates: &mut Vec<TypeCandidateRow>,
) {
    let mut cursor = fields.walk();
    for field in fields.named_children(&mut cursor) {
        let ty = if is_type_node(field.kind()) {
            Some(field)
        } else {
            field.child_by_field_name("type")
        };
        if let Some(ty) = ty {
            extend_type_refs(ty, source, Kind::Field, candidates);
        }
    }
}

fn generic_candidates(node: tree_sitter::Node<'_>, source: &[u8]) -> Vec<TypeCandidateRow> {
    let mut candidates = Vec::new();
    if let Some(params) = node.child_by_field_name("type_parameters") {
        for param in named_children(params) {
            if param.kind() != "type_parameter" {
                continue;
            }
            if let Some(default) = param.child_by_field_name("default_type") {
                extend_type_refs(default, source, Kind::Generic, &mut candidates);
            }
            if let Some(bounds) = param.child_by_field_name("bounds") {
                generic_trait_bounds(bounds, source, &mut candidates);
            }
        }
    }
    for child in named_children(node) {
        if child.kind() == "where_clause" {
            generic_where_candidates(child, source, &mut candidates);
        }
    }
    candidates
}

fn generic_where_candidates(
    where_clause: tree_sitter::Node<'_>,
    source: &[u8],
    candidates: &mut Vec<TypeCandidateRow>,
) {
    for predicate in named_children(where_clause) {
        if predicate.kind() != "where_predicate" {
            continue;
        }
        if let Some(left) = predicate.child_by_field_name("left") {
            extend_type_refs(left, source, Kind::Generic, candidates);
        }
        if let Some(bounds) = predicate.child_by_field_name("bounds") {
            generic_trait_bounds(bounds, source, candidates);
        }
    }
}

fn generic_trait_bounds(
    bounds: tree_sitter::Node<'_>,
    source: &[u8],
    candidates: &mut Vec<TypeCandidateRow>,
) {
    for bound in named_children(bounds) {
        if bound.kind() == "function_type" {
            if let Some(trait_path) = bound.child_by_field_name("trait") {
                if let Some(name) = path_name(trait_path, source) {
                    candidates.push(TypeCandidateRow {
                        to: name,
                        kind: Kind::Generic,
                    });
                }
            }
            let mut refs = Vec::new();
            if let Some(params) = bound.child_by_field_name("parameters") {
                extend_type_refs(params, source, Kind::Generic, &mut refs);
            }
            if let Some(ret) = bound.child_by_field_name("return_type") {
                extend_type_refs(ret, source, Kind::Generic, &mut refs);
            }
            refs.sort_by(|a, b| a.to.cmp(&b.to));
            refs.dedup_by(|a, b| a.to == b.to && a.kind == b.kind);
            candidates.extend(refs);
            continue;
        }
        if let Some(name) = path_name(bound, source) {
            candidates.push(TypeCandidateRow {
                to: name,
                kind: Kind::Generic,
            });
        }
        let mut stack = vec![bound];
        while let Some(node) = stack.pop() {
            if node.kind() == "type_arguments" {
                extend_type_argument_refs(node, source, Kind::Generic, candidates);
                continue;
            }
            stack.extend(named_children(node));
        }
    }
}

fn body_candidates(
    body: tree_sitter::Node<'_>,
    source: &[u8],
    candidates: &mut Vec<TypeCandidateRow>,
) {
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "closure_expression" => continue,
            "struct_expression" => {
                if let Some(path) = node.child_by_field_name("name") {
                    let name = text(path, source);
                    if !name.contains("::") && name != "Self" {
                        candidates.push(TypeCandidateRow {
                            to: name.to_owned(),
                            kind: Kind::Uses,
                        });
                    }
                }
            }
            "let_declaration" => {
                if let Some(ty) = node.child_by_field_name("type") {
                    extend_type_refs(ty, source, Kind::Uses, candidates);
                }
            }
            "type_arguments" => extend_type_argument_refs(node, source, Kind::Uses, candidates),
            _ => {}
        }
        let mut cursor = node.walk();
        let mut children = node.named_children(&mut cursor).collect::<Vec<_>>();
        children.reverse();
        stack.extend(children);
    }
}

fn extend_type_refs(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    kind: Kind,
    out: &mut Vec<TypeCandidateRow>,
) {
    let mut names = BTreeSet::new();
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if node.kind() == "type_binding" {
            if let Some(ty) = node.child_by_field_name("type") {
                stack.push(ty);
            }
            continue;
        }
        if node.kind() == "scoped_type_identifier" {
            if let Some(path) = node.child_by_field_name("path") {
                if path.kind() == "bracketed_type" {
                    if let Some(qualified) = named_children(path)
                        .into_iter()
                        .find(|child| child.kind() == "qualified_type")
                    {
                        if let Some(ty) = qualified.child_by_field_name("type") {
                            let name = text(ty, source);
                            if !is_primitive(name) {
                                names.insert(name.to_owned());
                            }
                        }
                        if let (Some(trait_ty), Some(slot)) = (
                            qualified.child_by_field_name("alias"),
                            node.child_by_field_name("name"),
                        ) {
                            names.insert(format!(
                                "{}::{}",
                                text(trait_ty, source),
                                text(slot, source)
                            ));
                        }
                        continue;
                    }
                }
            }
        }
        if matches!(node.kind(), "type_identifier" | "scoped_type_identifier") {
            let name = text(node, source);
            if !is_primitive(name) {
                names.insert(name.to_owned());
            }
            if node.kind() == "scoped_type_identifier" {
                continue;
            }
        }
        let mut cursor = node.walk();
        let mut children = node.named_children(&mut cursor).collect::<Vec<_>>();
        children.reverse();
        stack.extend(children);
    }
    out.extend(names.into_iter().map(|to| TypeCandidateRow { to, kind }));
}

fn extend_type_argument_refs(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    kind: Kind,
    out: &mut Vec<TypeCandidateRow>,
) {
    for argument in named_children(node) {
        if argument.kind() == "type_binding" {
            if let Some(ty) = argument.child_by_field_name("type") {
                extend_type_refs(ty, source, kind, out);
            }
            continue;
        }
        if is_type_node(argument.kind()) {
            extend_type_refs(argument, source, kind, out);
        }
    }
}

fn extend_projected_type_refs(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    kind: Kind,
    projections: &[(String, String)],
    out: &mut Vec<TypeCandidateRow>,
) {
    let mut rows = Vec::new();
    extend_type_refs(node, source, kind, &mut rows);
    out.extend(rows.into_iter().map(|mut row| {
        if let Some((head, slot)) = row.to.split_once("::") {
            if let Some((_, trait_name)) = projections.iter().find(|(param, _)| param == head) {
                row.to = format!("{trait_name}::{slot}");
            }
        }
        row
    }));
}

fn projection_bounds(node: tree_sitter::Node<'_>, source: &[u8]) -> Vec<(String, String)> {
    let mut bounds = Vec::new();
    if let Some(params) = node.child_by_field_name("type_parameters") {
        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            if param.kind() != "type_parameter" {
                continue;
            }
            let Some(name) = param.child_by_field_name("name") else {
                continue;
            };
            let Some(bound) = param.child_by_field_name("bounds") else {
                continue;
            };
            if let Some(trait_name) = first_trait_name(bound, source) {
                bounds.push((text(name, source).to_owned(), trait_name));
            }
        }
    }
    if let Some(where_clause) = node.child_by_field_name("where_clause") {
        let mut cursor = where_clause.walk();
        for predicate in where_clause.named_children(&mut cursor) {
            if predicate.kind() != "where_predicate" {
                continue;
            }
            let Some(bounded) = predicate.child_by_field_name("left") else {
                continue;
            };
            if bounded.kind() != "type_identifier" {
                continue;
            }
            let Some(bounds_node) = predicate.child_by_field_name("bounds") else {
                continue;
            };
            if let Some(trait_name) = first_trait_name(bounds_node, source) {
                bounds.push((text(bounded, source).to_owned(), trait_name));
            }
        }
    }
    bounds
}

fn first_trait_name(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if matches!(current.kind(), "type_identifier" | "scoped_type_identifier") {
            let name = text(current, source);
            if !is_primitive(name) {
                return Some(name.rsplit("::").next().unwrap_or(name).to_owned());
            }
            continue;
        }
        let mut cursor = current.walk();
        let children: Vec<_> = current.named_children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    None
}

fn extend_type_args(node: tree_sitter::Node<'_>, source: &[u8], out: &mut Vec<TypeCandidateRow>) {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if node.kind() == "type_arguments" {
            extend_type_argument_refs(node, source, Kind::Generic, out);
            continue;
        }
        let mut cursor = node.walk();
        let mut children = node.named_children(&mut cursor).collect::<Vec<_>>();
        children.reverse();
        stack.extend(children);
    }
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
    matches!(current.kind(), "type_identifier" | "scoped_type_identifier").then(|| {
        text(current, source)
            .rsplit("::")
            .next()
            .unwrap_or_default()
            .to_owned()
    })
}

fn path_name(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let path = if node.kind() == "generic_type" {
        node.child_by_field_name("type")?
    } else {
        node
    };
    matches!(path.kind(), "type_identifier" | "scoped_type_identifier")
        .then(|| text(path, source).trim_start_matches("::").to_owned())
}

fn bare_path_head(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<(Range<u32>, String)> {
    let head = if node.kind() == "generic_type" {
        node.child_by_field_name("type")?
    } else {
        node
    };
    (head.kind() == "type_identifier").then(|| (span(head), text(head, source).to_owned()))
}

fn contains_self_type(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.kind() == "struct_expression" {
            let name = current.child_by_field_name("name");
            let mut cursor = current.walk();
            stack.extend(current.named_children(&mut cursor).filter(|child| {
                name.map_or(true, |name| {
                    child.start_byte() != name.start_byte() || child.end_byte() != name.end_byte()
                })
            }));
            continue;
        }
        if matches!(current.kind(), "type_identifier" | "scoped_type_identifier")
            && text(current, source) == "Self"
        {
            return true;
        }
        let mut cursor = current.walk();
        stack.extend(current.named_children(&mut cursor));
    }
    false
}

fn retain_generics(node: tree_sitter::Node<'_>, source: &[u8], rows: &mut Vec<TypeCandidateRow>) {
    let mut names = BTreeSet::new();
    if let Some(params) = node.child_by_field_name("type_parameters") {
        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            if param.kind() == "type_parameter" {
                if let Some(name) = param.child_by_field_name("name") {
                    names.insert(text(name, source).to_owned());
                }
            }
        }
    }
    rows.retain(|row| !names.contains(&row.to));
}

fn push_declared(
    node: tree_sitter::Node<'_>,
    _source: &[u8],
    candidates: Vec<TypeCandidateRow>,
    groups: &mut Vec<TypeCandidateGroup>,
) {
    if let Some(name) = node.child_by_field_name("name") {
        groups.push(TypeCandidateGroup {
            owner: TypeCandidateOwner::Declared(span(name)),
            candidates,
        });
    }
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

fn span(node: tree_sitter::Node<'_>) -> Range<u32> {
    node.start_byte() as u32..node.end_byte() as u32
}

fn named_children(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).expect("Rust type syntax is UTF-8")
}

fn is_primitive(name: &str) -> bool {
    matches!(
        name,
        "_" | "Self"
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
    )
}
