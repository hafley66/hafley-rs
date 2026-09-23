//! The Rust `Cleave` arm: the only file on a cleave's path that names a Rust
//! node kind, the `pub` keyword or a `crate::` path. The planner reads fact
//! rows and asks here for the three spellings no fact carries.
//! @comment-ok: module header, the seam list every lang arm opens with

use crate::source::{FamilyMask, Source};
use crate::types::{Edit, FamilyTag, Cleave, Span};
use crate::wire::{flatten_each, FlatFact};

use super::rust::RustSource;

/// The path the parse is told it is reading. Only the extension is consulted.
const PARSE_AS: &str = "cleave.rs";

/// The directory component that ends a crate's source root. Everything before
/// the last one is layout a module path never spells.
const SOURCE_ROOT: &str = "src/";

/// File stems that stand for their own directory rather than a module of it.
const DIRECTORY_STEMS: [&str; 3] = ["mod", "lib", "main"];

impl Cleave for RustSource {
    fn edit_export(&self, text: &str, decl: Span, on: bool) -> Option<Edit> {
        let at = decl.start as usize;
        let tail = text.get(at..)?;
        let visibility_len = if tail.starts_with("pub ") {
            Some(4)
        } else if let Some(rest) = tail.strip_prefix("pub(") {
            rest.find(')').map(|end| 4 + end + 1)
        } else {
            None
        };
        match (on, visibility_len) {
            (true, None) => Some(Edit {
                span: Span::anchor(decl.start),
                text: "pub ".to_string(),
            }),
            (false, Some(len)) => Some(Edit {
                span: Span { start: decl.start, len: len as u32 },
                text: String::new(),
            }),
            _ => None,
        }
    }

    fn edit_import(&self, text: &str, names: &[String], module: &str) -> Option<Edit> {
        let leaves = leaves(self, text);
        let wanted = wanted(names, module);
        if names.is_empty() {
            let held = leaves.iter().find(|leaf| leaf.path == module)?;
            return Some(drop_leaf(&leaves, held));
        }
        let missing: Vec<&String> = wanted
            .iter()
            .filter(|path| !leaves.iter().any(|leaf| leaf.path == **path))
            .collect();
        if missing.is_empty() {
            return None;
        }
        let at = leaves.iter().map(|leaf| leaf.line.end()).max().unwrap_or(0);
        Some(Edit {
            span: Span::anchor(at),
            text: use_line(names, module),
        })
    }

    /// `crate::a::b`, or `super::b` when the two files are siblings under a
    /// module rather than under the crate root. Siblings is a module question,
    /// not a directory one: `lang/mod.rs` IS `lang`, so `lang/ts.rs` is under it.
    fn spell_module(&self, from_path: &str, to_path: &str) -> String {
        let to = module_parts(to_path);
        let siblings = parent_of(&module_parts(from_path)) == parent_of(&to);
        match (siblings, to.len() > 1) {
            (true, true) => format!("super::{}", to.last().cloned().unwrap_or_default()),
            _ => match to.is_empty() {
                true => "crate".to_string(),
                false => format!("crate::{}", to.join("::")),
            },
        }
    }
}

/// A module path without its own last segment.
fn parent_of(parts: &[String]) -> &[String] {
    parts.split_last().map_or(parts, |(_, head)| head)
}

/// The full paths `module` must supply once it binds `names`.
fn wanted(names: &[String], module: &str) -> Vec<String> {
    names
        .iter()
        .map(|name| match module == name || module.ends_with(&format!("::{name}")) {
            true => module.to_string(),
            false => format!("{module}::{name}"),
        })
        .collect()
}

/// One `use` line binding `names` from `module`, newline included.
fn use_line(names: &[String], module: &str) -> String {
    if names.len() == 1 {
        return format!("use {};\n", wanted(names, module)[0]);
    }
    let mut sorted: Vec<String> = names.to_vec();
    sorted.sort();
    format!("use {module}::{{{}}};\n", sorted.join(", "))
}

/// One name a `use` tree binds, beside the declaration that binds it.
struct Leaf {
    /// The whole path this leaf spells, `crate::a::b`.
    path: String,
    /// The path up to the brace list, empty when the declaration has none.
    prefix: String,
    /// The last segment as written.
    leaf: String,
    span: Span,
    /// The declaration's line-aligned span.
    line: Span,
}

/// The file's `use` leaves, in byte order, off its own cst plane.
fn leaves(source: &RustSource, text: &str) -> Vec<Leaf> {
    let mask = FamilyMask {
        cst: true,
        ..FamilyMask::NONE
    };
    let out = source.extract(PARSE_AS, text.as_bytes(), mask);
    let mut nodes: Vec<(String, Span)> = Vec::new();
    let _ = flatten_each(&out, None, &mut |fact: FlatFact| -> Result<(), ()> {
        if let FlatFact::Node {
            family: FamilyTag::Cst,
            span,
            kind,
            ..
        } = fact
        {
            nodes.push((
                kind,
                Span {
                    start: span.start,
                    len: span.end - span.start,
                },
            ));
        }
        Ok(())
    });
    let mut out = Vec::new();
    for (_, span) in nodes.iter().filter(|(kind, _)| kind == "use_declaration") {
        let line = line_span(text, *span);
        let listed: Vec<Span> = nodes
            .iter()
            .filter(|(kind, at)| kind == "use_list" && inside(*at, *span))
            .map(|(_, at)| *at)
            .collect();
        let Some(list) = listed.first().copied() else {
            let path = path_of(slice(text, *span));
            out.push(Leaf {
                prefix: String::new(),
                leaf: path.clone(),
                path,
                span: line,
                line,
            });
            continue;
        };
        let prefix = path_of(slice(
            text,
            Span {
                start: span.start,
                len: list.start - span.start,
            },
        ));
        let members: Vec<Span> = nodes
            .iter()
            .filter(|(kind, at)| {
                matches!(kind.as_str(), "identifier" | "type_identifier" | "scoped_identifier")
                    && inside(*at, list)
                    && !nodes.iter().any(|(held, outer)| {
                        held == "scoped_identifier" && inside(*at, *outer) && *outer != *at
                    })
            })
            .map(|(_, at)| *at)
            .collect();
        let mut members = members;
        members.sort_by_key(|at| at.start);
        members.dedup_by_key(|at| at.start);
        for at in &members {
            let leaf = slice(text, *at).to_string();
            out.push(Leaf {
                path: format!("{prefix}::{leaf}"),
                prefix: prefix.clone(),
                leaf,
                span: *at,
                line,
            });
        }
    }
    out.sort_by_key(|leaf| leaf.span.start);
    out
}

/// One leaf gone: the whole line when it was the only one, else the line
/// rewritten over what is left, so a one-name list loses its braces.
fn drop_leaf(leaves: &[Leaf], held: &Leaf) -> Edit {
    let kept: Vec<&str> = leaves
        .iter()
        .filter(|leaf| leaf.line == held.line && leaf.path != held.path)
        .map(|leaf| leaf.leaf.as_str())
        .collect();
    let text = match kept.len() {
        0 => String::new(),
        1 => format!("use {}::{};\n", held.prefix, kept[0]),
        _ => format!("use {}::{{{}}};\n", held.prefix, kept.join(", ")),
    };
    Edit {
        span: held.line,
        text,
    }
}

/// A `use` declaration's path text without its keyword, braces or semicolon.
fn path_of(text: &str) -> String {
    text.trim()
        .trim_start_matches("pub ")
        .trim_start()
        .trim_start_matches("use ")
        .trim()
        .trim_end_matches(';')
        .trim()
        .trim_end_matches("::")
        .trim()
        .to_string()
}

/// A file's module path from its crate root, by file layout alone.
fn module_parts(rel: &str) -> Vec<String> {
    let tail = match rel.rfind(SOURCE_ROOT) {
        Some(at) => &rel[at + SOURCE_ROOT.len()..],
        None => rel,
    };
    let mut parts: Vec<String> = tail.split('/').map(str::to_string).collect();
    let Some(last) = parts.pop() else {
        return parts;
    };
    let stem = last.strip_suffix(".rs").unwrap_or(&last);
    if !DIRECTORY_STEMS.contains(&stem) {
        parts.push(stem.to_string());
    }
    parts
}

fn slice(text: &str, span: Span) -> &str {
    text.get(span.start as usize..span.end() as usize)
        .unwrap_or_default()
}

fn inside(inner: Span, outer: Span) -> bool {
    inner.start >= outer.start && inner.end() <= outer.end()
}

/// `span` widened to whole lines, the newline closing its last included.
fn line_span(text: &str, span: Span) -> Span {
    let bytes = text.as_bytes();
    let mut start = span.start as usize;
    while start > 0 && bytes[start - 1] != b'\n' {
        start -= 1;
    }
    let mut end = span.end() as usize;
    while end < bytes.len() && bytes[end - 1] != b'\n' {
        end += 1;
    }
    Span {
        start: start as u32,
        len: (end - start) as u32,
    }
}
