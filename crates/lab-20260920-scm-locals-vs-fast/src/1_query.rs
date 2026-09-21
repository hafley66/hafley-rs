use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator};

use crate::{Capture, LabError, QueryOutput};

pub fn language(name: &str) -> Result<Language, LabError> {
    match name {
        "kotlin" => Ok(Language::new(tree_sitter_kotlin_sg::LANGUAGE)),
        "ts" | "typescript" => Ok(Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT)),
        other => Err(LabError::Query(format!("unknown language: {other}"))),
    }
}
pub fn matches_only(language_name: &str, query_text: &str, source: &[u8]) -> Result<QueryOutput, LabError> {
    let language = language(language_name)?;
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|error| LabError::Query(format!("set language: {error}")))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| LabError::Query("parse returned no tree".into()))?;
    let query = Query::new(&language, query_text)
        .map_err(|error| LabError::Query(format!("query row {}: {error}", error.row + 1)))?;
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut stream = cursor.matches(&query, tree.root_node(), source);
    let mut captures = Vec::new();
    while let Some(found) = stream.next() {
        for capture in found.captures {
            let node = capture.node;
            captures.push(Capture {
                label: names[capture.index as usize].to_string(),
                text: node.utf8_text(source).unwrap_or("").to_string(),
                start: node.start_byte() as u32,
                end: node.end_byte() as u32,
            });
        }
    }
    drop(stream);
    let did_exceed_match_limit = cursor.did_exceed_match_limit();
    Ok(QueryOutput {
        captures,
        did_exceed_match_limit,
    })
}
