//! The children of a level's root node pattern, for the sibling warning and its parent-edge lowering.
use super::_0_types::ScmppError;
use super::_1_parens::{after_group, cut};

pub struct Child {
    pub field: Option<String>,
    /// The child pattern with its trailing captures.
    pub text: String,
    pub captures: Vec<Box<str>>,
    /// A `( ... )` node pattern with no quantifier and no anchor beside it.
    pub plain: bool,
    pub anchored: bool,
}

pub struct Shape {
    pub kind: String,
    /// Captures and quantifiers after the root's closing paren.
    pub tail: String,
    pub children: Vec<Child>,
    pub negated_field: bool,
}

impl Shape {
    /// Node-pattern children with no `.` anchor beside them: tree-sitter tries every ordered pick of them.
    pub fn unanchored(&self) -> usize {
        self.children
            .iter()
            .filter(|child| child.text.starts_with(['(', '[']) && !child.anchored)
            .count()
    }

    /// Every child splits into its own parent edge and keeps tree-sitter's meaning.
    pub fn splittable(&self) -> bool {
        let mut names: Vec<&str> = Vec::new();
        let disjoint = self.children.iter().flat_map(|child| &child.captures).all(|name| {
            let fresh = !names.contains(&name.as_ref());
            names.push(name);
            fresh
        });
        !self.children.is_empty()
            && !self.negated_field
            && !self.tail.contains(['*', '+', '?'])
            && disjoint
            && self.children.iter().all(|child| child.plain)
    }
}

fn word_end(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len()
        && !bytes[i].is_ascii_whitespace()
        && !matches!(bytes[i], b'(' | b')' | b'[' | b']' | b'"' | b';' | b'@')
    {
        i += 1;
    }
    i
}

/// `None` when the root is not one `(kind child...)` node pattern.
pub fn shape(root: &str, base: usize) -> Result<Option<Shape>, ScmppError> {
    let bytes = root.as_bytes();
    if bytes.first() != Some(&b'(') {
        return Ok(None);
    }
    let end = after_group(bytes, 0, base)?;
    let tail = root[end..].trim().to_string();
    let inner = &root[1..end - 1];
    let bytes = inner.as_bytes();
    let mut i = bytes.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(bytes.len());
    let kind_end = word_end(bytes, i);
    if kind_end == i {
        return Ok(None);
    }
    let kind = inner[i..kind_end].to_string();
    i = kind_end;
    let mut shape = Shape {
        kind,
        tail,
        children: Vec::new(),
        negated_field: false,
    };
    let mut field = None;
    let mut anchor = false;
    while i < bytes.len() {
        match bytes[i] {
            b if b.is_ascii_whitespace() => i += 1,
            b';' => i = bytes[i..].iter().position(|b| *b == b'\n').map_or(bytes.len(), |n| i + n),
            b'.' => {
                anchor = true;
                if let Some(last) = shape.children.last_mut() {
                    last.anchored = true;
                    last.plain = false;
                }
                i += 1;
            }
            // A capture on a bare word child: that child stays unsplit.
            b'@' => {
                if let Some(last) = shape.children.last_mut() {
                    last.plain = false;
                }
                i = word_end(bytes, i + 1);
            }
            b'!' => {
                shape.negated_field = true;
                i = word_end(bytes, i + 1);
            }
            b'(' | b'[' | b'"' => {
                let start = i;
                i = after_group(bytes, i, base + 1)?;
                let group = inner[start..i].to_string();
                let mut quantifier = None;
                let mut tail = String::new();
                // Trailing captures and quantifiers belong to the child.
                loop {
                    let next = i + bytes[i..].iter().take_while(|b| b.is_ascii_whitespace()).count();
                    match bytes.get(next) {
                        Some(b'@') => {
                            i = word_end(bytes, next + 1);
                            tail.push(' ');
                            tail.push_str(&inner[next..i]);
                        }
                        Some(quantified @ (b'*' | b'+' | b'?')) => {
                            quantifier = Some(*quantified);
                            i = next + 1;
                        }
                        _ => break,
                    }
                }
                let (_, captures, _) = cut(&inner[start..i], base)?;
                // Between unanchored siblings tree-sitter answers `(x)+ @c` one node per match, the
                // rows `(x) @c` answers; with a capture inside the group that no longer holds.
                let single = quantifier == Some(b'+') && !group.contains('@');
                shape.children.push(Child {
                    field: field.take(),
                    plain: bytes[start] == b'(' && (quantifier.is_none() || single) && !anchor,
                    anchored: anchor,
                    text: match single {
                        true => format!("{group}{tail}"),
                        false => inner[start..i].to_string(),
                    },
                    captures,
                });
                anchor = false;
            }
            _ => {
                let start = i;
                i = word_end(bytes, i).max(i + 1);
                match inner[start..i].strip_suffix(':') {
                    Some(name) => field = Some(name.to_string()),
                    // A bare `_` or other word child: kept, never split.
                    None => shape.children.push(Child {
                        field: field.take(),
                        text: inner[start..i].to_string(),
                        captures: Vec::new(),
                        plain: false,
                        anchored: anchor,
                    }),
                }
            }
        }
    }
    Ok(Some(shape))
}
