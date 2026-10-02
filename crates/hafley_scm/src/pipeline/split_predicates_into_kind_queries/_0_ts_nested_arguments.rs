use tree_sitter::{Language, Query, QueryErrorKind};

use crate::types::QueryExtError;

pub const PATTERN_PREFIX: &str = "__scm_relation_pattern_";
pub const CAPTURE_PREFIX: &str = "__scm_relation_capture_";
pub const ROOT_CAPTURE: &str = "__scm_relation_root";

fn quote(text: &str) -> String {
    format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}

pub fn pattern_query(language: &Language, text: &str) -> Result<Query, QueryExtError> {
    let query =
        Query::new(language, &format!("({text} @{ROOT_CAPTURE})")).map_err(QueryExtError::Parse)?;
    for pattern in 0..query.pattern_count() {
        if let Some(predicate) = query.general_predicates(pattern).first() {
            return Err(QueryExtError::UnknownOperator(
                predicate.operator.to_string(),
            ));
        }
    }
    Ok(query)
}

// Query::new supplies the error position and accepts the replacement boundaries.
// No s-expression lexer or nesting counter is maintained here.
pub fn user_query(language: &Language, text: &str) -> Result<(Query, Vec<String>), QueryExtError> {
    normalize(language, text.to_owned(), Vec::new())
}

fn normalize(
    language: &Language,
    text: String,
    patterns: Vec<String>,
) -> Result<(Query, Vec<String>), QueryExtError> {
    let error = match Query::new(language, &text) {
        Ok(query) => return Ok((query, patterns)),
        Err(error) => error,
    };
    let offset = error.offset;
    if error.kind == QueryErrorKind::Capture && offset > 0 && text.as_bytes()[offset - 1] == b'@' {
        for pattern in &patterns {
            if let Ok(query) = pattern_query(language, pattern) {
                for name in query
                    .capture_names()
                    .iter()
                    .filter(|name| **name != ROOT_CAPTURE)
                {
                    if text[offset..].starts_with(name) {
                        let end = offset + name.len();
                        if text[end..]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
                        {
                            continue;
                        }
                        let mut replacement = text.clone();
                        replacement.replace_range(
                            offset - 1..end,
                            &quote(&format!("{CAPTURE_PREFIX}{name}")),
                        );
                        return normalize(language, replacement, patterns);
                    }
                }
            }
        }
    }
    if error.kind != QueryErrorKind::Syntax {
        return Err(QueryExtError::Parse(error));
    }
    let Some(start) = text[..offset].rfind("(#") else {
        return Err(QueryExtError::Parse(error));
    };
    let operator = text[start + 2..]
        .split_whitespace()
        .next()
        .unwrap_or_default();
    let bare = operator.strip_prefix("not-").unwrap_or(operator);
    if !matches!(
        bare,
        "has?" | "has-ancestor?" | "has-parent?" | "precedes?" | "follows?" | "nth-child?"
    ) {
        return Err(QueryExtError::Parse(error));
    }
    if text.as_bytes().get(offset) == Some(&b':') {
        let option = text[..offset].split_whitespace().last().unwrap_or_default();
        if matches!(option, "stopBy" | "field") {
            let mut replacement = text.clone();
            replacement.replace_range(offset..offset + 1, " ");
            return normalize(language, replacement, patterns);
        }
    }
    if !matches!(text.as_bytes().get(offset), Some(b'(' | b'[')) {
        return Err(QueryExtError::Parse(error));
    }
    for (relative, character) in text[offset..].char_indices() {
        let end = offset + relative + character.len_utf8();
        let boundary = matches!(character, ')' | ']')
            || text[end..]
                .chars()
                .next()
                .is_some_and(|next| next.is_whitespace() || next == ')');
        if !boundary {
            continue;
        }
        let fragment = &text[offset..end];
        if matches!(pattern_query(language, fragment), Err(QueryExtError::Parse(ref error)) if error.kind == QueryErrorKind::Syntax)
        {
            continue;
        }
        let mut replacement = text.clone();
        replacement.replace_range(
            offset..end,
            &quote(&format!("{PATTERN_PREFIX}{}", patterns.len())),
        );
        let mut next = patterns.clone();
        next.push(text[offset..end].to_owned());
        if let Ok(result) = normalize(language, replacement, next) {
            return Ok(result);
        }
    }
    Err(QueryExtError::Parse(error))
}
