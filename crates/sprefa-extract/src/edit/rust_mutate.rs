//! The Rust `Cleave` arm: the only file on a cleave's path that names a Rust
//! node kind, the `pub` keyword or a `crate::` path. The planner reads fact
//! rows and asks here for the three spellings no fact carries.
//! @comment-ok: module header, the seam list every lang arm opens with

use crate::move_cx::MoveCx;
use crate::source::{FamilyMask, Source};
use crate::types::{FamilyTag, Span};
use crate::wire::{flatten_each, FlatFact};

use crate::lang::rust::RustSource;
use crate::edit_seams::Edit;
use crate::edit_seams::Cleave;

/// The path the parse is told it is reading. Only the extension is consulted.
const PARSE_AS: &str = "cleave.rs";

/// The directory component that ends a crate's source root. Everything before
/// the last one is layout a module path never spells.
const SOURCE_ROOT: &str = "src/";

/// File stems that stand for their own directory rather than a module of it.
const DIRECTORY_STEMS: [&str; 3] = ["mod", "lib", "main"];

impl Cleave for RustSource {
    fn edit_export(&self, text: &str, decl: Span, on: bool) -> Option<Edit> {
        let at = past_trivia(text, decl.start as usize);
        let decl = Span { start: at as u32, len: decl.end().saturating_sub(at as u32) };
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
            let held = leaves
                .iter()
                .find(|leaf| leaf.path == module)
                .or_else(|| leaves.iter().find(|leaf| leaf.prefix == module))?;
            if held.prefix == module {
                return Some(Edit { span: held.line, text: String::new() });
            }
            return Some(drop_leaf(&leaves, held));
        }
        // A brace list under `module` that binds names outside `names` is
        // rewritten to bind exactly `names`, keeping its visibility.
        let listed: Vec<&Leaf> = leaves.iter().filter(|leaf| leaf.prefix == module).collect();
        if let Some(first) = listed.first() {
            let same_line = listed.iter().all(|leaf| leaf.line == first.line);
            let extra = listed.iter().any(|leaf| !names.contains(&leaf.leaf));
            if same_line && extra {
                return Some(Edit {
                    span: first.line,
                    text: format!("{}{}", first.vis, use_line(names, module)),
                });
            }
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

    fn edit_import_like(
        &self,
        text: &str,
        names: &[String],
        module: &str,
        like: &str,
        like_module: &str,
    ) -> Option<Edit> {
        let vis = leaves(self, text)
            .into_iter()
            .find(|leaf| leaf.path == format!("{like_module}::{like}") || leaf.path == like_module)
            .map(|leaf| leaf.vis)
            .unwrap_or_default();
        let mut edit = self.edit_import(text, names, module)?;
        if !edit.text.is_empty() && edit.span.len == 0 {
            edit.text = format!("{vis}{}", edit.text);
        }
        Some(edit)
    }

    /// `crate::a::b`, or `super::b` when the two files are siblings under a
    /// module rather than under the crate root. Siblings is a module question,
    /// not a directory one: `lang/mod.rs` IS `lang`, so `lang/ts.rs` is under it.
    fn spell_module(&self, cx: &MoveCx, from_path: &str, to_path: &str) -> String {
        let to = module_parts(cx, to_path);
        if let Some(ident) = foreign_crate(cx, from_path, to_path) {
            return std::iter::once(ident).chain(to).collect::<Vec<_>>().join("::");
        }
        let siblings = parent_of(&module_parts(cx, from_path)) == parent_of(&to);
        match (siblings, to.len() > 1) {
            (true, true) => format!("super::{}", to.last().cloned().unwrap_or_default()),
            _ => match to.is_empty() {
                true => "crate".to_string(),
                false => format!("crate::{}", to.join("::")),
            },
        }
    }

    fn declare_new_file(&self, cx: &MoveCx, src: &str, dest: &str) -> Option<(String, Edit)> {
        let dir = dest.rsplit_once('/').map_or("", |(dir, _)| dir);
        let file = dest.rsplit('/').next().unwrap_or(dest);
        let parent = parent_candidates(dir)
            .into_iter()
            .find(|candidate| cx.contains(candidate))?;
        let text = cx.text(&parent)?;
        let parsed = syn::parse_file(&text).ok()?;
        let (name, numbered) = module_name(file);
        let aim = match numbered {
            true => format!(
                "#[path = \"{}\"] ",
                crate::move_cx::relative_between(
                    parent.rsplit_once('/').map_or("", |(dir, _)| dir),
                    dest
                )
            ),
            false => String::new(),
        };
        let foreign = foreign_crate(cx, src, dest).is_some();
        let vis = match foreign || declared_public(cx, src) {
            true => "pub ",
            false => "pub(crate) ",
        };
        let line_starts = crate::lang::rust::build_line_starts(&text);
        let last_mod = parsed
            .items
            .iter()
            .filter(|item| matches!(item, syn::Item::Mod(module) if module.content.is_none()))
            .last()
            .map(|item| crate::lang::rust::syn_span(&line_starts, syn::spanned::Spanned::span(item)).end());
        let at = match last_mod {
            Some(end) => text[end as usize..]
                .find('\n')
                .map_or(text.len(), |found| end as usize + found + 1),
            None => parsed
                .items
                .first()
                .map(|item| {
                    let start = crate::lang::rust::syn_span(&line_starts, syn::spanned::Spanned::span(item)).start as usize;
                    text[..start].rfind('\n').map_or(0, |found| found + 1)
                })
                .unwrap_or(text.len()),
        };
        let lead = if at == text.len() && !text.is_empty() && !text.ends_with('\n') { "\n" } else { "" };
        Some((
            parent,
            Edit {
                span: Span::anchor(at as u32),
                text: format!("{lead}{aim}{vis}mod {name};\n"),
            },
        ))
    }

    fn respell_relative(&self, cx: &MoveCx, src: &str, dest: &str, module: &str) -> Option<String> {
        let head = module.split("::").next()?;
        if matches!(head, "" | "crate" | "self" | "super") {
            return None;
        }
        let parsed = syn::parse_file(&cx.text(src)?).ok()?;
        let child = parsed
            .items
            .iter()
            .any(|item| matches!(item, syn::Item::Mod(declared) if declared.ident == head));
        child.then(|| format!("{}::{module}", self.spell_module(cx, dest, src)))
    }

    fn publish_module(&self, cx: &MoveCx, dest: &str) -> Option<(String, Edit)> {
        let dir = dest.rsplit_once('/').map_or("", |(dir, _)| dir);
        let name = module_parts(cx, dest).pop()?;
        for parent in parent_candidates(dir).into_iter().filter(|candidate| cx.contains(candidate)) {
            let text = cx.text(&parent)?;
            let parsed = syn::parse_file(&text).ok()?;
            let line_starts = crate::lang::rust::build_line_starts(&text);
            let Some(module) = parsed.items.iter().find_map(|item| match item {
                syn::Item::Mod(module) if module.ident == name && module.content.is_none() => Some(module),
                _ => None,
            }) else {
                continue;
            };
            let edit = match &module.vis {
                syn::Visibility::Public(_) => return None,
                syn::Visibility::Restricted(restricted) => Edit {
                    span: crate::lang::rust::syn_span(&line_starts, syn::spanned::Spanned::span(restricted)),
                    text: "pub".to_string(),
                },
                syn::Visibility::Inherited => Edit {
                    span: Span::anchor(
                        crate::lang::rust::syn_span(&line_starts, module.mod_token.span).start,
                    ),
                    text: "pub ".to_string(),
                },
            };
            return Some((parent, edit));
        }
        None
    }

    fn imports_visible_to_children(&self, cx: &MoveCx, src: &str) -> bool {
        cx.text(src)
            .and_then(|text| syn::parse_file(&text).ok())
            .is_some_and(|file| file.items.iter().any(|item| matches!(item, syn::Item::Mod(_))))
    }
}

/// Whether `path`'s own `mod` declaration is `pub`: a file split off a public
/// module is declared public too, so paths through it keep resolving.
fn declared_public(cx: &MoveCx, path: &str) -> bool {
    let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let Some(name) = module_parts(cx, path).pop() else {
        return false;
    };
    parent_candidates(dir)
        .into_iter()
        .filter_map(|parent| cx.text(&parent))
        .filter_map(|text| syn::parse_file(&text).ok())
        .flat_map(|file| file.items.into_iter())
        .any(|item| {
            matches!(item, syn::Item::Mod(module)
                if module.ident == name && matches!(module.vis, syn::Visibility::Public(_)))
        })
}

/// The first byte at or after `at` that is not whitespace, a comment or an
/// outer attribute: where an item's visibility is written.
pub(crate) fn past_trivia(text: &str, mut at: usize) -> usize {
    let bytes = text.as_bytes();
    loop {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        let rest = &text[at..];
        if rest.starts_with("//") {
            at += rest.find('\n').map_or(rest.len(), |end| end + 1);
        } else if rest.starts_with("/*") {
            at += rest.find("*/").map_or(rest.len(), |end| end + 2);
        } else if rest.starts_with("#[") {
            let mut depth = 0usize;
            let mut end = rest.len();
            for (offset, byte) in rest.bytes().enumerate() {
                match byte {
                    b'[' => depth += 1,
                    b']' => {
                        depth -= 1;
                        if depth == 0 {
                            end = offset + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            at += end;
        } else {
            return at;
        }
    }
}

/// The crate ident `to_path` answers to when it sits in another Cargo package.
/// A package's `tests/`, `examples/` and `benches/` files are crates of their
/// own that reach the library through its ident, never through `crate::`.
fn foreign_crate(cx: &MoveCx, from_path: &str, to_path: &str) -> Option<String> {
    let from = crate::edit::rust_rehome::cargo_package(cx, from_path)?;
    let to = crate::edit::rust_rehome::cargo_package(cx, to_path)?;
    let own_target = ["tests/", "examples/", "benches/"].iter().any(|dir| {
        let prefix = match from.0.is_empty() {
            true => dir.to_string(),
            false => format!("{}/{dir}", from.0),
        };
        from_path.starts_with(&prefix)
    });
    (from.0 != to.0 || own_target).then_some(to.2)
}

/// The files that can own `dir`'s child modules, in rustc's probe order.
fn parent_candidates(dir: &str) -> Vec<String> {
    let join = |name: &str| match dir.is_empty() {
        true => name.to_string(),
        false => format!("{dir}/{name}"),
    };
    let mut out = vec![join("mod.rs"), join("lib.rs"), join("main.rs")];
    if !dir.is_empty() {
        out.push(format!("{dir}.rs"));
    }
    out
}

/// A new file's module name: its stem, minus a `3_` / `3a_` ordering prefix,
/// which a `#[path]` then carries.
fn module_name(file: &str) -> (String, bool) {
    let stem = file.strip_suffix(".rs").unwrap_or(file);
    if let Some((head, tail)) = stem.split_once('_') {
        let digits = head.trim_end_matches(|ch: char| ch.is_ascii_lowercase());
        if !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()) && !tail.is_empty() {
            return (tail.to_string(), true);
        }
    }
    (stem.to_string(), false)
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
    /// What the declaration writes before `use`: `pub `, `pub(crate) `, or empty.
    vis: String,
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
        // A file's imports start a line; an indented `use` is local to a body.
        if line.start != span.start {
            continue;
        }
        let written = slice(text, *span);
        let vis = written
            .find("use ")
            .map_or(String::new(), |at| written[..at].to_string());
        let listed: Vec<Span> = nodes
            .iter()
            .filter(|(kind, at)| kind == "use_list" && inside(*at, *span))
            .map(|(_, at)| *at)
            .collect();
        let Some(list) = listed.first().copied() else {
            let path = path_of(slice(text, *span));
            out.push(Leaf {
                prefix: String::new(),
                leaf: path.rsplit("::").next().unwrap_or(&path).to_string(),
                path,
                span: line,
                line,
                vis: vis.clone(),
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
                vis: vis.clone(),
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
    let vis = &held.vis;
    let text = match kept.len() {
        0 => String::new(),
        1 => format!("{vis}use {}::{};\n", held.prefix, kept[0]),
        _ => format!("{vis}use {}::{{{}}};\n", held.prefix, kept.join(", ")),
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
fn module_parts(cx: &MoveCx, rel: &str) -> Vec<String> {
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
        let parent = rel.rsplit_once('/').map_or("", |(parent, _)| parent);
        let declared = ["lib.rs", "mod.rs", "main.rs"]
            .iter()
            .filter_map(|index| cx.text(&format!("{parent}/{index}")))
            .filter_map(|text| syn::parse_file(&text).ok())
            .flat_map(|file| file.items.into_iter())
            .filter_map(|item| match item {
                syn::Item::Mod(module) => Some(module),
                _ => None,
            })
            .find(|module| {
                module.attrs.iter().any(|attr| {
                    matches!(
                        &attr.meta,
                        syn::Meta::NameValue(syn::MetaNameValue {
                            path,
                            value: syn::Expr::Lit(syn::ExprLit {
                                lit: syn::Lit::Str(value),
                                ..
                            }),
                            ..
                        }) if path.is_ident("path") && value.value() == last
                    )
                })
            })
            .map(|module| module.ident.to_string());
        parts.push(declared.unwrap_or_else(|| module_name(&last).0));
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
