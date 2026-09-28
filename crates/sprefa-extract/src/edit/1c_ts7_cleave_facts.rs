use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Mutex;

use lsp_types::request::Request as _;
use lsp_types::{
    request::DocumentSymbolRequest, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    SymbolKind, TextDocumentIdentifier,
};
use serde_json::{json, Value};

use super::ts7_lsp_session::{file_uri, TsSession, TS_SESSIONS};
use super::ts7_symbol_seed::byte_at_lsp_position;

#[derive(Clone, Debug)]
pub struct Item {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub type_only: bool,
}

#[derive(Clone, Debug)]
pub struct Definition {
    pub name: String,
    pub site: u32,
    pub path: String,
    pub type_only: bool,
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub path: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug)]
pub struct Facts {
    pub items: Vec<Item>,
    pub definitions: Vec<Definition>,
    pub references: Vec<Reference>,
}

fn session_pool() -> &'static Mutex<HashMap<std::path::PathBuf, TsSession>> {
    TS_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn with_session<T>(
    root: &Path,
    f: impl FnOnce(&mut TsSession) -> Result<T, String>,
) -> Result<T, String> {
    let mut sessions = session_pool().lock().map_err(|error| error.to_string())?;
    if !sessions.contains_key(root) {
        sessions.insert(root.to_path_buf(), TsSession::open(root)?);
    }
    let session = sessions.get_mut(root).unwrap();
    for changed in std::mem::take(&mut session.pending) {
        let text = std::fs::read_to_string(root.join(&changed))
            .map_err(|error| format!("read {changed}: {error}"))?;
        let uri = file_uri(&root.join(&changed))?;
        session.sync_document(&uri, &changed, &text)?;
    }
    f(session)
}

fn relative(root: &Path, uri: &str) -> Option<String> {
    let path = url::Url::parse(uri).ok()?.to_file_path().ok()?;
    path.strip_prefix(root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn byte_position(text: &str, offset: usize) -> Result<Value, String> {
    let before = text.get(..offset).ok_or("position splits a character")?;
    Ok(json!({
        "line": before.bytes().filter(|byte| *byte == b'\n').count(),
        "character": before.rsplit('\n').next().unwrap_or(before).encode_utf16().count(),
    }))
}

fn locations(result: Value) -> Vec<Value> {
    match result {
        Value::Array(rows) => rows,
        Value::Object(row) if row.contains_key("uri") => vec![Value::Object(row)],
        _ => Vec::new(),
    }
}

fn definition_type_only(
    session: &mut TsSession,
    root: &Path,
    path: &str,
    at: &Value,
) -> Result<bool, String> {
    let text = std::fs::read_to_string(root.join(path))
        .map_err(|error| format!("read definition {path}: {error}"))?;
    let uri = file_uri(&root.join(path))?;
    session.sync_document(&uri, path, &text)?;
    let reply = session.lsp.request(
        DocumentSymbolRequest::METHOD,
        &DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        },
    )?;
    if let Some(error) = reply.error {
        return Err(format!("documentSymbol {path}: {}", error.message));
    }
    let symbols: DocumentSymbolResponse =
        serde_json::from_value(reply.result.ok_or("documentSymbol returned no result")?)
            .map_err(|error| format!("documentSymbol {path}: {error}"))?;
    let at = serde_json::from_value(at.clone())
        .map_err(|error| format!("definition position: {error}"))?;
    let at = byte_at_lsp_position(&text, at)? as u32;
    match symbols {
        DocumentSymbolResponse::Nested(symbols) => Ok(symbols.iter().any(|symbol| {
            let Ok(item) = item_from_symbol(&text, symbol) else {
                return false;
            };
            item.type_only && item.start <= at && at <= item.end
        })),
        DocumentSymbolResponse::Flat(symbols) => Ok(symbols.iter().any(|symbol| {
            let Ok(start) = byte_at_lsp_position(&text, symbol.location.range.start) else {
                return false;
            };
            let Ok(end) = byte_at_lsp_position(&text, symbol.location.range.end) else {
                return false;
            };
            matches!(
                symbol.kind,
                SymbolKind::INTERFACE | SymbolKind::TYPE_PARAMETER
            ) && (start as u32) <= at
                && at <= end as u32
        })),
    }
}

pub fn collect_ts_cleave(
    root: &Path,
    source: &str,
    text: &str,
    item_name: &str,
    candidates: &[(String, u32)],
    imported_names: &BTreeSet<String>,
) -> Result<Facts, String> {
    with_session(root, |session| {
        let uri = file_uri(&root.join(source))?;
        session.sync_document(&uri, source, text)?;
        let reply = session.lsp.request(
            DocumentSymbolRequest::METHOD,
            &DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )?;
        if let Some(error) = reply.error {
            return Err(format!("documentSymbol: {}", error.message));
        }
        let symbols: DocumentSymbolResponse =
            serde_json::from_value(reply.result.ok_or("documentSymbol returned no result")?)
                .map_err(|error| format!("documentSymbol: {error}"))?;
        let mut items = Vec::new();
        match symbols {
            DocumentSymbolResponse::Nested(symbols) => {
                for symbol in symbols {
                    items.push(item_from_symbol(text, &symbol)?);
                }
            }
            DocumentSymbolResponse::Flat(symbols) => {
                for symbol in symbols
                    .into_iter()
                    .filter(|symbol| symbol.location.uri == uri)
                {
                    items.push(Item {
                        name: symbol.name,
                        start: byte_at_lsp_position(text, symbol.location.range.start)? as u32,
                        end: byte_at_lsp_position(text, symbol.location.range.end)? as u32,
                        type_only: matches!(
                            symbol.kind,
                            SymbolKind::INTERFACE | SymbolKind::TYPE_PARAMETER
                        ),
                    });
                }
            }
        }
        let item = items
            .iter()
            .find(|item| item.name == item_name)
            .ok_or_else(|| format!("documentSymbol declares no {item_name}"))?;
        let reference_reply = session.lsp.request(
            "textDocument/references",
            &json!({
                "textDocument": {"uri": uri.as_str()},
                "position": byte_position(text, item.start as usize)?,
                "context": {"includeDeclaration": false},
            }),
        )?;
        if let Some(error) = reference_reply.error {
            return Err(format!("references: {}", error.message));
        }
        let mut references = Vec::new();
        for row in locations(reference_reply.result.unwrap_or(Value::Null)) {
            let Some(path) = row["uri"].as_str().and_then(|uri| relative(root, uri)) else {
                continue;
            };
            let content = if path == source {
                text.to_string()
            } else {
                match std::fs::read_to_string(root.join(&path)) {
                    Ok(content) => content,
                    Err(_) => continue,
                }
            };
            let start = serde_json::from_value(row["range"]["start"].clone())
                .map_err(|error| format!("reference position: {error}"))?;
            let end = serde_json::from_value(row["range"]["end"].clone())
                .map_err(|error| format!("reference position: {error}"))?;
            references.push(Reference {
                path,
                start: byte_at_lsp_position(&content, start)? as u32,
                end: byte_at_lsp_position(&content, end)? as u32,
            });
        }
        let mut definitions = Vec::new();
        let mut seen = BTreeSet::new();
        let mut kinds = HashMap::new();
        for (name, site) in candidates {
            if !seen.insert((name.clone(), *site)) {
                continue;
            }
            let reply = session.lsp.request(
                "textDocument/definition",
                &json!({
                    "textDocument": {"uri": uri.as_str()},
                    "position": byte_position(text, *site as usize)?,
                }),
            )?;
            if let Some(error) = reply.error {
                return Err(format!("definition {name}: {}", error.message));
            }
            for location in locations(reply.result.unwrap_or(Value::Null)) {
                let Some(path) = location["uri"].as_str().and_then(|uri| relative(root, uri))
                else {
                    continue;
                };
                let key = (
                    path.clone(),
                    location["range"]["start"]["line"]
                        .as_u64()
                        .unwrap_or_default(),
                    location["range"]["start"]["character"]
                        .as_u64()
                        .unwrap_or_default(),
                );
                let type_only = if imported_names.contains(name) && path != source {
                    match kinds.get(&key) {
                        Some(type_only) => *type_only,
                        None => {
                            let type_only = definition_type_only(
                                session,
                                root,
                                &path,
                                &location["range"]["start"],
                            )?;
                            kinds.insert(key, type_only);
                            type_only
                        }
                    }
                } else {
                    false
                };
                definitions.push(Definition {
                    name: name.clone(),
                    site: *site,
                    path,
                    type_only,
                });
                break;
            }
        }
        Ok(Facts {
            items,
            definitions,
            references,
        })
    })
}

fn item_from_symbol(text: &str, symbol: &DocumentSymbol) -> Result<Item, String> {
    Ok(Item {
        name: symbol.name.clone(),
        start: byte_at_lsp_position(text, symbol.selection_range.start)? as u32,
        end: byte_at_lsp_position(text, symbol.selection_range.end)? as u32,
        type_only: matches!(
            symbol.kind,
            SymbolKind::INTERFACE | SymbolKind::TYPE_PARAMETER
        ),
    })
}
