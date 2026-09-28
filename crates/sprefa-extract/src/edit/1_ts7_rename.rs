//! Compiler-backed TypeScript rename. Syntax selects candidate seats; checker
//! identity filters them. Unsupported checker relations take the recorded LSP
//! fallback so the edit set remains exact.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};
use url::Url;

use super::ts7_api::{utf16_offset, Ts7Api};
use crate::edit_seams::{RefRole, RenameAbstain, RenameStop, SymbolRef};
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::Span;

// These TypeScript checker operations are absent from 7.0.2's synchronous API.
// Cases requiring them are passed to textDocument/rename with an abstain row.
pub const MISSING_CHECKER_QUERIES: &[&str] = &[
    "getMergedSymbol",
    "getRootSymbols",
    "getPropertySymbolsFromContextualType",
    "getPropertySymbolOfDestructuringAssignment",
    "getPropertySymbolFromBindingElement",
    "getSymbolsOfParameterPropertyDeclaration",
    "getContextualTypeFromParentOrAncestorTypeNode",
];

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

    if symbol["id"].as_u64().is_none()
        || text.match_indices(&request.old).any(|(at, _)|
            identifier_edges(&text, at, request.old.len()) && complex_at(&text, at, request.old.len()))
    {
        return fallback(cx, request, seed, span);
    }

    let seed_id = canonical_symbol(&mut api, &symbol).map_err(|_| inexact(request, span))?;
    let mut refs = Vec::new();
    for rel in cx.files_of(&crate::lang::ts::TsSource) {
        let Some(content) = cx.text(rel) else { continue };
        let path = cx.abs(rel);
        for (at, _) in content.match_indices(&request.old) {
            if !identifier_edges(&content, at, request.old.len()) { continue }
            let position = utf16_offset(&content, at + request.old.len() / 2);
            let found = api.symbol_at(&path, position).map_err(|_| inexact(request, span))?;
            if found["id"].as_u64().is_none() { continue }
            if canonical_symbol(&mut api, &found).map_err(|_| inexact(request, span))? != seed_id {
                continue;
            }
            if complex_at(&content, at, request.old.len()) {
                return fallback(cx, request, seed, span);
            }
            refs.push(SymbolRef {
                file: rel.to_owned(),
                span: Span { start: at as u32, len: request.old.len() as u32 },
                role: RefRole::Read,
                text: request.old.clone(),
            });
        }
    }
    if refs.is_empty() { return Err(not_found(request)); }
    Ok((refs, Vec::new()))
}

fn fallback(cx: &RenameCx, request: &RenameRequest, seed: usize, span: Span)
    -> Result<(Vec<SymbolRef>, Vec<RenameAbstain>), RenameStop>
{
    let refs = lsp_rename(cx, request, seed).map_err(|_| inexact(request, span))?;
    let abstain = RenameAbstain {
        file: request.anchor.clone(),
        span,
        symbol: request.old.clone(),
        reason: "ts7_api_missing_checker_query",
        receiver: MISSING_CHECKER_QUERIES.join(","),
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

fn complex_at(content: &str, at: usize, len: usize) -> bool {
    let before = &content[..at];
    let after = &content[at + len..];
    let line = before.rsplit('\n').next().unwrap_or("");
    let tail = after.split('\n').next().unwrap_or("");
    let trim = line.trim_end();
    let next = tail.trim_start();
    (trim.ends_with('"') || trim.ends_with('\''))
        || (next.starts_with('"') || next.starts_with('\''))
        || (trim.ends_with('{') && (next.starts_with('}') || next.starts_with(',')))
        || (trim.contains("import {") || trim.contains("export {"))
        || (next.starts_with(':') && (trim.ends_with('{') || trim.ends_with(';')))
        || (trim.ends_with('<') || trim.ends_with("</"))
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
    fn slow_rename_shorthand_records_lsp_fallback() {
        let cx = RenameCx::open(&fixture()).unwrap().with_slow(true);
        let request = RenameRequest { anchor: "1_shorthand.ts".into(), old: "old".into(), new: "next".into(), at: Some(6) };
        let (refs, abstains) = symbol_refs_and_abstains(&cx, &request).unwrap();
        assert_eq!(abstains.len(), 1);
        assert_eq!(abstains[0].reason, "ts7_api_missing_checker_query");
        assert!(refs.iter().any(|reference| cx.ts_slow_edit(&reference.file, reference.span).is_some()));
    }
}
