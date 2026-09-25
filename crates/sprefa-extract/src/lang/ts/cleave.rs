//! The TypeScript `Cleave` arm: the only file on a cleave's path that names a
//! TypeScript node kind, a `./` specifier or the `export` keyword. The planner
//! reads fact rows and asks here for the three spellings no fact carries.
//! @comment-ok: module header, the seam list every lang arm opens with

use crate::move_cx::{dirname, relative_between};
use crate::source::{FamilyMask, Source};
use crate::types::{FamilyTag, Span};
use crate::wire::{flatten_each, FlatFact};

use super::ts::TsSource;
use crate::edit_seams::Edit;
use crate::edit_seams::Cleave;

/// The extensions a module spelling drops.
const EXTENSIONS: [&str; 6] = ["ts", "tsx", "mts", "cts", "js", "mjs"];

/// The path the parse is told it is reading. Only the extension is consulted.
const PARSE_AS: &str = "cleave.ts";

impl Cleave for TsSource {
    fn edit_export(&self, text: &str, decl: Span, on: bool) -> Option<Edit> {
        let at = crate::lang::rust_mutate::past_trivia(text, decl.start as usize);
        if let Some(rest) = text.get(at..).and_then(|tail| tail.strip_prefix("export")) {
            if rest.starts_with([' ', '\t']) {
                let len = (rest.len() - rest.trim_start_matches([' ', '\t']).len() + 6) as u32;
                return (!on).then(|| Edit {
                    span: Span { start: at as u32, len },
                    text: String::new(),
                });
            }
        }
        let decl = Span { start: at as u32, len: decl.end().saturating_sub(at as u32) };
        let head = text.get(..at)?;
        let carried = head.trim_end_matches([' ', '\t']).ends_with("export");
        match (on, carried) {
            (true, false) => Some(Edit {
                span: Span::anchor(decl.start),
                text: "export ".to_string(),
            }),
            (false, true) => {
                let cut = head.rfind("export")? as u32;
                Some(Edit {
                    span: Span {
                        start: cut,
                        len: decl.start - cut,
                    },
                    text: String::new(),
                })
            }
            _ => None,
        }
    }

    fn edit_import(&self, text: &str, names: &[String], module: &str) -> Option<Edit> {
        let statements = imports(self, text);
        let Some(held) = statements.iter().find(|row| row.module == module) else {
            if names.is_empty() {
                return None;
            }
            let quote = statements.first().map_or('"', |row| row.quote);
            let at = statements.iter().map(|row| row.span.end()).max().unwrap_or(0);
            return Some(Edit {
                span: Span::anchor(at),
                text: import_line(names, module, quote),
            });
        };
        if names.is_empty() {
            return Some(Edit {
                span: held.span,
                text: String::new(),
            });
        }
        if held.names == names {
            return None;
        }
        Some(Edit {
            span: held.list?,
            text: format!("{{ {} }}", names.join(", ")),
        })
    }

    fn spell_module(&self, cx: &crate::move_cx::MoveCx, from_path: &str, to_path: &str) -> String {
        if let Some(spec) = crate::lang::ts_rehome::cross::spec_across(cx, from_path, to_path) {
            return spec;
        }
        let relative = relative_between(dirname(from_path), &drop_extension(to_path));
        match relative.is_empty() {
            true => ".".to_string(),
            false if relative.starts_with("..") => relative,
            false => format!("./{relative}"),
        }
    }
}

fn import_line(names: &[String], module: &str, quote: char) -> String {
    format!(
        "import {{ {} }} from {quote}{module}{quote};\n",
        names.join(", ")
    )
}

/// One `import` statement as written. `span` is line aligned, so deleting it
/// takes the whole line; `list` is the `{ ... }` a rewrite replaces.
struct Statement {
    span: Span,
    module: String,
    quote: char,
    list: Option<Span>,
    names: Vec<String>,
}

/// The file's import statements, in byte order, off its own cst plane.
fn imports(source: &TsSource, text: &str) -> Vec<Statement> {
    let mask = FamilyMask {
        cst: true,
        ..FamilyMask::NONE
    };
    let out = source.extract(PARSE_AS, text.as_bytes(), mask);
    let mut nodes: Vec<(String, Option<String>, Span)> = Vec::new();
    let _ = flatten_each(&out, None, &mut |fact: FlatFact| -> Result<(), ()> {
        if let FlatFact::Node {
            family: FamilyTag::Cst,
            span,
            kind,
            name,
            ..
        } = fact
        {
            nodes.push((
                kind,
                name,
                Span {
                    start: span.start,
                    len: span.end - span.start,
                },
            ));
        }
        Ok(())
    });
    let mut statements: Vec<Statement> = Vec::new();
    for (_, _, span) in nodes.iter().filter(|(kind, _, _)| kind == "import_statement") {
        let held = |wanted: &str| -> Option<Span> {
            nodes
                .iter()
                .filter(|(kind, _, at)| kind == wanted && inside(*at, *span))
                .map(|(_, _, at)| *at)
                .next()
        };
        let Some(literal) = held("string") else {
            continue;
        };
        statements.push(Statement {
            span: line_span(text, *span),
            module: bare(slice(text, literal)).to_string(),
            quote: slice(text, literal).chars().next().unwrap_or('"'),
            list: held("named_imports"),
            names: nodes
                .iter()
                .filter(|(kind, name, at)| {
                    kind == "import_specifier" && name.is_some() && inside(*at, *span)
                })
                .map(|(_, name, _)| name.clone().unwrap_or_default())
                .collect(),
        });
    }
    statements.sort_by_key(|row| row.span.start);
    statements
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

/// A string literal without its quotes.
fn bare(literal: &str) -> &str {
    let bytes = literal.as_bytes();
    let quoted = bytes.len() >= 2
        && matches!(bytes[0], b'\'' | b'"' | b'`')
        && bytes[bytes.len() - 1] == bytes[0];
    match quoted {
        true => &literal[1..literal.len() - 1],
        false => literal,
    }
}

/// A TS path without its extension; anything else unchanged.
fn drop_extension(rel: &str) -> String {
    match rel.rsplit_once('.') {
        Some((head, extension)) if EXTENSIONS.contains(&extension) => head.to_string(),
        _ => rel.to_string(),
    }
}
