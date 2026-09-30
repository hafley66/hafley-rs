//! The slow TypeScript rename is the compiler's LSP WorkspaceEdit.

use std::collections::HashMap;
use std::sync::Mutex;

use lsp_types::request::Request as RequestMethod;
use lsp_types::{
    request::{PrepareRenameRequest, Rename},
    Position, RenameParams, TextDocumentIdentifier, TextDocumentPositionParams, Uri, WorkspaceEdit,
};
use serde_json::Value;

use super::ts7_lsp_session::{file_uri, TsSession, TS_SESSIONS};
use super::ts7_symbol_seed::{byte_at_lsp_position, declaration_spans};
use crate::edit_seams::{RefRole, RenameAbstain, RenameStop, SymbolRef};
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::Span;

pub fn symbol_refs_and_abstains(
    cx: &RenameCx,
    request: &RenameRequest,
) -> Result<(Vec<SymbolRef>, Vec<RenameAbstain>), RenameStop> {
    let text = cx.text(&request.anchor).ok_or_else(|| not_found(request))?;
    let mut span = Span {
        start: request.at.unwrap_or(0),
        len: request.old.len() as u32,
    };
    let anchor_uri = file_uri(&cx.abs(&request.anchor)).map_err(|_| inexact(request, span))?;
    let pool = TS_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut sessions = pool.lock().map_err(|_| inexact(request, span))?;
    if !sessions.contains_key(cx.root()) {
        sessions.insert(
            cx.root().to_path_buf(),
            TsSession::open(cx.root()).map_err(|_| inexact(request, span))?,
        );
    }
    let session = sessions.get_mut(cx.root()).unwrap();
    for changed in std::mem::take(&mut session.pending) {
        let content = cx.text(&changed).ok_or_else(|| inexact(request, span))?;
        let uri = file_uri(&cx.abs(&changed)).map_err(|_| inexact(request, span))?;
        session
            .sync_document(&uri, &changed, &content)
            .map_err(|_| inexact(request, span))?;
    }
    session
        .sync_document(&anchor_uri, &request.anchor, &text)
        .map_err(|_| inexact(request, span))?;
    let lsp = &mut session.lsp;
    let position = if let Some(at) = request.at {
        position_at_byte(&text, at as usize).map_err(|_| inexact(request, span))?
    } else {
        let sites = declaration_spans(lsp, &anchor_uri, &text, &request.old)
            .map_err(|_| inexact(request, span))?;
        span = match sites.as_slice() {
            [] => return Err(not_found(request)),
            [site] => *site,
            _ => {
                return Err(RenameStop::Ambiguous {
                    anchor: request.anchor.clone(),
                    old: request.old.clone(),
                    sites,
                })
            }
        };
        position_at_byte(&text, span.start as usize).map_err(|_| inexact(request, span))?
    };

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
        session.pending.insert(rel.clone());
        cx.put_slow_edit(&rel, site, replacement);
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
    use std::path::Path;

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
                        cx.slow_edit(&reference.file, reference.span).unwrap(),
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
    fn warm_lsp_returns_the_cold_edits() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts7_api");
        let pool = TS_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        pool.lock().unwrap().remove(&root);
        let cx = RenameCx::open(&root).unwrap().with_slow(true);
        let request = RenameRequest {
            anchor: "0_fixture.ts".into(),
            old: "old".into(),
            new: "next".into(),
            at: Some(6),
        };
        let cold = symbol_refs_and_abstains(&cx, &request).unwrap().0;
        let warm = symbol_refs_and_abstains(&cx, &request).unwrap().0;
        let sites = |refs: Vec<SymbolRef>| {
            refs.into_iter()
                .map(|reference| (reference.file, reference.span.start, reference.text))
                .collect::<Vec<_>>()
        };
        assert_eq!(sites(cold), sites(warm));
    }

    #[test]
    fn lsp_selects_an_unpositioned_declaration_from_document_symbols() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts7_api");
        let cx = RenameCx::open(&root).unwrap().with_slow(true);
        let request = RenameRequest {
            anchor: "0_fixture.ts".into(),
            old: "old".into(),
            new: "next".into(),
            at: None,
        };
        let (refs, abstains) = symbol_refs_and_abstains(&cx, &request).unwrap();
        assert!(abstains.is_empty());
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn lsp_stops_on_ambiguous_document_symbols() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts7_api");
        let cx = RenameCx::open(&root).unwrap().with_slow(true);
        let request = RenameRequest {
            anchor: "17_overloads.ts".into(),
            old: "old".into(),
            new: "next".into(),
            at: None,
        };
        assert!(matches!(
            symbol_refs_and_abstains(&cx, &request),
            Err(RenameStop::Ambiguous { sites, .. }) if sites.len() > 1
        ));
        let missing = RenameRequest {
            anchor: "0_fixture.ts".into(),
            old: "absent".into(),
            new: "next".into(),
            at: None,
        };
        assert!(matches!(
            symbol_refs_and_abstains(&cx, &missing),
            Err(RenameStop::NotFound { .. })
        ));
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
