//! Compiler-backed TypeScript rename. Syntax selects candidate seats; checker
//! identity filters them. Unsupported checker relations take the recorded LSP
//! fallback so the edit set remains exact.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};
use tree_sitter::{Node, Parser, Tree};
use url::Url;

use super::ts7_api::{utf16_offset, Ts7Api};
use crate::edit_seams::{RefRole, RenameAbstain, RenameStop, SymbolRef};
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::Span;

pub fn symbol_refs_and_abstains(
    cx: &RenameCx,
    request: &RenameRequest,
) -> Result<(Vec<SymbolRef>, Vec<RenameAbstain>), RenameStop> {
    let text = cx.text(&request.anchor).ok_or_else(|| not_found(request))?;
    let seed = seed_offset(&text, request).ok_or_else(|| not_found(request))?;
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
        return fallback(cx, request, seed, span, query);
    }
    if symbol["id"].as_u64().is_none() { return Err(not_found(request)); }

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
                    return fallback(cx, request, seed, span, query);
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
                return fallback(cx, request, seed, span, query);
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
    if refs.is_empty() { return Err(not_found(request)); }
    Ok((refs, Vec::new()))
}

fn export_local_target(api: &mut Ts7Api, symbol: &Value) -> Result<Option<u64>, String> {
    let Some(handle) = symbol["declarations"].as_array()
        .and_then(|rows| rows.iter().filter_map(Value::as_str).find(|handle|
            handle.split('.').nth(1) == Some("282"))) else { return Ok(None) };
    let target = api.checker("getExportSpecifierLocalTargetSymbol", json!({"location": handle}))?;
    Ok(target["id"].as_u64())
}

fn fallback(cx: &RenameCx, request: &RenameRequest, seed: usize, span: Span, query: &'static str)
    -> Result<(Vec<SymbolRef>, Vec<RenameAbstain>), RenameStop>
{
    let refs = lsp_rename(cx, request, seed).map_err(|_| inexact(request, span))?;
    let abstain = RenameAbstain {
        file: request.anchor.clone(),
        span,
        symbol: request.old.clone(),
        reason: "ts7_api_missing_checker_query",
        receiver: query.to_owned(),
    };
    Ok((refs, vec![abstain]))
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

fn not_found(request: &RenameRequest) -> RenameStop {
    RenameStop::NotFound { anchor: request.anchor.clone(), old: request.old.clone() }
}

fn inexact(request: &RenameRequest, span: Span) -> RenameStop {
    RenameStop::Inexact { file: request.anchor.clone(), span, why: "ts7_api_or_lsp" }
}

fn lsp_rename(cx: &RenameCx, request: &RenameRequest, seed: usize) -> Result<Vec<SymbolRef>, String> {
    let mut lsp = Lsp::start(cx.root())?;
    let root_uri = Url::from_directory_path(cx.root()).map_err(|_| "invalid root path")?.to_string();
    let anchor = cx.abs(&request.anchor);
    let uri = Url::from_file_path(&anchor).map_err(|_| "invalid anchor path")?.to_string();
    let content = cx.text(&request.anchor).ok_or("missing anchor text")?;
    lsp.request("initialize", json!({
        "processId": std::process::id(), "rootUri": root_uri,
        "capabilities": {"textDocument": {"rename": {"prepareSupport": true}}}
    }))?;
    lsp.notify("initialized", json!({}))?;
    lsp.notify("textDocument/didOpen", json!({
        "textDocument": {"uri": uri, "languageId": "typescript", "version": 1, "text": content}
    }))?;
    let (line, character) = line_character(&content, seed + request.old.len() / 2);
    let result = lsp.request("textDocument/rename", json!({
        "textDocument": {"uri": uri}, "position": {"line": line, "character": character},
        "newName": request.new
    }))?;
    let mut refs = Vec::new();
    if let Some(changes) = result["changes"].as_object() {
        for (uri, edits) in changes {
            append_lsp_edits(cx, uri, edits, &mut refs)?;
        }
    }
    if let Some(changes) = result["documentChanges"].as_array() {
        for change in changes {
            if let (Some(uri), edits) = (change["textDocument"]["uri"].as_str(), &change["edits"]) {
                append_lsp_edits(cx, uri, edits, &mut refs)?;
            }
        }
    }
    Ok(refs)
}

fn append_lsp_edits(cx: &RenameCx, uri: &str, edits: &Value, refs: &mut Vec<SymbolRef>) -> Result<(), String> {
    let path = Url::parse(uri).map_err(|error| error.to_string())?
        .to_file_path().map_err(|_| format!("non-file URI {uri}"))?;
    let rel = path.strip_prefix(cx.root()).map_err(|_| format!("edit outside root: {}", path.display()))?
        .to_string_lossy().replace('\\', "/");
    let text = cx.text(&rel).ok_or_else(|| format!("missing edit source {rel}"))?;
    let edits = edits.as_array().ok_or("edits are not an array")?;
    for edit in edits {
        let start = byte_at_lsp_position(&text, &edit["range"]["start"])?;
        let end = byte_at_lsp_position(&text, &edit["range"]["end"])?;
        let replacement = edit["newText"].as_str().ok_or("edit without newText")?.to_owned();
        let span = Span { start: start as u32, len: (end - start) as u32 };
        cx.put_ts_slow_edit(&rel, span, replacement);
        refs.push(SymbolRef { file: rel.clone(), span, role: RefRole::Read, text: text[start..end].to_owned() });
    }
    Ok(())
}

fn line_character(text: &str, byte: usize) -> (usize, usize) {
    let prefix = &text[..byte];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let since_newline = prefix.rsplit('\n').next().unwrap_or("");
    (line, since_newline.encode_utf16().count())
}

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

struct Lsp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Lsp {
    fn start(root: &Path) -> Result<Self, String> {
        let tsc = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ts7/node_modules/typescript/bin/tsc");
        let mut child = Command::new(tsc).arg("--lsp").arg("--stdio")
            .current_dir(root).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit())
            .spawn().map_err(|error| error.to_string())?;
        let stdin = child.stdin.take().ok_or("LSP stdin unavailable")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("LSP stdout unavailable")?);
        Ok(Self { child, stdin, stdout, next_id: 1 })
    }

    fn send(&mut self, message: &Value) -> Result<(), String> {
        let body = serde_json::to_vec(message).map_err(|error| error.to_string())?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len()).map_err(|error| error.to_string())?;
        self.stdin.write_all(&body).map_err(|error| error.to_string())?;
        self.stdin.flush().map_err(|error| error.to_string())
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let response = self.read_message()?;
            if response["id"].as_u64() == Some(id) {
                if !response["error"].is_null() { return Err(response["error"].to_string()); }
                return Ok(response["result"].clone());
            }
            if response["method"].is_string() && !response["id"].is_null() {
                self.send(&json!({"jsonrpc": "2.0", "id": response["id"], "result": null}))?;
            }
        }
    }

    fn read_message(&mut self) -> Result<Value, String> {
        let mut length = None;
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).map_err(|error| error.to_string())?;
            if line.is_empty() { return Err("LSP stdout closed".into()); }
            if line == "\r\n" || line == "\n" { break; }
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("Content-Length") {
                    length = Some(value.trim().parse::<usize>().map_err(|error| error.to_string())?);
                }
            }
        }
        let mut bytes = vec![0; length.ok_or("LSP frame without Content-Length")?];
        self.stdout.read_exact(&mut bytes).map_err(|error| error.to_string())?;
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())
    }
}

impl Drop for Lsp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_oracle(source: &str, old: &str, at: u32) {
        assert_oracle_with_fallback(source, old, at, None);
    }

    fn assert_oracle_with_fallback(source: &str, old: &str, at: u32, missing_query: Option<&str>) {
        let cx = RenameCx::open(&fixture()).unwrap().with_slow(true);
        let new = if old.starts_with(char::is_uppercase) { "Next" } else { "next" };
        let request = RenameRequest { anchor: source.into(), old: old.into(), new: new.into(), at: Some(at) };
        let (refs, abstains) = symbol_refs_and_abstains(&cx, &request).unwrap();
        match missing_query {
            Some(query) => {
                assert_eq!(abstains.len(), 1);
                assert_eq!(abstains[0].receiver, query);
            }
            None => assert!(abstains.is_empty()),
        }
        let mut actual: Vec<_> = refs.iter().map(|reference| (
            reference.file.clone(), reference.span.start as usize, reference.span.end() as usize,
            cx.ts_slow_edit(&reference.file, reference.span).unwrap_or_else(|| request.new.clone()),
        )).collect();
        actual.sort();
        let stem = source.strip_suffix(".tsx").or_else(|| source.strip_suffix(".ts")).unwrap();
        let named_oracle = fixture().join(format!("{stem}.{old}.rename.json"));
        let oracle_file = if named_oracle.is_file() { named_oracle } else {
            fixture().join(format!("{stem}.rename.json"))
        };
        let oracle: Value = serde_json::from_slice(&std::fs::read(oracle_file).unwrap()).unwrap();
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

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts7_api")
    }

    #[test]
    fn slow_rename_plain_identifier_uses_checker_identity() {
        let cx = RenameCx::open(&fixture()).unwrap().with_slow(true);
        let request = RenameRequest { anchor: "0_fixture.ts".into(), old: "old".into(), new: "next".into(), at: Some(6) };
        let (refs, abstains) = symbol_refs_and_abstains(&cx, &request).unwrap();
        assert_eq!(refs.len(), 2);
        assert!(abstains.is_empty());
    }

    #[test]
    fn slow_rename_shorthand_uses_value_symbol_query() {
        let cx = RenameCx::open(&fixture()).unwrap().with_slow(true);
        let request = RenameRequest { anchor: "1_shorthand.ts".into(), old: "old".into(), new: "next".into(), at: Some(6) };
        let (refs, abstains) = symbol_refs_and_abstains(&cx, &request).unwrap();
        assert!(abstains.is_empty());
        assert!(refs.iter().any(|reference| cx.ts_slow_edit(&reference.file, reference.span).as_deref() == Some("old: next")));
    }

    #[test]
    fn slow_rename_destructuring_names_missing_checker_query() {
        let cx = RenameCx::open(&fixture()).unwrap().with_slow(true);
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
    fn slow_rename_case_destructuring_lsp_fallback() {
        assert_oracle_with_fallback("2_destructure.ts", "old", 17, Some("getPropertySymbolFromBindingElement"));
    }

    #[test]
    fn slow_rename_case_contextual_shorthand_lsp_fallback() {
        assert_oracle_with_fallback("8_contextual_shorthand.ts", "old", 15, Some("getPropertySymbolsFromContextualType"));
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
    fn slow_rename_case_declaration_merge_lsp_fallback() {
        assert_oracle_with_fallback("18_merge.ts", "Old", 10, Some("getRootSymbols"));
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
    fn slow_rename_case_destructuring_alias_property_lsp_fallback() {
        assert_oracle_with_fallback("10_destructure_alias.ts", "old", 17, Some("getPropertySymbolFromBindingElement"));
    }

    #[test]
    fn slow_rename_case_contextual_member_lsp_fallback() {
        assert_oracle_with_fallback("14_contextual_member.ts", "old", 15, Some("getPropertySymbolsFromContextualType"));
    }

    #[test]
    fn slow_rename_case_union_context_lsp_fallback() {
        assert_oracle_with_fallback("15_union_context.ts", "old", 22, Some("getPropertySymbolsFromContextualType"));
    }

    #[test]
    fn slow_rename_case_intersection_property_lsp_fallback() {
        assert_oracle_with_fallback("16_intersection.ts", "old", 11, Some("getRootSymbols"));
    }

    #[test]
    fn slow_rename_case_string_property_api() {
        assert_oracle("20_string_property.ts", "old", 20);
    }

    #[test]
    fn slow_rename_case_string_type_lsp_fallback() {
        assert_oracle_with_fallback("21_string_type.ts", "old", 16, Some("getContextualTypeFromParentOrAncestorTypeNode"));
    }

    #[test]
    fn slow_rename_case_numeric_property_api() {
        assert_oracle("22_numeric.ts", "0", 15);
    }

    fn assert_null_oracle(source: &str, old: &str, at: u32) {
        let cx = RenameCx::open(&fixture()).unwrap().with_slow(true);
        let request = RenameRequest { anchor: source.into(), old: old.into(), new: "next".into(), at: Some(at) };
        let stem = source.strip_suffix(".tsx").or_else(|| source.strip_suffix(".ts")).unwrap();
        let oracle: Value = serde_json::from_slice(&std::fs::read(fixture().join(format!("{stem}.rename.json"))).unwrap()).unwrap();
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
