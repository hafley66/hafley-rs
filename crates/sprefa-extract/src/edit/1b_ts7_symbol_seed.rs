use lsp_types::request::Request as RequestMethod;
use lsp_types::{
    request::DocumentSymbolRequest, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    Position, Range, TextDocumentIdentifier, Uri,
};

use super::ts7_lsp_session::Ts7Lsp;
use crate::Span;

pub(super) fn declaration_spans(
    lsp: &mut Ts7Lsp,
    uri: &Uri,
    text: &str,
    old: &str,
) -> Result<Vec<Span>, String> {
    let response = lsp.request(
        DocumentSymbolRequest::METHOD,
        &DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        },
    )?;
    if let Some(error) = response.error {
        return Err(format!("documentSymbol: {}", error.message));
    }
    let symbols = response.result.ok_or("documentSymbol returned no result")?;
    if symbols.is_null() {
        return Ok(Vec::new());
    }
    let symbols: DocumentSymbolResponse =
        serde_json::from_value(symbols).map_err(|error| error.to_string())?;
    let mut ranges = Vec::new();
    match symbols {
        DocumentSymbolResponse::Flat(symbols) => {
            for symbol in symbols {
                if symbol.name == old && symbol.location.uri == *uri {
                    ranges.push(symbol.location.range);
                }
            }
        }
        DocumentSymbolResponse::Nested(symbols) => {
            for symbol in symbols {
                collect_named(&symbol, old, &mut ranges);
            }
        }
    }
    let mut sites = ranges
        .into_iter()
        .map(|range| {
            let start = byte_at_lsp_position(text, range.start)?;
            let end = byte_at_lsp_position(text, range.end)?;
            if end < start {
                return Err("documentSymbol returned a reversed selection range".into());
            }
            Ok(Span {
                start: start as u32,
                len: (end - start) as u32,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    sites.sort_by_key(|site| (site.start, site.len));
    sites.dedup_by_key(|site| (site.start, site.len));
    Ok(sites)
}

fn collect_named(symbol: &DocumentSymbol, old: &str, ranges: &mut Vec<Range>) {
    if symbol.name == old {
        ranges.push(symbol.selection_range);
    }
    if let Some(children) = &symbol.children {
        for child in children {
            collect_named(child, old, ranges);
        }
    }
}

pub(super) fn byte_at_lsp_position(text: &str, position: Position) -> Result<usize, String> {
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
