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
    collect_items(tree.root_node(), source, &mut groups);
    groups
}

fn collect_items(node: tree_sitter::Node<'_>, source: &[u8], groups: &mut Vec<TypeCandidateGroup>) {
    let mut cursor = node.walk();
    for item in node.named_children(&mut cursor) {
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
                            retain_generics(item, source, &mut variant_candidates);
                            if !variant_candidates.is_empty() {
                                if let Some(variant_name) = variant.child_by_field_name("name") {
                                    groups.push(TypeCandidateGroup {
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
                        push_declared(method, source, method_candidates, groups);
                    }
                }
            }
            "impl_item" => collect_impl(item, source, groups),
            "mod_item" => {
                if let Some(body) = item.child_by_field_name("body") {
                    collect_items(body, source, groups);
                }
            }
            _ => {}
        }
    }
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
        if let Some(name) = primary_type(trait_ty, source) {
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
            "associated_type" => {
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
    if let Some(params) = node.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            if let Some(ty) = param.child_by_field_name("type") {
                extend_type_refs(ty, source, Kind::Param, candidates);
            }
        }
    }
    if let Some(ret) = node.child_by_field_name("return_type") {
        extend_type_refs(ret, source, Kind::Returns, candidates);
    }
}

fn field_candidates(
    fields: tree_sitter::Node<'_>,
    source: &[u8],
    candidates: &mut Vec<TypeCandidateRow>,
) {
    let mut cursor = fields.walk();
    for field in fields.named_children(&mut cursor) {
        if let Some(ty) = field.child_by_field_name("type") {
            extend_type_refs(ty, source, Kind::Field, candidates);
        }
    }
}

fn generic_candidates(node: tree_sitter::Node<'_>, source: &[u8]) -> Vec<TypeCandidateRow> {
    let mut candidates = Vec::new();
    if let Some(params) = node.child_by_field_name("type_parameters") {
        extend_type_refs(params, source, Kind::Generic, &mut candidates);
    }
    if let Some(where_clause) = node.child_by_field_name("where_clause") {
        extend_type_refs(where_clause, source, Kind::Generic, &mut candidates);
    }
    candidates
}

fn body_candidates(
    body: tree_sitter::Node<'_>,
    source: &[u8],
    candidates: &mut Vec<TypeCandidateRow>,
) {
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "struct_expression" => {
                if let Some(path) = node.child_by_field_name("name") {
                    let name = text(path, source);
                    if !name.contains("::") {
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
            "type_arguments" => extend_type_refs(node, source, Kind::Uses, candidates),
            _ => {}
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
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
        if matches!(node.kind(), "type_identifier" | "scoped_type_identifier") {
            let name = text(node, source);
            if !is_primitive(name) {
                names.insert(name.to_owned());
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    out.extend(names.into_iter().map(|to| TypeCandidateRow { to, kind }));
}

fn extend_type_args(node: tree_sitter::Node<'_>, source: &[u8], out: &mut Vec<TypeCandidateRow>) {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if node.kind() == "type_arguments" {
            extend_type_refs(node, source, Kind::Generic, out);
            continue;
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
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

fn span(node: tree_sitter::Node<'_>) -> Range<u32> {
    node.start_byte() as u32..node.end_byte() as u32
}

fn text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).expect("Rust type syntax is UTF-8")
}

fn is_primitive(name: &str) -> bool {
    matches!(
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
    )
}
