//! Compiler destinations joined to the existing extraction spans and fact rows.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::ts7_cleave_facts::with_session;
use super::ts7_lsp_session::file_uri;
use super::ts7_symbol_seed::{byte_at_lsp_position, qualified_declaration};
use crate::{FamilyTag, FlatFact, ProjectError, RawProjectFact, ResolveRequest, ResolveWithRawError};

#[derive(Default)]
struct File {
    text: String,
    definitions: Vec<(u32, u32, Option<String>, bool)>,
    sites: Vec<(u32, u32, String)>,
    attributes: Vec<(u32, u32)>,
}

#[derive(Default)]
struct References {
    files: BTreeMap<String, File>,
}

fn typescript(path: &str) -> bool {
    [".ts", ".tsx", ".mts", ".cts"].iter().any(|suffix| path.ends_with(suffix))
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn position(text: &str, offset: usize) -> Result<Value, String> {
    let before = text.get(..offset).ok_or("reference splits a character")?;
    Ok(json!({
        "line": before.bytes().filter(|byte| *byte == b'\n').count(),
        "character": before.rsplit('\n').next().unwrap_or(before).encode_utf16().count(),
    }))
}

fn locations(result: Value) -> Vec<Value> {
    match result {
        Value::Array(rows) => rows,
        Value::Object(_) => vec![result],
        _ => Vec::new(),
    }
}

impl References {
    fn capture(&mut self, raw: &RawProjectFact<'_>) {
        if !typescript(raw.path) {
            return;
        }
        let file = self.files.entry(raw.path.to_string()).or_insert_with(|| File {
            text: String::from_utf8_lossy(raw.content).into_owned(),
            ..File::default()
        });
        match &raw.fact {
            FlatFact::Node { family: FamilyTag::Cst, span, kind, .. }
                if kind == "jsx_attribute" =>
            {
                // The CST span already marks this attribute. Its first token
                // is the name; the expression/string value stays outside it.
                if let Some(attribute) = file.text.get(span.start as usize..span.end as usize) {
                    let name = attribute.split(|ch: char| ch.is_whitespace() || ch == '=')
                        .next().unwrap_or("");
                    if !name.is_empty() {
                        file.attributes.push((span.start, span.start + name.len() as u32));
                    }
                }
            }
            FlatFact::Node { family, span, name, .. }
                if matches!(family, FamilyTag::Call | FamilyTag::Type) =>
            {
                file.definitions.push((span.start, span.end, name.clone(), *family == FamilyTag::Call));
            }
            FlatFact::Site { family: FamilyTag::Call, span, callee, .. } => {
                file.sites.push((span.start, span.end, callee.clone()));
            }
            _ => {}
        }
    }

    fn append(&self, root: &Path, calls: bool, facts: &mut Vec<FlatFact>) -> Result<(), String> {
        if self.files.is_empty() || !calls {
            return Ok(());
        }
        let root = canonical(&crate::io_path(root));
        let supplied: BTreeMap<_, _> = self.files.keys().map(|path| {
            (canonical(&crate::io_path(Path::new(path))), path.as_str())
        }).collect();
        with_session(&root, |session| {
            let mut declarations = BTreeSet::new();
            let mut qualified_names = BTreeMap::new();
            for (absolute, supplied) in &supplied {
                let file = &self.files[*supplied];
                session.sync_document(&file_uri(absolute)?, supplied, &file.text)?;
            }
            for (source, file) in &self.files {
                let uri = file_uri(&canonical(&crate::io_path(Path::new(source))))?;
                for (start, end, name) in &file.sites {
                    let Some(written) = file.text.get(*start as usize..*end as usize) else { continue };
                    // Callee spans cover a member expression, a constructor call,
                    // or a JSX opening element. Probe only the callee token.
                    let callee = if let Some(tag) = written.strip_prefix('<') {
                        tag.split(|ch: char| ch.is_whitespace() || ch == '/' || ch == '>')
                            .next().unwrap_or(tag)
                    } else {
                        written.split('(').next().unwrap_or(written)
                    };
                    let Some(relative) = callee.rfind(name) else { continue };
                    let offset = *start as usize + relative + usize::from(written.starts_with('<'));
                    let reply = session.lsp.request("textDocument/definition", &json!({
                        "textDocument": {"uri": uri.as_str()},
                        "position": position(&file.text, offset)?,
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
                        let target_file = &self.files[*target];
                        let range = if location.get("range").is_some() {
                            &location["range"]
                        } else {
                            &location["targetSelectionRange"]
                        };
                        let at = serde_json::from_value(range["start"].clone())
                            .map_err(|error| format!("definition range: {error}"))?;
                        let at = byte_at_lsp_position(&target_file.text, at)? as u32;
                        let definition = target_file.definitions.iter()
                            .filter(|(lo, hi, name, _)| *lo <= at && at < *hi && name.is_some())
                            .min_by_key(|(lo, hi, _, is_call)| (!is_call, hi - lo));
                        let Some((lo, hi, target_name, _)) = definition else { continue };
                        let mut present = false;
                        for fact in facts.iter_mut() {
                            if let FlatFact::ResolvedEdge {
                                caller_path, caller_site_start, caller_site_end, callee_path,
                                callee_name, callee_start, callee_end, kind, resolution_origin, ..
                            } = fact {
                                if caller_path == source && caller_site_start == start && caller_site_end == end {
                                    *callee_path = (*target).to_string();
                                    *callee_name = target_name.clone();
                                    *callee_start = *lo;
                                    *callee_end = *hi;
                                    *kind = "checker_resolve".to_string();
                                    *resolution_origin = "checker".to_string();
                                    present = true;
                                }
                            }
                        }
                        if !present {
                            let caller = file.definitions.iter()
                                .filter(|(lo, hi, _, is_call)| *is_call && *lo <= *start && *end <= *hi)
                                .min_by_key(|(lo, hi, _, _)| hi - lo)
                                .map(|(lo, _, name, _)| name.clone().unwrap_or_else(|| format!("closure@{lo}")));
                            facts.push(FlatFact::ResolvedEdge {
                                fact: None,
                                caller_path: source.clone(), caller_name: caller,
                                caller_site_start: *start, caller_site_end: *end,
                                callee_path: (*target).to_string(), callee_name: target_name.clone(),
                                callee_start: *lo, callee_end: *hi,
                                kind: "checker_resolve".to_string(), resolution_origin: "checker".to_string(),
                            });
                        }
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
                        let target_file = &self.files[*target];
                        let range = if location.get("range").is_some() {
                            &location["range"]
                        } else {
                            &location["targetSelectionRange"]
                        };
                        let lo = serde_json::from_value(range["start"].clone())
                            .map_err(|error| format!("JSX declaration start: {error}"))?;
                        let hi = serde_json::from_value(range["end"].clone())
                            .map_err(|error| format!("JSX declaration end: {error}"))?;
                        let lo = byte_at_lsp_position(&target_file.text, lo)? as u32;
                        let hi = byte_at_lsp_position(&target_file.text, hi)? as u32;
                        let key = ((*target).to_string(), lo, hi);
                        if !qualified_names.contains_key(&key) {
                            let name = qualified_declaration(
                                &mut session.lsp, &file_uri(&absolute)?, &target_file.text, lo,
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
        })
    }
}

fn checker_error(error: String) -> ProjectError {
    ProjectError::CheckerUnavailable(format!("TypeScript tsgo checker: {error}"))
}

pub fn resolve_project_with_raw<E>(
    request: &ResolveRequest<'_>,
    push_raw: &mut impl FnMut(RawProjectFact<'_>) -> Result<(), E>,
) -> Result<Vec<FlatFact>, ResolveWithRawError<E>> {
    let mut references = References::default();
    let mut facts = crate::resolve_project_with_raw(request, &mut |raw| {
        if request.ts_checker.is_some() { references.capture(&raw); }
        push_raw(raw)
    })?;
    if let Some(root) = request.ts_checker {
        crate::slow::require_ts_checker(request.paths).map_err(ResolveWithRawError::Project)?;
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
    lines.extend(crate::sorted_lines(body));
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
