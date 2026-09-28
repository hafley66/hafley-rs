//! Compiler-backed TypeScript rename. Syntax selects candidate seats; checker
//! identity filters them. Missing checker queries yield named abstentions.

#[cfg(test)]
use std::path::PathBuf;

use serde_json::{json, Value};
use tree_sitter::{Node, Parser, Tree};

use super::ts7_api::{utf16_offset, Ts7Api};
use crate::edit_seams::{RefRole, RenameAbstain, RenameStop, SymbolRef};
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::Span;

pub fn symbol_refs_and_abstains(
    cx: &RenameCx,
    request: &RenameRequest,
) -> Result<(Vec<SymbolRef>, Vec<RenameAbstain>), RenameStop> {
    let text = cx.text(&request.anchor).ok_or_else(|| ts7_not_found(request))?;
    let seed = seed_offset(&text, request).ok_or_else(|| ts7_not_found(request))?;
    let span = Span { start: seed as u32, len: request.old.len() as u32 };
    let file = cx.abs(&request.anchor);
    let mut api = Ts7Api::open(cx.root(), &file).map_err(|_| inexact(request, span))?;
    let position = utf16_offset(&text, seed + request.old.len() / 2);
    let symbol = api.symbol_at(&file, position).map_err(|_| inexact(request, span))?;
    let anchor_tree = parse_tree(&request.anchor, &text).ok_or_else(|| inexact(request, span))?;
    let anchor_node = anchor_tree.root_node().descendant_for_byte_range(seed, seed + request.old.len())
        .ok_or_else(|| inexact(request, span))?;
    let seed_kind = anchor_node.kind();
    let intersection_context = has_node_kind(anchor_tree.root_node(), "intersection_type");
    if let Some(query) = missing_for_node(anchor_node, seed_kind, intersection_context) {
        return abstain(request, span, query);
    }
    if symbol["id"].as_u64().is_none() { return Err(ts7_not_found(request)); }

    let seed_alias = symbol["flags"].as_u64().unwrap_or(0) & 2_097_152 != 0;
    let seed_export_target = if seed_alias && anchor_node.parent().is_some_and(|node| node.kind() == "export_specifier") {
        export_local_target(&mut api, &symbol).map_err(|_| inexact(request, span))?
    } else {
        None
    };
    let seed_id = if seed_alias {
        symbol["id"].as_u64().ok_or_else(|| inexact(request, span))?
    } else {
        canonical_symbol(&mut api, &symbol).map_err(|_| inexact(request, span))?
    };
    let mut refs = Vec::new();
    for rel in cx.files_of(&crate::lang::ts::TsSource) {
        let Some(content) = cx.text(rel) else { continue };
        let path = cx.abs(rel);
        let tree = parse_tree(rel, &content).ok_or_else(|| inexact(request, span))?;
        let candidates: Vec<usize> = content.match_indices(&request.old)
            .map(|(at, _)| at)
            .filter(|at| identifier_edges(&content, *at, request.old.len()))
            .collect();
        for at in candidates {
            let position = utf16_offset(&content, at + request.old.len() / 2);
            let found = api.symbol_at(&path, position).map_err(|_| inexact(request, span))?;
            if found["id"].as_u64().is_none() { continue }
            let found_id = if seed_alias {
                found["id"].as_u64().ok_or_else(|| inexact(request, span))?
            } else {
                canonical_symbol(&mut api, &found).map_err(|_| inexact(request, span))?
            };
            let node = tree.root_node().descendant_for_byte_range(at, at + request.old.len())
                .ok_or_else(|| inexact(request, span))?;
            let shorthand = node.kind() == "shorthand_property_identifier";
            let binding_shorthand = node.kind() == "shorthand_property_identifier_pattern";
            if rel == request.anchor {
                if let Some(query) = missing_for_node(node, seed_kind, intersection_context) {
                    return abstain(request, span, query);
                }
            }
            let related_export = if found_id != seed_id && seed_export_target.is_some()
                && node.parent().is_some_and(|node| node.kind() == "export_specifier")
            {
                export_local_target(&mut api, &found).map_err(|_| inexact(request, span))?
                    == seed_export_target
            } else {
                false
            };
            if found_id != seed_id && !shorthand && !related_export { continue; }
            if let Some(query) = missing_for_node(node, seed_kind, intersection_context) {
                return abstain(request, span, query);
            }
            let mut replacement = None;
            if binding_shorthand && found_id == seed_id {
                replacement = Some(format!("{}: {}", request.old, request.new));
            }
            if shorthand {
                let parent = found["declarations"].as_array()
                    .and_then(|rows| rows.iter().find_map(Value::as_str))
                    .ok_or_else(|| inexact(request, span))?;
                let value = api.checker("getShorthandAssignmentValueSymbol", json!({"location": parent}))
                    .map_err(|_| inexact(request, span))?;
                if value["id"].as_u64() == Some(seed_id) {
                    replacement = Some(format!("{}: {}", request.old, request.new));
                } else if found_id == seed_id {
                    replacement = Some(format!("{}: {}", request.new, request.old));
                }
            }
            if found_id != seed_id && replacement.is_none() && !related_export { continue; }
            let site_span = Span { start: at as u32, len: request.old.len() as u32 };
            if seed_alias && node.parent().is_some_and(|parent|
                parent.kind() == "import_specifier" && parent.child_by_field_name("alias").is_none())
            {
                replacement = Some(format!("{} as {}", request.old, request.new));
            }
            if node.kind() == "number" && node.parent().is_some_and(|parent| parent.kind() == "subscript_expression") {
                replacement = Some(format!("\"{}\"", request.new));
            }
            if let Some(replacement) = replacement { cx.put_ts_slow_edit(rel, site_span, replacement); }
            refs.push(SymbolRef {
                file: rel.to_owned(),
                span: site_span,
                role: RefRole::Read,
                text: request.old.clone(),
            });
        }
    }
    if refs.is_empty() { return Err(ts7_not_found(request)); }
    Ok((refs, Vec::new()))
}

fn export_local_target(api: &mut Ts7Api, symbol: &Value) -> Result<Option<u64>, String> {
    let Some(handle) = symbol["declarations"].as_array()
        .and_then(|rows| rows.iter().filter_map(Value::as_str).find(|handle|
            handle.split('.').nth(1) == Some("282"))) else { return Ok(None) };
    let target = api.checker("getExportSpecifierLocalTargetSymbol", json!({"location": handle}))?;
    Ok(target["id"].as_u64())
}

fn abstain(request: &RenameRequest, span: Span, query: &'static str)
    -> Result<(Vec<SymbolRef>, Vec<RenameAbstain>), RenameStop>
{
    let abstain = RenameAbstain {
        file: request.anchor.clone(),
        span,
        symbol: request.old.clone(),
        reason: "ts7_api_missing_checker_query",
        receiver: query.to_owned(),
    };
    Ok((Vec::new(), vec![abstain]))
}

fn canonical_symbol(api: &mut Ts7Api, symbol: &Value) -> Result<u64, String> {
    let id = symbol["id"].as_u64().ok_or("symbol without id")?;
    // SymbolFlags.Alias. The compiler owns alias resolution; Rust chooses which
    // candidate seats to ask about and which spelling belongs to the edit.
    if symbol["flags"].as_u64().unwrap_or(0) & 2_097_152 != 0 {
        let target = api.checker("getAliasedSymbol", json!({"symbol": id}))?;
        return target["id"].as_u64().ok_or("alias target without id".into());
    }
    Ok(id)
}

fn seed_offset(text: &str, request: &RenameRequest) -> Option<usize> {
    match request.at {
        Some(at) => text.match_indices(&request.old)
            .map(|(start, _)| start)
            .find(|start| *start <= at as usize && (at as usize) < *start + request.old.len()),
        None => text.match_indices(&request.old)
            .map(|(start, _)| start)
            .find(|start| identifier_edges(text, *start, request.old.len())),
    }
}

fn identifier_edges(text: &str, at: usize, len: usize) -> bool {
    fn ident(byte: u8) -> bool { byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' }
    let bytes = text.as_bytes();
    (at == 0 || !ident(bytes[at - 1])) &&
        (at + len == bytes.len() || !ident(bytes[at + len]))
}

fn parse_tree(rel: &str, content: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    let language = if rel.ends_with(".tsx") {
        tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TSX)
    } else {
        tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT)
    };
    parser.set_language(&language).ok()?;
    parser.parse(content, None)
}

fn has_node_kind(node: Node<'_>, wanted: &str) -> bool {
    if node.kind() == wanted { return true; }
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).any(|child| has_node_kind(child, wanted));
    found
}

fn missing_for_node(node: Node<'_>, seed_kind: &str, intersection_context: bool) -> Option<&'static str> {
    let leaf_kind = node.kind();
    if leaf_kind == "string_fragment" {
        let mut current = node.parent();
        while let Some(ancestor) = current {
            if matches!(ancestor.kind(), "property_signature" | "subscript_expression") { return None; }
            current = ancestor.parent();
        }
    }
    let mut current = Some(node);
    while let Some(node) = current {
        match node.kind() {
            "object_pattern" if leaf_kind == "property_identifier"
                || (leaf_kind == "shorthand_property_identifier_pattern"
                    && seed_kind != "shorthand_property_identifier_pattern") => {
                return Some("getPropertySymbolFromBindingElement")
            }
            "interface_declaration" | "class_declaration" | "class_body" => return Some("getRootSymbols"),
            "type_alias_declaration" if matches!(leaf_kind, "property_identifier" | "string_fragment") => {
                return Some(if intersection_context { "getRootSymbols" } else { "getPropertySymbolsFromContextualType" })
            }
            "string" | "string_fragment" => return Some("getContextualTypeFromParentOrAncestorTypeNode"),
            _ => {}
        }
        current = node.parent();
    }
    None
}

fn ts7_not_found(request: &RenameRequest) -> RenameStop {
    RenameStop::NotFound { anchor: request.anchor.clone(), old: request.old.clone() }
}

fn inexact(request: &RenameRequest, span: Span) -> RenameStop {
    RenameStop::Inexact { file: request.anchor.clone(), span, why: "ts7_api" }
}

#[cfg(test)]
fn byte_at_lsp_position(text: &str, position: &Value) -> Result<usize, String> {
    let line = position["line"].as_u64().ok_or("position without line")? as usize;
    let character = position["character"].as_u64().ok_or("position without character")? as usize;
    let mut start = 0;
    for _ in 0..line {
        let next = text[start..].find('\n').ok_or("line out of range")?;
        start += next + 1;
    }
    let mut utf16 = 0;
    for (relative, ch) in text[start..].char_indices() {
        if utf16 == character { return Ok(start + relative); }
        if ch == '\n' { break; }
        utf16 += ch.len_utf16();
    }
    if utf16 == character { return Ok(start + text[start..].find('\n').unwrap_or(text.len() - start)); }
    Err("UTF-16 position splits a character or exceeds the line".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_oracle(source: &str, old: &str, at: u32) {
        assert_oracle_or_abstain(source, old, at, None);
    }

    fn assert_oracle_or_abstain(source: &str, old: &str, at: u32, missing_query: Option<&str>) {
        let cx = RenameCx::open(&ts7_rename_fixture()).unwrap().with_slow(true);
        let new = if old.starts_with(char::is_uppercase) { "Next" } else { "next" };
        let request = RenameRequest { anchor: source.into(), old: old.into(), new: new.into(), at: Some(at) };
        let (refs, abstains) = symbol_refs_and_abstains(&cx, &request).unwrap();
        if let Some(query) = missing_query {
            assert!(refs.is_empty());
            assert_eq!(abstains.len(), 1);
            assert_eq!(abstains[0].reason, "ts7_api_missing_checker_query");
            assert_eq!(abstains[0].receiver, query);
        } else {
            assert!(abstains.is_empty());
        }
        let mut actual: Vec<_> = refs.iter().map(|reference| (
            reference.file.clone(), reference.span.start as usize, reference.span.end() as usize,
            cx.ts_slow_edit(&reference.file, reference.span).unwrap_or_else(|| request.new.clone()),
        )).collect();
        actual.sort();
        let stem = source.strip_suffix(".tsx").or_else(|| source.strip_suffix(".ts")).unwrap();
        let named_oracle = ts7_rename_fixture().join(format!("{stem}.{old}.rename.json"));
        let oracle_file = if named_oracle.is_file() { named_oracle } else {
            ts7_rename_fixture().join(format!("{stem}.rename.json"))
        };
        let oracle: Value = serde_json::from_str(&std::fs::read_to_string(crate::io_path(&oracle_file)).unwrap()).unwrap();
        if missing_query.is_some() {
            assert!(oracle["changes"].is_object());
            return;
        }
        if oracle.is_null() {
            assert!(actual.is_empty());
            return;
        }
        let mut expected = Vec::new();
        for (file, edits) in oracle["changes"].as_object().unwrap() {
            let text = cx.text(file).unwrap();
            for edit in edits.as_array().unwrap() {
                let start = byte_at_lsp_position(&text, &edit["range"]["start"]).unwrap();
                let end = byte_at_lsp_position(&text, &edit["range"]["end"]).unwrap();
                expected.push((file.clone(), start, end, edit["newText"].as_str().unwrap().to_owned()));
            }
        }
        expected.sort();
        assert_eq!(actual, expected);
    }

    fn ts7_rename_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts7_api")
    }

    #[test]
    fn slow_rename_plain_identifier_uses_checker_identity() {
        let cx = RenameCx::open(&ts7_rename_fixture()).unwrap().with_slow(true);
        let request = RenameRequest { anchor: "0_fixture.ts".into(), old: "old".into(), new: "next".into(), at: Some(6) };
        let (refs, abstains) = symbol_refs_and_abstains(&cx, &request).unwrap();
        assert_eq!(refs.len(), 2);
        assert!(abstains.is_empty());
    }

    #[test]
    fn slow_rename_shorthand_uses_value_symbol_query() {
        let cx = RenameCx::open(&ts7_rename_fixture()).unwrap().with_slow(true);
        let request = RenameRequest { anchor: "1_shorthand.ts".into(), old: "old".into(), new: "next".into(), at: Some(6) };
        let (refs, abstains) = symbol_refs_and_abstains(&cx, &request).unwrap();
        assert!(abstains.is_empty());
        assert!(refs.iter().any(|reference| cx.ts_slow_edit(&reference.file, reference.span).as_deref() == Some("old: next")));
    }

    #[test]
    fn slow_rename_destructuring_names_missing_checker_query() {
        let cx = RenameCx::open(&ts7_rename_fixture()).unwrap().with_slow(true);
        let request = RenameRequest { anchor: "2_destructure.ts".into(), old: "old".into(), new: "next".into(), at: Some(17) };
        let (_refs, abstains) = symbol_refs_and_abstains(&cx, &request).unwrap();
        assert_eq!(abstains.len(), 1);
        assert_eq!(abstains[0].reason, "ts7_api_missing_checker_query");
        assert_eq!(abstains[0].receiver, "getPropertySymbolFromBindingElement");
    }

    #[test]
    fn slow_rename_case_plain_api() {
        assert_oracle("0_fixture.ts", "old", 6);
    }

    #[test]
    fn slow_rename_case_shorthand_api() {
        assert_oracle("1_shorthand.ts", "old", 6);
    }

    #[test]
    fn slow_rename_case_export_alias_api() {
        assert_oracle("4_export.ts", "old", 6);
    }

    #[test]
    fn slow_rename_case_imported_name_api() {
        assert_oracle("5_module.ts", "old", 13);
    }

    #[test]
    fn slow_rename_case_local_import_alias_api() {
        assert_oracle("6_import.ts", "local", 16);
    }

    #[test]
    fn slow_rename_case_unaliased_import_binding_api() {
        assert_oracle("7_import_unaliased.ts", "old", 9);
    }

    #[test]
    fn slow_rename_case_exported_alias_api() {
        assert_oracle("4_export.ts", "publicName", 31);
    }

    #[test]
    fn slow_rename_case_destructuring_abstained() {
        assert_oracle_or_abstain("2_destructure.ts", "old", 17, Some("getPropertySymbolFromBindingElement"));
    }

    #[test]
    fn slow_rename_case_contextual_shorthand_abstained() {
        assert_oracle_or_abstain("8_contextual_shorthand.ts", "old", 15, Some("getPropertySymbolsFromContextualType"));
    }

    #[test]
    fn slow_rename_case_reexport_chain_api() {
        assert_oracle("11_reexport_mid.ts", "mid", 16);
    }

    #[test]
    fn slow_rename_case_overloads_api() {
        assert_oracle("17_overloads.ts", "old", 9);
    }

    #[test]
    fn slow_rename_case_declaration_merge_abstained() {
        assert_oracle_or_abstain("18_merge.ts", "Old", 10, Some("getRootSymbols"));
    }

    #[test]
    fn slow_rename_case_jsx_component_api() {
        assert_oracle("19_jsx_component.tsx", "Old", 9);
    }

    #[test]
    fn slow_rename_case_destructuring_local_api() {
        assert_oracle("9_destructure_local.ts", "old", 35);
    }

    #[test]
    fn slow_rename_case_destructuring_alias_local_api() {
        assert_oracle("10_destructure_alias.ts", "local", 40);
    }

    #[test]
    fn slow_rename_case_destructuring_alias_property_abstained() {
        assert_oracle_or_abstain("10_destructure_alias.ts", "old", 17, Some("getPropertySymbolFromBindingElement"));
    }

    #[test]
    fn slow_rename_case_contextual_member_abstained() {
        assert_oracle_or_abstain("14_contextual_member.ts", "old", 15, Some("getPropertySymbolsFromContextualType"));
    }

    #[test]
    fn slow_rename_case_union_context_abstained() {
        assert_oracle_or_abstain("15_union_context.ts", "old", 22, Some("getPropertySymbolsFromContextualType"));
    }

    #[test]
    fn slow_rename_case_intersection_property_abstained() {
        assert_oracle_or_abstain("16_intersection.ts", "old", 11, Some("getRootSymbols"));
    }

    #[test]
    fn slow_rename_case_string_property_api() {
        assert_oracle("20_string_property.ts", "old", 20);
    }

    #[test]
    fn slow_rename_case_string_type_abstained() {
        assert_oracle_or_abstain("21_string_type.ts", "old", 16, Some("getContextualTypeFromParentOrAncestorTypeNode"));
    }

    #[test]
    fn slow_rename_case_numeric_property_api() {
        assert_oracle("22_numeric.ts", "0", 15);
    }

    fn assert_null_oracle(source: &str, old: &str, at: u32) {
        let cx = RenameCx::open(&ts7_rename_fixture()).unwrap().with_slow(true);
        let request = RenameRequest { anchor: source.into(), old: old.into(), new: "next".into(), at: Some(at) };
        let stem = source.strip_suffix(".tsx").or_else(|| source.strip_suffix(".ts")).unwrap();
        let oracle: Value = serde_json::from_str(&std::fs::read_to_string(crate::io_path(&ts7_rename_fixture().join(format!("{stem}.rename.json")))).unwrap()).unwrap();
        assert!(oracle.is_null());
        match symbol_refs_and_abstains(&cx, &request) {
            Ok((refs, _)) => assert!(refs.is_empty()),
            Err(RenameStop::NotFound { .. }) => {},
            Err(other) => panic!("unexpected rename result: {other:?}"),
        }
    }

    #[test]
    fn slow_rename_case_intrinsic_jsx_null_lsp() {
        assert_null_oracle("3_intrinsic.tsx", "div", 17);
    }

    #[test]
    fn slow_rename_case_module_path_null_lsp() {
        assert_null_oracle("24_module_path.ts", "23_path_source", 26);
    }
}
