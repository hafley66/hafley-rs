use super::_0_types::ScmppError;

/// One level's text with its predicates cut out.
pub struct LevelText {
    pub root: String,
    pub captures: Vec<Box<str>>,
    pub preds: Vec<Pred>,
}

pub struct Pred {
    pub op: String,
    pub text: String,
    pub args: Vec<Arg>,
    pub offset: usize,
}

#[derive(Debug, PartialEq)]
pub enum Arg {
    Capture(String),
    Word(String),
    Str(String),
    /// A balanced `( ... )` or `[ ... ]` argument and its byte offset in the whole query.
    Group(String, usize),
    Key(String),
}

fn syntax(offset: usize, message: impl Into<String>) -> ScmppError {
    ScmppError::Syntax { offset, message: message.into() }
}

fn after_string(text: &[u8], start: usize, base: usize) -> Result<usize, ScmppError> {
    let mut i = start + 1;
    while i < text.len() {
        match text[i] {
            b'\\' => i += 2,
            b'"' => return Ok(i + 1),
            _ => i += 1,
        }
    }
    Err(syntax(base + start, "unterminated string"))
}

fn after_comment(text: &[u8], start: usize) -> usize {
    text[start..].iter().position(|b| *b == b'\n').map_or(text.len(), |n| start + n)
}

/// The one paren counter: `(` and `[` open, `)` and `]` close; strings and `;` comments skipped.
pub fn after_group(text: &[u8], start: usize, base: usize) -> Result<usize, ScmppError> {
    let mut depth = 0usize;
    let mut i = start;
    while i < text.len() {
        match text[i] {
            b'"' => {
                i = after_string(text, i, base)?;
                continue;
            }
            b';' => {
                i = after_comment(text, i);
                continue;
            }
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                depth = depth.checked_sub(1).ok_or_else(|| syntax(base + i, "unbalanced close"))?;
                if depth == 0 {
                    return Ok(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err(syntax(base + start, "unbalanced open"))
}

fn name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')
}

fn word_char(b: u8) -> bool {
    !b.is_ascii_whitespace() && !matches!(b, b'(' | b')' | b'[' | b']' | b'"' | b';' | b'@')
}

fn unescape(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn pred(text: &str, base: usize) -> Result<Pred, ScmppError> {
    let bytes = text.as_bytes();
    let mut i = bytes.iter().position(|b| *b == b'#').unwrap() + 1;
    let op_start = i;
    while i < bytes.len() && word_char(bytes[i]) {
        i += 1;
    }
    let op = text[op_start..i].to_string();
    let end = bytes.len() - 1;
    let mut args = Vec::new();
    while i < end {
        match bytes[i] {
            b if b.is_ascii_whitespace() => i += 1,
            b';' => i = after_comment(bytes, i),
            b'@' => {
                let start = i + 1;
                i = start;
                while i < end && name_char(bytes[i]) {
                    i += 1;
                }
                args.push(Arg::Capture(text[start..i].to_string()));
            }
            b'"' => {
                let after = after_string(bytes, i, base)?;
                args.push(Arg::Str(unescape(&text[i + 1..after - 1])));
                i = after;
            }
            b'(' | b'[' => {
                let after = after_group(bytes, i, base)?;
                args.push(Arg::Group(text[i..after].to_string(), base + i));
                i = after;
            }
            b')' | b']' => return Err(syntax(base + i, "unbalanced close in predicate")),
            _ => {
                let start = i;
                while i < end && word_char(bytes[i]) {
                    i += 1;
                }
                let token = &text[start..i];
                match token.split_once(':') {
                    Some((key, rest)) => {
                        args.push(Arg::Key(key.to_string()));
                        if !rest.is_empty() {
                            args.push(Arg::Word(rest.to_string()));
                        }
                    }
                    None => args.push(Arg::Word(token.to_string())),
                }
            }
        }
    }
    Ok(Pred { op, text: text.to_string(), args, offset: base })
}

/// Cuts every `(#...)` predicate out of `text`; nested patterns stay inside their predicate's args.
pub fn split(text: &str, base: usize) -> Result<LevelText, ScmppError> {
    let bytes = text.as_bytes();
    let mut root = String::with_capacity(text.len());
    let mut preds = Vec::new();
    let mut captures: Vec<Box<str>> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let after = after_string(bytes, i, base)?;
                root.push_str(&text[i..after]);
                i = after;
            }
            b';' => i = after_comment(bytes, i),
            b'(' if text[i + 1..].trim_start().starts_with('#') => {
                let after = after_group(bytes, i, base)?;
                preds.push(pred(&text[i..after], base + i)?);
                root.truncate(root.trim_end().len());
                if !matches!(bytes.get(after), None | Some(b')' | b']')) {
                    root.push(' ');
                }
                i = after;
            }
            b'@' => {
                let start = i + 1;
                i = start;
                while i < bytes.len() && name_char(bytes[i]) {
                    i += 1;
                }
                let name = &text[start..i];
                if !captures.iter().any(|seen| seen.as_ref() == name) {
                    captures.push(name.into());
                }
                root.push('@');
                root.push_str(name);
            }
            _ => {
                let c = text[i..].chars().next().unwrap();
                root.push(c);
                i += c.len_utf8();
            }
        }
    }
    let root = root_of(root.trim(), base)?;
    Ok(LevelText { root, captures, preds })
}

/// Start offsets of the pattern items at depth 0; captures, quantifiers and anchors attach.
fn items(text: &str, base: usize) -> Result<Vec<usize>, ScmppError> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b if b.is_ascii_whitespace() => i += 1,
            b'*' | b'+' | b'?' | b'.' => i += 1,
            b'"' => {
                found.push(i);
                i = after_string(bytes, i, base)?;
            }
            b'(' | b'[' => {
                found.push(i);
                i = after_group(bytes, i, base)?;
            }
            b'@' => {
                i += 1;
                while i < bytes.len() && name_char(bytes[i]) {
                    i += 1;
                }
            }
            b')' | b']' => return Err(syntax(base + i, "unbalanced close")),
            _ => {
                found.push(i);
                while i < bytes.len() && word_char(bytes[i]) {
                    i += 1;
                }
            }
        }
    }
    Ok(found)
}

/// Unwraps grouping parens until one node pattern (or alternation) remains.
fn root_of(text: &str, base: usize) -> Result<String, ScmppError> {
    let mut text = text;
    loop {
        if items(text, base)?.len() != 1 {
            return Err(syntax(base, format!("a level needs exactly one root pattern: `{text}`")));
        }
        let bytes = text.as_bytes();
        let grouping = bytes.first() == Some(&b'(')
            && after_group(bytes, 0, base)? == bytes.len()
            && matches!(text[1..].trim_start().as_bytes().first(), Some(b'(' | b'[' | b'"'));
        if !grouping {
            return Ok(text.to_string());
        }
        text = text[1..text.len() - 1].trim();
    }
}
