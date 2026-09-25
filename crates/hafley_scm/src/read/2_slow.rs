//! `ryi slow`: a SCIP index projected onto the tables `ryi fast` writes. Every
//! edge comes from the index; the parse only supplies the keying spans.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::read::project::{
    conformance_edges, read_inputs, scip_conformances, ProjectError, ProjectInput,
    RawProjectFact, ResolveWithRawError,
};
use crate::read::scip::{byte_range_at, join_documents, LineTable};
use crate::read::scip_ensure::{default_cache_dir, ensure_index_picked_for_root, IndexBudget};
use crate::read::scip_v5_rels::descriptor_name;
use crate::read::seams::{FileSet, IndexBag, ManifestMap, ProjectCx, ProjectDigest};
use crate::read::shape::{ContentId, FamilyTag, Span};
use crate::read::types::{covering_def, OccurrenceRole, ResolutionOrigin, ScipIndex, SymbolId};
use crate::read::wire::{FlatFact, SpanOut};

/// The oracle rows for `files`, with SCIP document paths relative to `root`.
pub fn slow_project(
    files: &[PathBuf],
    root: &Path,
    index: Option<&Path>,
    checkers: bool,
) -> Result<Vec<FlatFact>, ProjectError> {
    let mut ignore = |_: RawProjectFact<'_>| Ok::<(), std::convert::Infallible>(());
    slow_project_with_raw(files, root, index, checkers, &mut ignore).map_err(|error| match error {
        ResolveWithRawError::Project(error) => error,
        ResolveWithRawError::RawSink(never) => match never {},
    })
}

/// `slow_project`, with each input's file row and phase-1 rows sent to `push_raw`
/// first, the way `diet_scip_with_raw` lands fast's.
pub fn slow_project_with_raw<E>(
    files: &[PathBuf],
    root: &Path,
    index: Option<&Path>,
    checkers: bool,
    push_raw: &mut impl FnMut(RawProjectFact<'_>) -> Result<(), E>,
) -> Result<Vec<FlatFact>, ResolveWithRawError<E>> {
    let mut inputs = read_inputs(files).map_err(ResolveWithRawError::Project)?;
    for input in &mut inputs {
        push_phase_one(input, push_raw)?;
    }
    let mut facts = Vec::new();
    let index = match index {
        Some(path) => crate::read::scip_decode::load_index(path)
            .map_err(|error| ResolveWithRawError::Project(ProjectError::Scip(error)))?,
        None => {
            let report = ensure_index_picked_for_root(
                root,
                &default_cache_dir(root),
                IndexBudget::from_env(),
                None,
            );
            facts.extend(report.skips.iter().map(|skip| FlatFact::ScipSkipRow {
                lang: skip.lang.to_string(),
                bin: skip.bin.to_string(),
                reason: skip.reason.slug().to_string(),
                detail: skip.reason.detail(),
            }));
            let Some(path) = report.index else {
                return Ok(facts);
            };
            crate::read::scip_decode::load_index(&path)
                .map_err(|error| ResolveWithRawError::Project(ProjectError::Scip(error)))?
        }
    };
    facts.extend(project_index(&inputs, root, index));
    if checkers {
        facts.extend(checker_facts(files, root).map_err(ResolveWithRawError::Project)?);
    }
    Ok(facts)
}

fn push_phase_one<E>(
    input: &mut ProjectInput,
    push_raw: &mut impl FnMut(RawProjectFact<'_>) -> Result<(), E>,
) -> Result<(), ResolveWithRawError<E>> {
    if let Some(file) = input.file.take() {
        push_raw(RawProjectFact { path: &input.path, content_id: &input.blob, fact: file })
            .map_err(ResolveWithRawError::RawSink)?;
    }
    crate::read::wire::flatten_each(input.output.as_ref(), None, &mut |mut fact| {
        if let FlatFact::Unresolved { path, .. } = &mut fact {
            path.get_or_insert_with(|| input.path.clone());
        }
        push_raw(RawProjectFact { path: &input.path, content_id: &input.blob, fact })
    })
    .map_err(ResolveWithRawError::RawSink)
}

/// One indexed document joined to the input carrying the same bytes.
struct Doc<'a> {
    ix: usize,
    input: &'a ProjectInput,
    content: &'a [u8],
    lines: LineTable,
}

fn project_index(inputs: &[ProjectInput], root: &Path, index: ScipIndex) -> Vec<FlatFact> {
    let root_buf = root.to_path_buf();
    let reader = move |relative: &str| std::fs::read(root_buf.join(relative)).ok();
    let joined = join_documents(&index, &reader);
    let input_of_blob: HashMap<&ContentId, &ProjectInput> =
        inputs.iter().rev().map(|input| (&input.blob, input)).collect();
    let docs: HashMap<usize, Doc<'_>> = joined
        .iter()
        .enumerate()
        .filter_map(|(ix, entry)| {
            let (blob, content) = entry.as_ref()?;
            let input = *input_of_blob.get(blob)?;
            Some((ix, Doc { ix, input, content, lines: LineTable::build(content) }))
        })
        .collect();
    let mut order: Vec<&Doc<'_>> = docs.values().collect();
    order.sort_by_key(|doc| doc.ix);
    let defs = definitions(&index, &order);

    let mut facts = Vec::new();
    for doc in &order {
        facts.extend(occurrence_rows(&index, doc));
        facts.extend(site_rows(&index, doc, &defs));
        facts.extend(type_rows(&index, doc, &defs));
        facts.extend(import_rows(&index, doc, &defs));
    }

    let cx = ProjectCx {
        files: &FileSet,
        manifests: &ManifestMap,
        reader: Some(&reader),
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
        witness: false,
    };
    let _ = cx.indexes.scip_index.set(index);
    facts.extend(conformance_edges(inputs, &scip_conformances(inputs, &cx, Some(root))));
    facts
}

/// Byte span of one SCIP range inside `doc`.
fn span_of(doc: &Doc<'_>, index: &ScipIndex, range: [i32; 4]) -> Option<Span> {
    let encoding = index.documents[doc.ix].position_encoding;
    byte_range_at(doc.content, &doc.lines, range, encoding)
}

/// `symbol` and `occurrence` rows for every occurrence the document carries,
/// in the scm rows' shape.
fn occurrence_rows(index: &ScipIndex, doc: &Doc<'_>) -> Vec<FlatFact> {
    let path = &doc.input.path;
    let mut facts = Vec::new();
    for occurrence in &index.documents[doc.ix].occurrences {
        let Some(span) = span_of(doc, index, occurrence.range) else { continue };
        let symbol = index.symbol(occurrence.symbol).to_string();
        let definition = occurrence.roles.contains(OccurrenceRole::DEFINITION);
        let decl = occurrence
            .enclosing_range
            .and_then(|range| span_of(doc, index, range))
            .unwrap_or(span);
        if definition {
            facts.push(FlatFact::SymbolRow {
                symbol: symbol.clone(),
                path: path.clone(),
                kind: symbol_kind(&symbol).to_string(),
            });
        }
        facts.push(FlatFact::OccurrenceRow {
            symbol,
            path: path.clone(),
            start: span.start,
            end: span.end(),
            role: if definition { "def" } else { "ref" }.to_string(),
            exported: false,
            decl_start: decl.start,
            decl_end: decl.end(),
        });
    }
    facts
}

/// The SCIP descriptor suffix, spelled as the scm rows spell a kind.
fn symbol_kind(symbol: &str) -> &'static str {
    if symbol.starts_with("local ") {
        "local"
    } else if symbol.ends_with(").") {
        "function"
    } else if symbol.ends_with('#') {
        "type"
    } else if symbol.ends_with('/') {
        "module"
    } else if symbol.ends_with('!') {
        "macro"
    } else {
        "term"
    }
}

/// A symbol a call site can land on: a callable, a type used as a constructor,
/// or a local binding.
fn site_symbol(symbol: &str) -> bool {
    symbol.ends_with(").") || symbol.ends_with('#') || symbol.starts_with("local ")
}

/// Reference occurrences keyed by their end byte, for the site join.
fn refs_by_end(index: &ScipIndex, doc: &Doc<'_>) -> HashMap<u32, Vec<(u32, SymbolId)>> {
    let mut by_end: HashMap<u32, Vec<(u32, SymbolId)>> = HashMap::new();
    for occurrence in &index.documents[doc.ix].occurrences {
        if occurrence.roles.contains(OccurrenceRole::DEFINITION) {
            continue;
        }
        let Some(span) = span_of(doc, index, occurrence.range) else { continue };
        by_end.entry(span.end()).or_default().push((span.start, occurrence.symbol));
    }
    by_end
}

/// Global symbol -> its definition inside the inputs, (path, span), least first:
/// a definition outside every input leaves the symbol external.
type Defs = HashMap<SymbolId, (String, Span)>;

fn definitions(index: &ScipIndex, docs: &[&Doc<'_>]) -> Defs {
    let mut defs = Defs::new();
    for doc in docs {
        for occurrence in &index.documents[doc.ix].occurrences {
            if !occurrence.roles.contains(OccurrenceRole::DEFINITION)
                || index.symbol(occurrence.symbol).starts_with("local ")
            {
                continue;
            }
            let Some(span) = span_of(doc, index, occurrence.range) else { continue };
            let here = (doc.input.path.clone(), span);
            defs.entry(occurrence.symbol)
                .and_modify(|best| {
                    if (&here.0, here.1.start) < (&best.0, best.1.start) {
                        *best = here.clone();
                    }
                })
                .or_insert(here);
        }
    }
    defs
}

/// One row per (call site, reference ending at the site's end): `resolved_edge`
/// for a definition in the inputs, else `unresolved` local/external/no_occurrence.
fn site_rows(index: &ScipIndex, doc: &Doc<'_>, defs: &Defs) -> Vec<FlatFact> {
    let Some(call) = doc.input.output.call.as_ref() else { return Vec::new() };
    let strings = &doc.input.output.strings;
    let path = &doc.input.path;
    let by_end = refs_by_end(index, doc);
    let unresolved = |site: Span, reason: &str, detail: String| FlatFact::Unresolved {
        family: FamilyTag::Call,
        path: Some(path.clone()),
        span: SpanOut::new(site.start, site.end()),
        reason: reason.to_string(),
        detail,
    };
    let mut facts = Vec::new();
    let mut seen = BTreeSet::new();
    for site in &call.aux.sites {
        let span = site.span;
        if !seen.insert((span.start, span.end())) {
            continue;
        }
        let hits: Vec<SymbolId> = by_end
            .get(&span.end())
            .into_iter()
            .flatten()
            .filter(|(start, symbol)| *start >= span.start && site_symbol(index.symbol(*symbol)))
            .map(|(_, symbol)| *symbol)
            .collect();
        if hits.is_empty() {
            facts.push(unresolved(span, "no_occurrence", strings.lookup(site.callee).to_string()));
            continue;
        }
        for symbol in hits {
            let text = index.symbol(symbol);
            if text.starts_with("local ") {
                facts.push(unresolved(span, "local", text.to_string()));
                continue;
            }
            let Some((callee_path, callee)) = defs.get(&symbol).cloned() else {
                facts.push(unresolved(span, "external", text.to_string()));
                continue;
            };
            facts.push(FlatFact::ResolvedEdge {
                fact: None,
                caller_path: path.clone(),
                caller_name: caller_name(doc.input, span),
                callee_path,
                callee_name: descriptor_name(text),
                caller_site_start: span.start,
                caller_site_end: span.end(),
                callee_start: callee.start,
                callee_end: callee.end(),
                kind: "call".to_string(),
                resolution_origin: ResolutionOrigin::Scip.as_str().to_string(),
            });
        }
    }
    facts
}

/// One `resolved_type_edge` per parse type-edge candidate: the first type
/// reference inside the owner spelling the candidate's name names the target.
fn type_rows(index: &ScipIndex, doc: &Doc<'_>, defs: &Defs) -> Vec<FlatFact> {
    let Some(types) = doc.input.output.types.as_ref() else { return Vec::new() };
    let strings = &doc.input.output.strings;
    let mut refs: Vec<(Span, SymbolId)> = index.documents[doc.ix]
        .occurrences
        .iter()
        .filter(|occurrence| {
            !occurrence.roles.contains(OccurrenceRole::DEFINITION)
                && index.symbol(occurrence.symbol).ends_with('#')
        })
        .filter_map(|occurrence| Some((span_of(doc, index, occurrence.range)?, occurrence.symbol)))
        .collect();
    refs.sort_by_key(|(span, _)| (span.start, span.end()));
    // An owner span is its name; the owner's text runs to the next owner's name.
    let starts: BTreeSet<u32> = types.aux.candidates.iter().map(|c| c.owner.start).collect();
    let mut seen = BTreeSet::new();
    let mut facts = Vec::new();
    for candidate in &types.aux.candidates {
        let owner = candidate.owner;
        let until = starts
            .range(owner.start + 1..)
            .next()
            .copied()
            .unwrap_or(doc.content.len() as u32);
        let written = strings.lookup(candidate.to);
        let bare = written.rsplit([':', '.']).next().unwrap_or(written);
        let text = |span: &Span| doc.content.get(span.start as usize..span.end() as usize);
        let hit = match candidate.kind {
            // `impl Trait for Owner` sits on its own line, anywhere in the file.
            crate::read::types::TypeEdgeKind::Impl => {
                let name = text(&owner).unwrap_or_default();
                refs.iter().find(|(span, _)| {
                    text(span) == Some(bare.as_bytes()) && line_of(doc.content, *span).windows(name.len()).any(|w| w == name)
                })
            }
            _ => refs.iter().find(|(span, _)| {
                span.start >= owner.start && span.end() <= until && text(span) == Some(bare.as_bytes())
            }),
        };
        let Some(&(_, symbol)) = hit else { continue };
        let Some((target_path, _)) = defs.get(&symbol) else { continue };
        let kind = candidate.kind.as_str();
        if !seen.insert((owner.start, owner.end(), target_path.clone(), symbol, kind)) {
            continue;
        }
        facts.push(FlatFact::ResolvedTypeEdge {
            fact: None,
            owner_path: doc.input.path.clone(),
            owner_name: owner_name(types, strings, owner),
            owner_start: owner.start,
            owner_end: owner.end(),
            target_path: target_path.clone(),
            target_name: descriptor_name(index.symbol(symbol)),
            kind: kind.to_string(),
            resolution_origin: ResolutionOrigin::Scip.as_str().to_string(),
        });
    }
    facts
}

/// The whole source line holding `span`.
fn line_of(content: &[u8], span: Span) -> &[u8] {
    let start = content[..span.start as usize].iter().rposition(|b| *b == b'\n').map_or(0, |at| at + 1);
    let end = content[span.end() as usize..]
        .iter()
        .position(|b| *b == b'\n')
        .map_or(content.len(), |at| span.end() as usize + at);
    &content[start..end]
}

/// The declared name at `owner`: a type node, else an impl owner.
fn owner_name(
    types: &crate::read::rows::FamilyBundle<crate::read::types::TypeF>,
    strings: &crate::read::shape::Strings,
    owner: Span,
) -> Option<String> {
    if let Some(node) = types.nodes.iter().find(|node| node.span == owner) {
        return node.name.map(|name| strings.lookup(name).to_string());
    }
    types
        .aux
        .impl_owners
        .iter()
        .find(|impl_owner| impl_owner.span == owner)
        .map(|impl_owner| strings.lookup(impl_owner.name).to_string())
}

/// The tightest def around `site` in the parse, named the way fast names it.
fn caller_name(input: &ProjectInput, site: Span) -> Option<String> {
    let call = input.output.call.as_ref()?;
    let node = call.node(covering_def(call, site)?);
    Some(match node.name {
        Some(name) => input.output.strings.lookup(name).to_string(),
        None => format!("closure@{}", node.span.start),
    })
}

/// `resolved_import` from specifier spans: one `module` row per target file,
/// one `local` row per binding whose target is not a module.
fn import_rows(index: &ScipIndex, doc: &Doc<'_>, defs: &Defs) -> Vec<FlatFact> {
    let Some(call) = doc.input.output.call.as_ref() else { return Vec::new() };
    let strings = &doc.input.output.strings;
    let path = &doc.input.path;
    let mut spans: Vec<(Span, SymbolId)> = index.documents[doc.ix]
        .occurrences
        .iter()
        .filter_map(|occurrence| Some((span_of(doc, index, occurrence.range)?, occurrence.symbol)))
        .collect();
    spans.sort_by_key(|(span, _)| (span.start, span.end()));
    let mut modules = BTreeSet::new();
    let mut facts = Vec::new();
    for specifier in &call.aux.specifiers {
        let outer = specifier.span;
        let inside: Vec<(Span, SymbolId)> = spans
            .iter()
            .filter(|(span, _)| span.start >= outer.start && span.end() <= outer.end())
            .copied()
            .collect();
        let pick = inside
            .iter()
            .rev()
            .find(|(span, _)| span.end() == outer.end())
            .or_else(|| inside.iter().rev().find(|(_, symbol)| index.symbol(*symbol).ends_with('/')));
        let Some(&(_, symbol)) = pick else { continue };
        let Some((target_path, _)) = defs.get(&symbol).cloned() else { continue };
        if target_path == *path {
            continue;
        }
        let text = index.symbol(symbol);
        let local = strings.lookup(specifier.name).to_string();
        if !text.ends_with('/') {
            facts.push(FlatFact::ResolvedImportRow {
                src_path: path.clone(),
                name: descriptor_name(text).unwrap_or_else(|| local.clone()),
                local: local.clone(),
                target_path: target_path.clone(),
                target_name: descriptor_name(text),
                kind: "local".to_string(),
                hops: 0,
            });
        }
        if modules.insert(target_path.clone()) {
            facts.push(FlatFact::ResolvedImportRow {
                src_path: path.clone(),
                name: local.clone(),
                local,
                target_path,
                target_name: None,
                kind: "module".to_string(),
                hops: 1,
            });
        }
    }
    facts
}

/// Rows of origin `checker` from the checkers compiled in, for the languages in
/// `files`; a present language without its checker is one stderr line.
fn checker_facts(files: &[PathBuf], root: &Path) -> Result<Vec<FlatFact>, ProjectError> {
    let present: BTreeSet<&str> = files
        .iter()
        .filter_map(|path| crate::read::lang::source_for(&path.to_string_lossy()).map(|src| src.name()))
        .collect();
    let compiled = |lang: &str| match lang {
        "rust" => cfg!(feature = "rust-checker"),
        "ts" => cfg!(feature = "ts-checker"),
        "go" => cfg!(feature = "go-checker"),
        _ => false,
    };
    let mut want = |lang: &'static str| {
        if !present.contains(lang) {
            return None;
        }
        if compiled(lang) {
            return Some(root);
        }
        // @eprintln-ok: CLI-UX note, off the fact stream.
        eprintln!("ryi slow: no {lang} checker in this build (cargo feature {lang}-checker)");
        None
    };
    let request = crate::read::project::ResolveRequest {
        paths: files,
        arms: crate::read::project::ResolveArms { call: true, types: true, flow: false },
        scip: crate::read::project::ScipMode::Off,
        project_root: None,
        scip_records: crate::read::scip_rows::ScipRecords::all(),
        occurrence_text: false,
        rust_checker: want("rust"),
        ts_checker: want("ts"),
        go_checker: want("go"),
        witness: false,
    };
    if request.rust_checker.is_none() && request.ts_checker.is_none() && request.go_checker.is_none() {
        return Ok(Vec::new());
    }
    let checker = ResolutionOrigin::Checker.as_str();
    Ok(crate::read::project::resolve_project(&request)?
        .into_iter()
        .filter(|fact| match fact {
            FlatFact::ResolvedEdge { resolution_origin, .. }
            | FlatFact::ResolvedTypeEdge { resolution_origin, .. } => resolution_origin == checker,
            _ => false,
        })
        .collect())
}
