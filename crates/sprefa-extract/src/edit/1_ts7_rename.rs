//! The slow TypeScript rename is the compiler's LSP WorkspaceEdit.

use std::io::{BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use lsp_server::{Message, Notification, Request, RequestId, Response};
use lsp_types::notification::Notification as NotificationMethod;
use lsp_types::request::Request as RequestMethod;
use lsp_types::{
    notification::{DidOpenTextDocument, Initialized},
    request::{PrepareRenameRequest, Rename},
    DidOpenTextDocumentParams, InitializeParams, InitializedParams, Position, RenameParams,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, Uri, WorkspaceEdit,
    WorkspaceFolder,
};
use serde::Serialize;
use serde_json::{json, Value};
use tree_sitter::{Node, Parser};

use crate::edit_seams::{RefRole, RenameAbstain, RenameStop, SymbolRef};
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::Span;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

pub fn symbol_refs_and_abstains(
    cx: &RenameCx,
    request: &RenameRequest,
) -> Result<(Vec<SymbolRef>, Vec<RenameAbstain>), RenameStop> {
    let text = cx.text(&request.anchor).ok_or_else(|| not_found(request))?;
    let seed = seed_offset(&text, request).ok_or_else(|| not_found(request))?;
    let span = Span {
        start: seed as u32,
        len: request.old.len() as u32,
    };
    let position = position_at_byte(&text, seed).map_err(|_| inexact(request, span))?;
    let anchor_uri = file_uri(&cx.abs(&request.anchor)).map_err(|_| inexact(request, span))?;
    let root_uri = file_uri(cx.root()).map_err(|_| inexact(request, span))?;
    let mut lsp = Ts7Lsp::open(cx.root()).map_err(|_| inexact(request, span))?;

    let mut initialize = InitializeParams::default();
    initialize.process_id = Some(std::process::id());
    initialize.workspace_folders = Some(vec![WorkspaceFolder {
        uri: root_uri,
        name: cx
            .root()
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
    }]);
    initialize.initialization_options = Some(json!({}));
    let response = lsp
        .request("initialize", &initialize)
        .map_err(|_| inexact(request, span))?;
    if response.error.is_some() {
        return Err(inexact(request, span));
    }
    lsp.notify::<Initialized>(&InitializedParams {})
        .map_err(|_| inexact(request, span))?;
    lsp.notify::<DidOpenTextDocument>(&DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: anchor_uri.clone(),
            language_id: if request.anchor.ends_with(".tsx") {
                "typescriptreact".into()
            } else {
                "typescript".into()
            },
            version: 1,
            text,
        },
    })
    .map_err(|_| inexact(request, span))?;

    let location = TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri: anchor_uri },
        position,
    };
    let prepare = lsp
        .request(PrepareRenameRequest::METHOD, &location)
        .map_err(|_| inexact(request, span))?;
    if prepare.error.is_some() || prepare.result.as_ref().is_none_or(Value::is_null) {
        return Ok(abstain(request, span, "ts7_lsp_rename_rejected"));
    }
    let rename = lsp
        .request(
            Rename::METHOD,
            &RenameParams {
                text_document_position: location,
                new_name: request.new.clone(),
                work_done_progress_params: Default::default(),
            },
        )
        .map_err(|_| inexact(request, span))?;
    if rename.error.is_some() {
        return Ok(abstain(request, span, "ts7_lsp_rename_rejected"));
    }
    let Some(edit) = rename
        .result
        .filter(|value| !value.is_null())
        .map(serde_json::from_value::<WorkspaceEdit>)
        .transpose()
        .map_err(|_| inexact(request, span))?
    else {
        return Ok(abstain(request, span, "ts7_lsp_rename_empty"));
    };
    let edits = workspace_edits(edit).map_err(|_| inexact(request, span))?;
    if edits.is_empty() {
        return Ok(abstain(request, span, "ts7_lsp_rename_empty"));
    }

    let mut refs = Vec::with_capacity(edits.len());
    let mut replacements = Vec::with_capacity(edits.len());
    for (uri, edit) in edits {
        let path = url::Url::parse(uri.as_str())
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .ok_or_else(|| inexact(request, span))?;
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let rel = path
            .strip_prefix(cx.root())
            .map_err(|_| inexact(request, span))?
            .to_string_lossy()
            .replace('\\', "/");
        if !cx.files().contains(&rel) {
            return Err(inexact(request, span));
        }
        let content = cx.text(&rel).ok_or_else(|| inexact(request, span))?;
        let start =
            byte_at_lsp_position(&content, edit.range.start).map_err(|_| inexact(request, span))?;
        let end =
            byte_at_lsp_position(&content, edit.range.end).map_err(|_| inexact(request, span))?;
        let original = content
            .get(start..end)
            .ok_or_else(|| inexact(request, span))?;
        let site = Span {
            start: start as u32,
            len: (end - start) as u32,
        };
        refs.push(SymbolRef {
            file: rel.clone(),
            span: site,
            role: RefRole::Read,
            text: original.into(),
        });
        replacements.push((rel, site, edit.new_text));
    }
    for (rel, site, replacement) in replacements {
        cx.put_ts_slow_edit(&rel, site, replacement);
    }
    Ok((refs, Vec::new()))
}

fn workspace_edits(edit: WorkspaceEdit) -> Result<Vec<(Uri, lsp_types::TextEdit)>, String> {
    let mut out = Vec::new();
    if let Some(changes) = edit.document_changes {
        match changes {
            lsp_types::DocumentChanges::Edits(documents) => {
                for document in documents {
                    for edit in document.edits {
                        let edit = match edit {
                            lsp_types::OneOf::Left(edit) => edit,
                            lsp_types::OneOf::Right(edit) => edit.text_edit,
                        };
                        out.push((document.text_document.uri.clone(), edit));
                    }
                }
            }
            lsp_types::DocumentChanges::Operations(_) => {
                return Err("rename returned file operations".into());
            }
        }
    } else if let Some(changes) = edit.changes {
        for (uri, edits) in changes {
            out.extend(edits.into_iter().map(|edit| (uri.clone(), edit)));
        }
    }
    Ok(out)
}

struct Ts7Lsp {
    child: Child,
    stdin: ChildStdin,
    receiver: Receiver<Result<Option<Message>, String>>,
    next_id: i32,
}

impl Ts7Lsp {
    fn open(root: &Path) -> Result<Self, String> {
        let tsc = Path::new(env!("CARGO_MANIFEST_DIR")).join("ts7/node_modules/typescript/bin/tsc");
        if !tsc.is_file() {
            return Err(format!(
                "TypeScript LSP executable missing: {}",
                tsc.display()
            ));
        }
        let mut child = Command::new(&tsc)
            .args(["--lsp", "--stdio"])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("start {}: {error}", tsc.display()))?;
        let stdin = child.stdin.take().ok_or("tsc stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("tsc stdout unavailable")?;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                let message = Message::read(&mut stdout).map_err(|error| error.to_string());
                let done = !matches!(message, Ok(Some(_)));
                if sender.send(message).is_err() || done {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            receiver,
            next_id: 1,
        })
    }

    fn send(&mut self, message: Message) -> Result<(), String> {
        message
            .write(&mut self.stdin)
            .map_err(|error| error.to_string())?;
        self.stdin.flush().map_err(|error| error.to_string())
    }

    fn notify<N: NotificationMethod>(&mut self, params: &N::Params) -> Result<(), String> {
        self.send(Message::Notification(Notification {
            method: N::METHOD.into(),
            params: serde_json::to_value(params).map_err(|error| error.to_string())?,
        }))
    }

    fn request(&mut self, method: &str, params: &impl Serialize) -> Result<Response, String> {
        let id = RequestId::from(self.next_id);
        self.next_id += 1;
        self.send(Message::Request(Request {
            id: id.clone(),
            method: method.into(),
            params: serde_json::to_value(params).map_err(|error| error.to_string())?,
        }))?;
        loop {
            let message = self
                .receiver
                .recv_timeout(RESPONSE_TIMEOUT)
                .map_err(|error| format!("{method}: {error}"))??;
            match message.ok_or_else(|| format!("{method}: server closed stdout"))? {
                Message::Response(response) if response.id == id => return Ok(response),
                Message::Request(request) => {
                    let result = match request.method.as_str() {
                        "workspace/configuration" => json!(request.params["items"]
                            .as_array()
                            .map(|items| vec![Value::Null; items.len()])
                            .unwrap_or_default()),
                        "workspace/workspaceFolders" => json!([]),
                        _ => Value::Null,
                    };
                    self.send(Message::Response(Response {
                        id: request.id,
                        result: Some(result),
                        error: None,
                    }))?;
                }
                _ => {}
            }
        }
    }
}

impl Drop for Ts7Lsp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn file_uri(path: &Path) -> Result<Uri, String> {
    let url =
        url::Url::from_file_path(path).map_err(|_| format!("file URI for {}", path.display()))?;
    Uri::from_str(url.as_str()).map_err(|error| error.to_string())
}

fn position_at_byte(text: &str, offset: usize) -> Result<Position, String> {
    let before = text.get(..offset).ok_or("position splits a character")?;
    let line = before.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or(before)
        .encode_utf16()
        .count() as u32;
    Ok(Position {
        line,
        character: column,
    })
}

fn byte_at_lsp_position(text: &str, position: Position) -> Result<usize, String> {
    let mut start = 0;
    for _ in 0..position.line {
        let next = text[start..].find('\n').ok_or("line out of range")?;
        start += next + 1;
    }
    let mut utf16 = 0;
    for (relative, ch) in text[start..].char_indices() {
        if utf16 == position.character {
            return Ok(start + relative);
        }
        if ch == '\n' {
            break;
        }
        utf16 += ch.len_utf16() as u32;
    }
    if utf16 == position.character {
        return Ok(start + text[start..].find('\n').unwrap_or(text.len() - start));
    }
    Err("UTF-16 position splits a character or exceeds the line".into())
}

fn seed_offset(text: &str, request: &RenameRequest) -> Option<usize> {
    let mut parser = Parser::new();
    let language = if request.anchor.ends_with(".tsx") {
        tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TSX)
    } else {
        tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT)
    };
    parser.set_language(&language).ok()?;
    let tree = parser.parse(text, None)?;
    if let Some(at) = request.at {
        let at = at as usize;
        let mut node = Some(tree.root_node().descendant_for_byte_range(at, at + 1)?);
        while let Some(current) = node {
            if current.end_byte() - current.start_byte() == request.old.len()
                && text.get(current.byte_range()) == Some(request.old.as_str())
            {
                return Some(current.start_byte());
            }
            node = current.parent();
        }
        return (at < text.len()).then_some(at);
    }
    fn first_name(node: Node<'_>, text: &str, old: &str) -> Option<usize> {
        if node.named_child_count() == 0 && text.get(node.byte_range()) == Some(old) {
            return Some(node.start_byte());
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if let Some(start) = first_name(child, text, old) {
                return Some(start);
            }
        }
        None
    }
    first_name(tree.root_node(), text, &request.old)
}

fn abstain(
    request: &RenameRequest,
    span: Span,
    reason: &'static str,
) -> (Vec<SymbolRef>, Vec<RenameAbstain>) {
    (
        Vec::new(),
        vec![RenameAbstain {
            file: request.anchor.clone(),
            span,
            symbol: request.old.clone(),
            reason,
            receiver: String::new(),
        }],
    )
}

fn not_found(request: &RenameRequest) -> RenameStop {
    RenameStop::NotFound {
        anchor: request.anchor.clone(),
        old: request.old.clone(),
    }
}

fn inexact(request: &RenameRequest, span: Span) -> RenameStop {
    RenameStop::Inexact {
        file: request.anchor.clone(),
        span,
        why: "ts7_lsp",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    #[test]
    fn lsp_fixture_edits_match_classic_tsserver() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts7_api");
        let cases = [
            ("0_fixture.ts", "old", 6, "0_fixture.rename.json", None),
            ("1_shorthand.ts", "old", 6, "1_shorthand.rename.json", None),
            (
                "2_destructure.ts",
                "old",
                17,
                "2_destructure.rename.json",
                None,
            ),
            (
                "3_intrinsic.tsx",
                "div",
                17,
                "3_intrinsic.rename.json",
                Some("ts7_lsp_rename_rejected"),
            ),
            ("4_export.ts", "old", 6, "4_export.rename.json", None),
            (
                "4_export.ts",
                "publicName",
                31,
                "4_export.publicName.rename.json",
                None,
            ),
            ("5_module.ts", "old", 13, "5_module.rename.json", None),
            ("6_import.ts", "local", 16, "6_import.rename.json", None),
            (
                "7_import_unaliased.ts",
                "old",
                9,
                "7_import_unaliased.rename.json",
                None,
            ),
            (
                "8_contextual_shorthand.ts",
                "old",
                15,
                "8_contextual_shorthand.rename.json",
                None,
            ),
            (
                "9_destructure_local.ts",
                "old",
                35,
                "9_destructure_local.rename.json",
                None,
            ),
            (
                "10_destructure_alias.ts",
                "old",
                17,
                "10_destructure_alias.old.rename.json",
                None,
            ),
            (
                "10_destructure_alias.ts",
                "local",
                40,
                "10_destructure_alias.local.rename.json",
                None,
            ),
            (
                "11_reexport_mid.ts",
                "mid",
                16,
                "11_reexport_mid.rename.json",
                None,
            ),
            (
                "13_reexport_consumer.ts",
                "api",
                9,
                "13_reexport_consumer.rename.json",
                None,
            ),
            (
                "14_contextual_member.ts",
                "old",
                15,
                "14_contextual_member.rename.json",
                None,
            ),
            (
                "15_union_context.ts",
                "old",
                22,
                "15_union_context.rename.json",
                None,
            ),
            (
                "16_intersection.ts",
                "old",
                11,
                "16_intersection.rename.json",
                None,
            ),
            (
                "17_overloads.ts",
                "old",
                9,
                "17_overloads.rename.json",
                None,
            ),
            ("18_merge.ts", "Old", 10, "18_merge.rename.json", None),
            (
                "19_jsx_component.tsx",
                "Old",
                9,
                "19_jsx_component.rename.json",
                None,
            ),
            (
                "20_string_property.ts",
                "old",
                20,
                "20_string_property.rename.json",
                None,
            ),
            (
                "21_string_type.ts",
                "old",
                16,
                "21_string_type.rename.json",
                None,
            ),
            ("22_numeric.ts", "0", 15, "22_numeric.rename.json", None),
            (
                "24_module_path.ts",
                "23_path_source",
                26,
                "24_module_path.rename.json",
                Some("ts7_lsp_rename_rejected"),
            ),
        ];
        for (source, old, at, oracle, expected_reason) in cases {
            let cx = RenameCx::open(&root).unwrap().with_slow(true);
            let request = RenameRequest {
                anchor: source.into(),
                old: old.into(),
                new: if old.starts_with(char::is_uppercase) {
                    "Next"
                } else {
                    "next"
                }
                .into(),
                at: Some(at),
            };
            let (refs, abstains) = symbol_refs_and_abstains(&cx, &request).unwrap();
            if let Some(reason) = expected_reason {
                assert!(refs.is_empty(), "{source}: {} refs", refs.len());
                assert_eq!(abstains.len(), 1, "{source}");
                assert_eq!(abstains[0].reason, reason, "{source}");
                continue;
            }
            assert!(abstains.is_empty(), "{source}: {abstains:?}");
            let mut actual: Vec<_> = refs
                .iter()
                .map(|reference| {
                    (
                        reference.file.clone(),
                        reference.span.start as usize,
                        reference.span.end() as usize,
                        cx.ts_slow_edit(&reference.file, reference.span).unwrap(),
                    )
                })
                .collect();
            actual.sort();
            let oracle: Value =
                serde_json::from_str(&std::fs::read_to_string(root.join(oracle)).unwrap()).unwrap();
            let mut expected = Vec::new();
            for (file, edits) in oracle["changes"].as_object().unwrap() {
                let content = cx.text(file).unwrap();
                let edits: Vec<lsp_types::TextEdit> =
                    serde_json::from_value(edits.clone()).unwrap();
                for edit in edits {
                    expected.push((
                        file.clone(),
                        byte_at_lsp_position(&content, edit.range.start).unwrap(),
                        byte_at_lsp_position(&content, edit.range.end).unwrap(),
                        edit.new_text,
                    ));
                }
            }
            expected.sort();
            assert_eq!(actual, expected, "{source} {old}");
        }
    }

    #[test]
    fn lsp_renames_plain_identifier() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts7_api");
        let cx = RenameCx::open(&root).unwrap().with_slow(true);
        let request = RenameRequest {
            anchor: "0_fixture.ts".into(),
            old: "old".into(),
            new: "next".into(),
            at: Some(6),
        };
        let (refs, abstains) = symbol_refs_and_abstains(&cx, &request).unwrap();
        assert!(abstains.is_empty(), "{abstains:?}");
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn utf16_positions_round_trip() {
        let text = "a😀é\nold";
        assert_eq!(
            position_at_byte(text, 7).unwrap(),
            Position {
                line: 0,
                character: 4
            }
        );
        assert_eq!(
            byte_at_lsp_position(
                text,
                Position {
                    line: 0,
                    character: 4
                }
            )
            .unwrap(),
            7
        );
        assert_eq!(
            byte_at_lsp_position(
                text,
                Position {
                    line: 1,
                    character: 0
                }
            )
            .unwrap(),
            8
        );
        assert!(byte_at_lsp_position(
            text,
            Position {
                line: 0,
                character: 2
            }
        )
        .is_err());
    }
}
