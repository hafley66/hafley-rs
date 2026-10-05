//! Compiler destinations joined to the existing extraction spans and fact rows.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::checker_edges::{CheckerDefs, CheckerEdge};
use super::ts_cst_tokens::CstTokens;
use super::ts7_cleave_facts::with_session;
use super::ts7_lsp_session::file_uri;
use super::ts7_graph_target::position;
use super::ts7_symbol_seed::{byte_at_lsp_position, qualified_declaration};
use crate::{FamilyTag, FlatFact, ProjectError, RawProjectFact, ResolveRequest, ResolveWithRawError};

#[derive(Default)]
struct File {
    text: String,
    /// `(site start, site end, callee token start)` per call site.
    calls: Vec<(u32, u32, u32)>,
    attributes: Vec<(u32, u32)>,
}

#[derive(Default)]
pub(super) struct References {
    files: BTreeMap<String, File>,
    defs: CheckerDefs,
    /// The file whose facts are arriving: its CST and its call sites.
    open: Option<(String, CstTokens, Vec<(u32, u32, String)>)>,
    /// A one-shot question's name: only call sites and JSX attributes whose
    /// written callee or attribute text is this name are asked. The text is a
    /// prefilter; the checker's answer decides every edge. `None` asks all.
    pub(super) demand: Option<String>,
}

fn typescript(path: &str) -> bool {
    [".ts", ".tsx", ".mts", ".cts"].iter().any(|suffix| path.ends_with(suffix))
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn locations(result: Value) -> Vec<Value> {
    match result {
        Value::Array(rows) => rows,
        Value::Object(_) => vec![result],
        _ => Vec::new(),
    }
}

impl References {
    /// A file's raw facts arrive together; its CST lives until the next file starts.
    pub(super) fn capture(&mut self, raw: &RawProjectFact<'_>) {
        if !typescript(raw.path) {
            return;
        }
        self.defs.capture(raw);
        if self.open.as_ref().is_none_or(|(path, ..)| path != raw.path) {
            self.close();
            self.files.entry(raw.path.to_string()).or_insert_with(|| File {
                text: String::from_utf8_lossy(raw.content).into_owned(),
                ..File::default()
            });
            self.open = Some((raw.path.to_string(), CstTokens::default(), Vec::new()));
        }
        let Some((_, cst, sites)) = self.open.as_mut() else { return };
        cst.push(&raw.fact);
        if let FlatFact::Site { family: FamilyTag::Call, span, callee, .. } = &raw.fact {
            sites.push((span.start, span.end, callee.clone()));
        }
    }

    /// Whether any captured file holds a site or attribute to ask about.
    pub(super) fn has_questions(&self) -> bool {
        self.files.values().any(|file| !file.calls.is_empty() || !file.attributes.is_empty())
    }

    /// The open file's callee and attribute tokens; its CST is dropped.
    pub(super) fn close(&mut self) {
        let Some((path, mut cst, sites)) = self.open.take() else { return };
        cst.seal();
        let Some(file) = self.files.get_mut(&path) else { return };
        let asked = |name: &str| self.demand.as_deref().is_none_or(|demand| demand == name);
        for (start, end, name) in sites {
            if !asked(&name) {
                continue;
            }
            // A site with no CST callee token is not asked about.
            if let Some((token, _)) = cst.callee(&file.text, start, end, &name) {
                file.calls.push((start, end, token));
            }
        }
        let text = &file.text;
        file.attributes.extend(cst.attribute_names().filter(|(start, end)| {
            text.get(*start as usize..*end as usize).is_some_and(asked)
        }));
    }

    fn append(&mut self, root: &Path, calls: bool, facts: &mut Vec<FlatFact>) -> Result<(), String> {
        self.close();
        self.defs.seal();
        if self.files.is_empty() || !calls {
            return Ok(());
        }
        let supplied: Vec<&str> = self.files.keys().map(String::as_str).collect();
        let edges = self.ask(root, &supplied, &self.defs, facts)?;
        self.defs.write(facts, edges);
        Ok(())
    }

    /// One `textDocument/definition` per asked call site and JSX attribute.
    /// `supplied` is the project's file set: a destination outside it is no edge.
    /// `defs` names each destination.
    pub(super) fn ask(
        &self, root: &Path, supplied: &[&str], defs: &CheckerDefs, facts: &mut Vec<FlatFact>,
    ) -> Result<Vec<CheckerEdge>, String> {
        let root = canonical(&crate::io_path(root));
        let supplied: BTreeMap<_, _> = supplied.iter().map(|path| {
            (canonical(&crate::io_path(Path::new(path))), *path)
        }).collect();
        let mut texts: BTreeMap<String, String> = BTreeMap::new();
        let mut edges = Vec::new();
        with_session(&root, |session| {
            let mut declarations = BTreeSet::new();
            let mut qualified_names = BTreeMap::new();
            // Only a file holding an asked site is opened; tsc reads the rest of
            // its project from disk through the tsconfig.
            for (source, file) in &self.files {
                if file.calls.is_empty() && file.attributes.is_empty() {
                    continue;
                }
                let uri = file_uri(&canonical(&crate::io_path(Path::new(source))))?;
                session.sync_document(&uri, source, &file.text)?;
            }
            for (source, file) in &self.files {
                if file.calls.is_empty() && file.attributes.is_empty() {
                    continue;
                }
                let uri = file_uri(&canonical(&crate::io_path(Path::new(source))))?;
                for &(start, end, offset) in &file.calls {
                    let reply = session.lsp.request("textDocument/definition", &json!({
                        "textDocument": {"uri": uri.as_str()},
                        "position": position(&file.text, offset as usize)?,
                    }))?;
                    if let Some(error) = reply.error {
                        return Err(format!("definition {source}:{offset}: {}", error.message));
                    }
                    for location in locations(reply.result.unwrap_or(Value::Null)) {
                        let target_uri = location["uri"].as_str().or_else(|| location["targetUri"].as_str());
                        let Some(absolute) = target_uri.and_then(|uri| url::Url::parse(uri).ok())
                            .and_then(|uri| uri.to_file_path().ok()) else { continue };
                        let absolute = canonical(&absolute);
                        let Some(target) = supplied.get(&absolute) else { continue };
                        let target_text = text_of(&self.files, &mut texts, target, &absolute)?;
                        let range = if location.get("range").is_some() {
                            &location["range"]
                        } else {
                            &location["targetSelectionRange"]
                        };
                        let at = serde_json::from_value(range["start"].clone())
                            .map_err(|error| format!("definition range: {error}"))?;
                        let at = byte_at_lsp_position(target_text, at)? as u32;
                        let Some(target) = defs.target(target, at, self.demand.as_deref()) else { continue };
                        edges.push(CheckerEdge { source: source.clone(), site_start: start, site_end: end, target });
                        break;
                    }
                }
                for (start, end) in &file.attributes {
                    let reply = session.lsp.request("textDocument/definition", &json!({
                        "textDocument": {"uri": uri.as_str()},
                        "position": position(&file.text, *start as usize)?,
                    }))?;
                    if let Some(error) = reply.error {
                        return Err(format!("JSX attribute {source}:{start}: {}", error.message));
                    }
                    for location in locations(reply.result.unwrap_or(Value::Null)) {
                        let target_uri = location["uri"].as_str().or_else(|| location["targetUri"].as_str());
                        let Some(absolute) = target_uri.and_then(|uri| url::Url::parse(uri).ok())
                            .and_then(|uri| uri.to_file_path().ok()) else { continue };
                        let absolute = canonical(&absolute);
                        let Some(target) = supplied.get(&absolute) else { continue };
                        let target_text = text_of(&self.files, &mut texts, target, &absolute)?.to_string();
                        let range = if location.get("range").is_some() {
                            &location["range"]
                        } else {
                            &location["targetSelectionRange"]
                        };
                        let lo = serde_json::from_value(range["start"].clone())
                            .map_err(|error| format!("JSX declaration start: {error}"))?;
                        let hi = serde_json::from_value(range["end"].clone())
                            .map_err(|error| format!("JSX declaration end: {error}"))?;
                        let lo = byte_at_lsp_position(&target_text, lo)? as u32;
                        let hi = byte_at_lsp_position(&target_text, hi)? as u32;
                        let key = ((*target).to_string(), lo, hi);
                        if !qualified_names.contains_key(&key) {
                            // documentSymbol reads an open document.
                            session.sync_document(&file_uri(&absolute)?, target, &target_text)?;
                            let name = qualified_declaration(
                                &mut session.lsp, &file_uri(&absolute)?, &target_text, lo,
                            )?;
                            qualified_names.insert(key.clone(), name);
                        }
                        let Some(name) = &qualified_names[&key] else { continue };
                        let symbol = format!("tsgo {target}#{name}@{lo}:{hi}");
                        if declarations.insert(key) {
                            facts.push(FlatFact::SymbolRow {
                                symbol: symbol.clone(), path: (*target).to_string(), kind: "property".to_string(),
                            });
                            facts.push(FlatFact::OccurrenceRow {
                                symbol: symbol.clone(), path: (*target).to_string(),
                                start: lo, end: hi, role: "def".to_string(), exported: false,
                                decl_start: lo, decl_end: hi,
                            });
                        }
                        facts.push(FlatFact::OccurrenceRow {
                            symbol, path: source.clone(), start: *start, end: *end,
                            role: "ref".to_string(), exported: false, decl_start: *start, decl_end: *end,
                        });
                    }
                }
            }
            Ok(())
        })?;
        Ok(edges)
    }
}

/// A destination file's text: the captured one, else read once from disk.
fn text_of<'t>(
    files: &'t BTreeMap<String, File>, texts: &'t mut BTreeMap<String, String>, path: &str, absolute: &Path,
) -> Result<&'t str, String> {
    if let Some(file) = files.get(path) {
        return Ok(&file.text);
    }
    if !texts.contains_key(path) {
        let text = std::fs::read_to_string(absolute)
            .map_err(|error| format!("read {}: {error}", absolute.display()))?;
        texts.insert(path.to_string(), text);
    }
    Ok(&texts[path])
}

fn checker_error(error: String) -> ProjectError {
    ProjectError::CheckerUnavailable(format!("TypeScript tsgo checker: {error}"))
}

pub fn resolve_project_with_raw<E>(
    request: &ResolveRequest<'_>,
    push_raw: &mut impl FnMut(RawProjectFact<'_>) -> Result<(), E>,
) -> Result<Vec<FlatFact>, ResolveWithRawError<E>> {
    // Resolve reports an unavailable optional tier through its decline stream.
    // Slow entry points enforce their checker requirement separately.
    if !cfg!(feature = "ts-checker") {
        return crate::resolve_project_with_raw(request, push_raw);
    }
    let mut references = References::default();
    let mut facts = crate::resolve_project_with_raw(request, &mut |raw| {
        if request.ts_checker.is_some() { references.capture(&raw); }
        push_raw(raw)
    })?;
    if let Some(root) = request.ts_checker {
        references.append(root, request.arms.call, &mut facts)
            .map_err(|error| ResolveWithRawError::Project(checker_error(error)))?;
    }
    Ok(facts)
}

pub fn resolve_project_jsonl(request: &ResolveRequest<'_>) -> Result<Vec<String>, ProjectError> {
    if request.ts_checker.is_none() { return crate::resolve_project_jsonl(request); }
    let facts = resolve_project_with_raw(request, &mut |_| Ok::<(), std::convert::Infallible>(()))
        .map_err(|error| match error {
            ResolveWithRawError::Project(error) => error,
            ResolveWithRawError::RawSink(never) => match never {},
        })?;
    let (header, body): (Vec<_>, Vec<_>) = facts.into_iter()
        .partition(|fact| matches!(fact, FlatFact::Protocol { .. } | FlatFact::Run(_)));
    let mut lines: Vec<_> = header.iter().map(|fact| serde_json::to_string(fact).unwrap()).collect();
    let (syntax, resolved): (Vec<_>, Vec<_>) = body.into_iter().partition(|fact| {
        matches!(fact, FlatFact::CallSiteRow { .. }
            | FlatFact::JsxElementRow { .. } | FlatFact::JsxAttributeRow { .. })
    });
    lines.extend(crate::sorted_lines(resolved));
    lines.extend(crate::sorted_lines(syntax));
    Ok(lines)
}

pub fn slow_project_with_raw<E>(
    files: &[PathBuf], root: &Path, index: Option<&Path>, checkers: bool,
    push_raw: &mut impl FnMut(RawProjectFact<'_>) -> Result<(), E>,
) -> Result<Vec<FlatFact>, ResolveWithRawError<E>> {
    let mut references = References::default();
    let mut facts = crate::slow_project_with_raw(files, root, index, checkers, &mut |raw| {
        if checkers { references.capture(&raw); }
        push_raw(raw)
    })?;
    if checkers {
        references.append(root, true, &mut facts)
            .map_err(|error| ResolveWithRawError::Project(checker_error(error)))?;
    }
    Ok(facts)
}

pub fn slow_project(
    files: &[PathBuf], root: &Path, index: Option<&Path>, checkers: bool,
) -> Result<Vec<FlatFact>, ProjectError> {
    slow_project_with_raw(files, root, index, checkers, &mut |_| Ok::<(), std::convert::Infallible>(()))
        .map_err(|error| match error {
            ResolveWithRawError::Project(error) => error,
            ResolveWithRawError::RawSink(never) => match never {},
        })
}
