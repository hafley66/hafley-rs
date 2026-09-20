//! `impl Rename for RustSource`: every question `extract rename` asks a language,
//! answered for Rust over `syn`, the parse `lang/rust.rs` already carries. Spans
//! are identifier spans bridged by `build_line_starts` (`rust.rs:57`) and
//! `syn_span` (`rust.rs:81`); no new crate.
//! @comment-ok: module header, the seam list every lang file opens with
//!
//! rustc's module-file law (crate roots, `mod.rs` owning its directory, a module
//! path read off the layout) is restated here rather than shared: the same law
//! sits in `rust_rehome.rs:685-1300` behind a `MoveCx`, and a rename carries a
//! `RenameCx`.
//!
//! Seats a run reports and never rewrites: a glob `use m::*` whose scope then
//! writes the bare name, and a `.old` member token when the anchor is a method.
//! A macro body's idents are classified by token context instead: a `::`
//! segment resolves on the scope plane, a `$old`/`'old` token never reaches the
//! symbol, a bare ident follows the bare-name law (shadowed, block-local, else
//! the scope binding).
//! @comment-ok: module header, the arm's seat and shadow laws
//!
//! Stated limits: a `let`/`for` binding shadows the bare name at BLOCK
//! granularity only; an owner segment (`Owner { .. }`, `Kind::Old`) never
//! reaches through a block-scoped `use`. @comment-ok: module header waiver

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Deserialize;
use syn::spanned::Spanned;

use super::rust::{build_line_starts, syn_span, RustSource};
use crate::move_cx::{dirname, join_rel, stem};
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::types::{RefRole, Rename, RenameStop, Respell, Span, SymbolRef, SymbolSeat};

impl Rename for RustSource {
    fn symbol_refs(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
    ) -> Result<Vec<SymbolRef>, RenameStop> {
        let corpus = Corpus::open(cx, &request.old);
        let anchor = corpus
            .scans
            .get(&request.anchor)
            .ok_or_else(|| not_found(request))?;
        // An item at the anchor's own module root is the one a `use` can reach;
        // an item nested in a `mod` block or a function body needs `--at`.
        let at_root: Vec<&Decl> = anchor
            .decls
            .iter()
            .filter(|decl| {
                decl.chain.is_empty() && decl.block.is_none() && !decl.kind.is_member()
            })
            .collect();
        let declaration = match (request.at, at_root.as_slice(), anchor.decls.as_slice()) {
            (_, _, []) => return Err(not_found(request)),
            (None, [one], _) => *one,
            (None, [], [one]) => one,
            (None, [], many) => {
                return Err(ambiguous(
                    request,
                    many.iter().map(|decl| decl.span).collect(),
                ))
            }
            (None, many, _) => {
                return Err(ambiguous(
                    request,
                    many.iter().map(|decl| decl.span).collect(),
                ))
            }
            (Some(_), _, many) => select_by_at(many, request.at)
                .ok_or_else(|| ambiguous(request, many.iter().map(|decl| decl.span).collect()))?,
        };
        let anchor_modules: Vec<ModuleId> = corpus
            .homes_of(&request.anchor)
            .iter()
            .map(|home| module_of(home, &declaration.chain))
            .collect();
        let nameable = corpus.nameable(&anchor_modules);
        let reexports =
            corpus.reexports(&nameable, (request.anchor.clone(), declaration.chain.clone()));

        let mut refs = vec![SymbolRef {
            file: request.anchor.clone(),
            span: declaration.span,
            role: RefRole::Definition,
            text: request.old.clone(),
        }];
        let mut seats: Vec<SymbolSeat> = Vec::new();
        for (rel, scan) in &corpus.scans {
            let anchored = (rel == &request.anchor).then_some(declaration);
            let line_starts = cx
                .text(rel)
                .map(|text| build_line_starts(&text))
                .unwrap_or_default();
            for home in corpus.homes_of(rel) {
                corpus.harvest(
                    rel, home, scan, &line_starts, &nameable, &reexports, anchored,
                    &declaration.kind, &anchor_modules, request, &mut refs, &mut seats,
                );
            }
        }
        if let Some(stop) = corpus.inexact(&refs) {
            return Err(stop);
        }
        if !seats.is_empty() {
            seats.sort_by(|left, right| {
                left.file
                    .cmp(&right.file)
                    .then(left.span.start.cmp(&right.span.start))
            });
            seats.dedup_by(|left, right| left.file == right.file && left.span == right.span);
            return Err(RenameStop::Dynamic(seats));
        }
        Ok(settle(refs))
    }

    fn respell_symbol(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
        reference: &SymbolRef,
    ) -> Option<Respell> {
        // `Owner { old }` respells to `new: old`, keeping the local name; the
        // typing walk proves the shape so a same-spelled use leaf never matches.
        let shorthand = match cx.text(&reference.file) {
            Some(source) => match syn::parse_file(&source) {
                Ok(parsed) => {
                    let line_starts = build_line_starts(&source);
                    field_sites(&parsed, &line_starts, &request.old)
                        .iter()
                        .any(|site| {
                            matches!(site,
                                FieldSite::Owner { span, shorthand: true, .. }
                                    if *span == reference.span)
                        })
                }
                Err(_) => false,
            },
            None => false,
        };
        let (text, receipt) = match shorthand {
            true => (
                format!("{}: {}", request.new, request.old),
                Some("shorthand".to_string()),
            ),
            false => (request.new.clone(), None),
        };
        Some(Respell {
            file: reference.file.clone(),
            span: reference.span,
            text,
            receipt,
        })
    }

    fn text_spellings(&self, cx: &RenameCx, request: &RenameRequest) -> Vec<(String, String)> {
        // A string literal is never a symbol, so serde and doc spellings ride
        // the report only; one corpus-wide pattern covers every such literal.
        let corpus = Corpus::open(cx, &request.old);
        match corpus.scans.values().any(|scan| scan.has_lit) {
            true => vec![(request.old.clone(), request.new.clone())],
            false => Vec::new(),
        }
    }
}

fn not_found(request: &RenameRequest) -> RenameStop {
    RenameStop::NotFound {
        anchor: request.anchor.clone(),
        old: request.old.clone(),
    }
}

fn ambiguous(request: &RenameRequest, sites: Vec<Span>) -> RenameStop {
    RenameStop::Ambiguous {
        anchor: request.anchor.clone(),
        old: request.old.clone(),
        sites,
    }
}

/// `--at` picks the declaration the offset lands in, else the nearest one
/// opening at or before it, the law `ts_rename.rs:111` states.
fn select_by_at(candidates: &[Decl], at: Option<u32>) -> Option<&Decl> {
    let at = at?;
    let inside: Vec<&Decl> = candidates
        .iter()
        .filter(|decl| decl.span.start <= at && at < decl.span.end())
        .collect();
    match inside.as_slice() {
        [one] => Some(one),
        [] => candidates
            .iter()
            .filter(|decl| decl.span.start <= at)
            .max_by_key(|decl| decl.span.start),
        _ => None,
    }
}

/// One seat per `(file, offset)`, in plan order: a `use` clause's trailing
/// segment and the binding walk can name the same token.
fn settle(mut refs: Vec<SymbolRef>) -> Vec<SymbolRef> {
    refs.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.span.start.cmp(&right.span.start))
    });
    refs.dedup_by(|left, right| left.file == right.file && left.span.start == right.span.start);
    refs
}

// ── the corpus view ─────────────────────────────────────────────────────────

/// One module: the crate-root file it answers to, and its path from that root.
type ModuleId = (String, Vec<String>);

fn module_of(home: &ModuleId, chain: &[String]) -> ModuleId {
    let mut path = home.1.clone();
    path.extend(chain.iter().cloned());
    (home.0.clone(), path)
}

/// Every Rust file that spells the old name, scanned once, plus the two layout
/// tables the module law reads.
struct Corpus {
    scans: BTreeMap<String, FileScan>,
    /// rel -> every module that file IS, in route order; never empty.
    homes: BTreeMap<String, Vec<ModuleId>>,
    /// A crate's identifier as a `use` writes it -> that crate's root file.
    crates: BTreeMap<String, String>,
}

impl Corpus {
    fn open(cx: &RenameCx, old: &str) -> Self {
        let roots = crate_roots(cx);
        let crates = crate_idents(cx);
        let path_mods = path_module_table(cx, &roots);
        let mut scans = BTreeMap::new();
        let mut homes = BTreeMap::new();
        for rel in cx.files_of(&RustSource) {
            let Some(text) = cx.text(rel) else {
                continue;
            };
            // A file that never spells the name seats it nowhere, and a re-export
            // chain writes the name at every hop, so the filter drops no seat.
            if !text.contains(old) {
                continue;
            }
            let Ok(parsed) = syn::parse_file(&text) else {
                continue;
            };
            let line_starts = build_line_starts(&text);
            let mut scan = Scan {
                old,
                source: &text,
                line_starts: &line_starts,
                chain: Vec::new(),
                blocks: Vec::new(),
                role: RefRole::TypeRef,
                out: FileScan::default(),
            };
            syn::visit::Visit::visit_file(&mut scan, &parsed);
            scan.out.field_sites = field_sites(&parsed, &line_starts, old);
            homes.insert(
                rel.to_string(),
                path_mods
                    .get(rel)
                    .cloned()
                    .unwrap_or_else(|| vec![module_path(rel, &roots)]),
            );
            scans.insert(rel.to_string(), scan.out);
        }
        Corpus {
            scans,
            homes,
            crates,
        }
    }

    /// The modules a file is. A file under no crate root answers to itself, so its
    /// own paths still resolve against each other.
    fn homes_of(&self, rel: &str) -> &[ModuleId] {
        static ORPHAN: [ModuleId; 1] = [(String::new(), Vec::new())];
        self.homes.get(rel).map_or(&ORPHAN, Vec::as_slice)
    }

    /// The `::`-prefix of a path, as a module. `None` when it climbs above a
    /// crate root, which names nothing.
    fn resolve(&self, home: &ModuleId, chain: &[String], prefix: &[String]) -> Option<ModuleId> {
        let mut here: Vec<String> = home.1.iter().chain(chain).cloned().collect();
        let mut rest = prefix;
        let mut root = home.0.clone();
        match rest.first().map(String::as_str) {
            Some("crate") => {
                here.clear();
                rest = &rest[1..];
            }
            Some("self") => {
                rest = &rest[1..];
            }
            Some("super") => {
                while rest.first().map(String::as_str) == Some("super") {
                    here.pop()?;
                    rest = &rest[1..];
                }
            }
            Some(name) if self.crates.contains_key(name) => {
                here.clear();
                root = self.crates.get(name)?.clone();
                rest = &rest[1..];
            }
            _ => {}
        }
        here.extend(rest.iter().cloned());
        Some((root, here))
    }

    /// Every module the symbol can be named from: the declaring one, plus a hop
    /// per public re-export under the SAME name, to a fixpoint.
    fn nameable(&self, anchors: &[ModuleId]) -> BTreeSet<ModuleId> {
        let mut set: BTreeSet<ModuleId> = anchors.iter().cloned().collect();
        loop {
            let mut grew = false;
            for (rel, scan) in &self.scans {
                for home in self.homes_of(rel) {
                    for leaf in &scan.uses {
                        if !leaf.exported
                            || !matches!(leaf.kind, LeafKind::Name | LeafKind::SelfName)
                        {
                            continue;
                        }
                        let Some(from) = self.resolve(home, &leaf.chain, &leaf.prefix) else {
                            continue;
                        };
                        if set.contains(&from) {
                            grew |= set.insert(module_of(home, &leaf.chain));
                        }
                    }
                }
            }
            if !grew {
                return set;
            }
        }
    }

    /// The file scope a module id names: the file whose home is its deepest
    /// module prefix, with the inline-mod chain left over.
    fn scope_of(&self, module: &ModuleId) -> Option<(String, Vec<String>)> {
        let (rel, home) = self
            .homes
            .iter()
            .flat_map(|(rel, homes)| homes.iter().map(move |home| (rel, home)))
            .filter(|(_, home)| home.0 == module.0 && module.1.starts_with(&home.1))
            .max_by_key(|(_, home)| home.1.len())?;
        Some((rel.clone(), module.1[home.1.len()..].to_vec()))
    }

    /// Scopes that bind the name, to a fixpoint: a named import binds directly,
    /// `use m::*` binds when m's scope binds. Seeded with the anchor's own scope.
    fn reexports(
        &self,
        nameable: &BTreeSet<ModuleId>,
        seed: (String, Vec<String>),
    ) -> BTreeMap<String, BTreeSet<Vec<String>>> {
        let mut binds: BTreeSet<(String, Vec<String>)> = BTreeSet::from([seed]);
        loop {
            let mut grew = false;
            for (rel, scan) in &self.scans {
                for home in self.homes_of(rel) {
                    for leaf in &scan.uses {
                        if leaf.block.is_some() {
                            continue;
                        }
                        let Some(target) = self.resolve(home, &leaf.chain, &leaf.prefix) else {
                            continue;
                        };
                        let binding = match leaf.kind {
                            LeafKind::Name | LeafKind::SelfName => nameable.contains(&target),
                            LeafKind::Glob if !nameable.contains(&target) => self
                                .scope_of(&target)
                                .is_some_and(|scope| binds.contains(&scope)),
                            _ => false,
                        };
                        if binding {
                            grew |= binds.insert((rel.clone(), leaf.chain.clone()));
                        }
                    }
                }
            }
            if !grew {
                break;
            }
        }
        let mut out: BTreeMap<String, BTreeSet<Vec<String>>> = BTreeMap::new();
        for (rel, chain) in binds {
            out.entry(rel).or_default().insert(chain);
        }
        out
    }

    /// One file's seats. `anchored` carries the declaration when this file is the
    /// anchor, so its own scope binds the name and its own ident is not a shadow.
    #[allow(clippy::too_many_arguments)]
    fn harvest(
        &self,
        rel: &str,
        home: &ModuleId,
        scan: &FileScan,
        line_starts: &[u32],
        nameable: &BTreeSet<ModuleId>,
        reexports: &BTreeMap<String, BTreeSet<Vec<String>>>,
        anchored: Option<&Decl>,
        anchor_kind: &DeclKind,
        anchor_modules: &[ModuleId],
        request: &RenameRequest,
        refs: &mut Vec<SymbolRef>,
        seats: &mut Vec<SymbolSeat>,
    ) {
        let mut ours: BTreeSet<&[String]> = BTreeSet::new();
        let mut shadowed: BTreeSet<&[String]> = BTreeSet::new();
        let mut shadow_blocks: Vec<Span> = Vec::new();
        let mut globs: BTreeMap<&[String], Vec<(Span, Option<Span>)>> = BTreeMap::new();
        // A fn-body `use` binds its name inside its block only: (scope, block)
        // pairs judged beside the module facts, never instead of them.
        let mut local_ours: Vec<(&[String], Span)> = Vec::new();

        for decl in &scan.decls {
            if decl.kind.is_member() {
                continue;
            }
            match (anchored, decl.block) {
                (Some(picked), _) if picked.span == decl.span => {
                    ours.insert(&picked.chain);
                }
                (_, Some(block)) => shadow_blocks.push(block),
                (_, None) => {
                    shadowed.insert(&decl.chain);
                }
            }
        }
        let inside_shadow_block = |blocks: &[Span], span: Span| {
            blocks.iter().any(|block| block.start <= span.start && span.end() <= block.end())
        };
        let inside_local_block = |blocks: &[Span], span: Span| {
            blocks.iter().any(|block| block.start <= span.start && span.end() <= block.end())
        };
        for leaf in &scan.uses {
            let reaches = self
                .resolve(home, &leaf.chain, &leaf.prefix)
                .is_some_and(|module| nameable.contains(&module));
            let binds_variant = match anchor_kind {
                DeclKind::Variant { owner } => {
                    self.variant_leaf(home, &leaf.chain, &leaf.prefix, owner, anchor_modules)
                }
                _ => false,
            };
            match leaf.kind {
                LeafKind::Name if reaches && leaf.block.is_none() => {
                    ours.insert(&leaf.chain);
                    refs.push(seat(rel, leaf.span, RefRole::Import, &request.old));
                }
                LeafKind::Name if reaches => {
                    local_ours.push((&leaf.chain, leaf.block.expect("a leaf in a block")));
                    refs.push(seat(rel, leaf.span, RefRole::Import, &request.old));
                }
                LeafKind::Name if binds_variant => {
                    ours.insert(&leaf.chain);
                    refs.push(seat(rel, leaf.span, RefRole::Import, &request.old));
                }
                LeafKind::Name => match leaf.block {
                    None => {
                        shadowed.insert(&leaf.chain);
                    }
                    Some(block) => shadow_blocks.push(block),
                },
                LeafKind::SelfName if reaches && leaf.block.is_none() => {
                    ours.insert(&leaf.chain);
                }
                LeafKind::SelfName if reaches => {
                    local_ours.push((&leaf.chain, leaf.block.expect("a leaf in a block")));
                }
                LeafKind::SelfName => match leaf.block {
                    None => {
                        shadowed.insert(&leaf.chain);
                    }
                    Some(block) => shadow_blocks.push(block),
                },
                LeafKind::Alias if reaches => {
                    refs.push(seat(rel, leaf.span, RefRole::Import, &request.old))
                }
                LeafKind::Shadow => match leaf.block {
                    None => {
                        shadowed.insert(&leaf.chain);
                    }
                    Some(block) => shadow_blocks.push(block),
                },
                LeafKind::Glob if reaches => {
                    globs.entry(&leaf.chain).or_default().push((leaf.item, leaf.block))
                }
                // A glob of a scope that binds the name re-exposes it here; the
                // glob itself has no token to rewrite.
                LeafKind::Glob
                    if leaf.block.is_none()
                        && reexports
                            .get(rel)
                            .is_some_and(|chains| chains.contains(&leaf.chain)) =>
                {
                    ours.insert(&leaf.chain);
                }
                _ => {}
            }
        }

        for path in &scan.paths {
            if !path.prefix.is_empty() {
                let variant = match anchor_kind {
                    DeclKind::Variant { owner } => self.owner_reach(
                        home, &path.chain, &path.prefix, owner, anchor_modules, nameable, scan,
                    ),
                    _ => false,
                };
                if variant
                    || self
                        .resolve(home, &path.chain, &path.prefix)
                        .is_some_and(|module| nameable.contains(&module))
                {
                    refs.push(seat(rel, path.span, path.role, &request.old));
                }
                continue;
            }
            if local_ours.iter().any(|(chain, block)| {
                *chain == path.chain.as_slice() && block.start <= path.span.start
                    && path.span.end() <= block.end()
            }) {
                refs.push(seat(rel, path.span, path.role, &request.old));
                continue;
            }
            if shadowed.contains(path.chain.as_slice())
                || inside_shadow_block(&shadow_blocks, path.span)
                || (path.bare && inside_local_block(&scan.locals, path.span))
            {
                continue;
            }
            if ours.contains(path.chain.as_slice()) {
                refs.push(seat(rel, path.span, path.role, &request.old));
                continue;
            }
            // A glob whose scope never writes the bare name survives the rename
            // untouched, so only a scope that writes it stops.
            for (span, bound) in globs.get(path.chain.as_slice()).into_iter().flatten() {
                if bound.is_some_and(|block| block.start > path.span.start || path.span.end() > block.end()) {
                    continue;
                }
                seats.push(SymbolSeat {
                    file: rel.to_string(),
                    span: *span,
                    line: line_starts.partition_point(|start| *start <= span.start) as u32,
                    reaches: String::new(),
                    form: "glob import",
                });
            }
        }

        // Field anchors seat the accesses the same-file typing walk proves and
        // the owner keys the owner law proves; an access nothing types stops.
        if let DeclKind::Field { owner } = anchor_kind {
            for site in &scan.field_sites {
                match site {
                    FieldSite::Access { span, ty: Some(ty), write } if ty == owner => {
                        let role = match write {
                            true => RefRole::Write,
                            false => RefRole::Read,
                        };
                        refs.push(seat(rel, *span, role, &request.old));
                    }
                    FieldSite::Access { span, ty: None, .. } => seats.push(SymbolSeat {
                        file: rel.to_string(),
                        span: *span,
                        line: line_starts.partition_point(|start| *start <= span.start) as u32,
                        reaches: String::new(),
                        form: "untyped field",
                    }),
                    FieldSite::Owner { span, chain, prefix, owner: site_owner, pattern, .. }
                        if site_owner == owner
                            && self.owner_reach(
                                home,
                                chain,
                                &owner_path(prefix, site_owner),
                                owner,
                                anchor_modules,
                                nameable,
                                scan,
                            ) =>
                    {
                        let role = match pattern {
                            true => RefRole::Read,
                            false => RefRole::Write,
                        };
                        refs.push(seat(rel, *span, role, &request.old));
                    }
                    _ => {}
                }
            }
        }

        // A `::` token in a macro body is a path segment like any other; a local
        // binding cannot shadow it, only a same-named item can.
        for token in &scan.opaque {
            let Some(prefix) = token.prefix.as_deref() else {
                continue;
            };
            if !prefix.is_empty() {
                if self
                    .resolve(home, &token.chain, prefix)
                    .is_some_and(|module| nameable.contains(&module))
                {
                    refs.push(seat(rel, token.span, RefRole::Read, &request.old));
                }
                continue;
            }
            if shadowed.contains(token.chain.as_slice()) {
                continue;
            }
            if ours.contains(token.chain.as_slice()) {
                refs.push(seat(rel, token.span, RefRole::Read, &request.old));
                continue;
            }
            for (span, bound) in globs.get(token.chain.as_slice()).into_iter().flatten() {
                if bound.is_some_and(|block| block.start > token.span.start || token.span.end() > block.end()) {
                    continue;
                }
                seats.push(SymbolSeat {
                    file: rel.to_string(),
                    span: *span,
                    line: line_starts.partition_point(|start| *start <= span.start) as u32,
                    reaches: String::new(),
                    form: "glob import",
                });
            }
        }

        // `.old` inside a macro cannot be resolved to the anchor unless the
        // anchor IS a member the token could reach.
        let member_anchor = anchored.is_some_and(|decl| match decl.kind {
            DeclKind::Method => !ours.is_empty(),
            DeclKind::Field { .. } | DeclKind::Variant { .. } => true,
            DeclKind::Item => false,
        });
        if member_anchor {
            for token in &scan.opaque {
                if token.member {
                    seats.push(SymbolSeat {
                        file: rel.to_string(),
                        span: token.span,
                        line: line_starts.partition_point(|start| *start <= token.span.start)
                            as u32,
                        reaches: String::new(),
                        form: "macro body",
                    });
                }
            }
        }
        if ours.is_empty() {
            return;
        }
        for token in &scan.opaque {
            if token.prefix.is_some() || token.member {
                continue;
            }
            if shadowed.contains(token.chain.as_slice()) || inside_local_block(&scan.locals, token.span) {
                continue;
            }
            let locally = local_ours.iter().any(|(chain, block)| {
                *chain == token.chain.as_slice() && block.start <= token.span.start
                    && token.span.end() <= block.end()
            });
            if ours.contains(token.chain.as_slice()) || locally {
                refs.push(seat(rel, token.span, RefRole::Read, &request.old));
                continue;
            }
            for (span, bound) in globs.get(token.chain.as_slice()).into_iter().flatten() {
                if bound.is_some_and(|block| block.start > token.span.start || token.span.end() > block.end()) {
                    continue;
                }
                seats.push(SymbolSeat {
                    file: rel.to_string(),
                    span: *span,
                    line: line_starts.partition_point(|start| *start <= span.start) as u32,
                    reaches: String::new(),
                    form: "glob import",
                });
            }
        }
        if anchored.is_some_and(|decl| matches!(decl.kind, DeclKind::Method)) {
            for span in &scan.methods {
                refs.push(seat(rel, *span, RefRole::Read, &request.old));
            }
        }
    }

    /// Whether a `use` leaf's prefix ends at the anchor enum: `use Kind::Old;`
    /// resolves the segments before `owner` to the enum's module.
    fn variant_leaf(
        &self,
        home: &ModuleId,
        chain: &[String],
        prefix: &[String],
        owner: &str,
        anchors: &[ModuleId],
    ) -> bool {
        match prefix.split_last() {
            Some((last, before)) if last == owner => self
                .resolve(home, chain, before)
                .is_some_and(|module| anchors.contains(&module)),
            _ => false,
        }
    }

    /// Whether a path's owner segment names the anchor's owner: the segments
    /// before it resolve to the anchor module, or a module-scope `use` binds it.
    #[allow(clippy::too_many_arguments)]
    fn owner_reach(
        &self,
        home: &ModuleId,
        chain: &[String],
        prefix: &[String],
        owner: &str,
        anchors: &[ModuleId],
        nameable: &BTreeSet<ModuleId>,
        scan: &FileScan,
    ) -> bool {
        let Some((last, before)) = prefix.split_last() else {
            return false;
        };
        if last != owner {
            return false;
        }
        match self.resolve(home, chain, before) {
            Some(module) if anchors.contains(&module) => true,
            Some(_) if before.is_empty() => scan.owner_leaves.iter().any(|leaf| {
                leaf.name == owner
                    && leaf.block.is_none()
                    && self
                        .resolve(home, &leaf.chain, &leaf.prefix)
                        .is_some_and(|module| nameable.contains(&module))
            }),
            _ => false,
        }
    }

    /// The first span in a touched file whose bytes are not the old name. One
    /// such span means every span in that file is suspect, so the run stops.
    fn inexact(&self, refs: &[SymbolRef]) -> Option<RenameStop> {
        let touched: BTreeSet<&str> = refs
            .iter()
            .map(|reference| reference.file.as_str())
            .collect();
        touched
            .into_iter()
            .find_map(|rel| Some((rel, *self.scans.get(rel)?.inexact.first()?)))
            .map(|(rel, span)| RenameStop::Inexact {
                file: rel.to_string(),
                span,
                why: "a syn char column does not read back as the identifier",
            })
    }
}

/// `owner_reach` wants `prefix` ending in the owner segment; a `FieldSite::Owner`'s
/// own `prefix` sits before the owner, so this appends it first.
fn owner_path(prefix: &[String], owner: &str) -> Vec<String> {
    let mut path = prefix.to_vec();
    path.push(owner.to_string());
    path
}

fn seat(rel: &str, span: Span, role: RefRole, old: &str) -> SymbolRef {
    SymbolRef {
        file: rel.to_string(),
        span,
        role,
        text: old.to_string(),
    }
}

// ── the syn scan ────────────────────────────────────────────────────────────

/// One file's old-name seats, off ONE `syn::parse_file`.
#[derive(Default)]
struct FileScan {
    decls: Vec<Decl>,
    uses: Vec<UseLeaf>,
    paths: Vec<PathSeat>,
    /// `x.old()` receivers, kept for a request whose anchor IS a method.
    methods: Vec<Span>,
    /// The name written as an identifier token inside a macro or attribute body.
    opaque: Vec<OpaqueToken>,
    /// Blocks that bind the name as a local (`let`, `for`).
    locals: Vec<Span>,
    /// Field-shaped spellings the typing walk recorded.
    field_sites: Vec<FieldSite>,
    /// Every `use` binding of any name, the owner law's raw material.
    owner_leaves: Vec<OwnerLeaf>,
    /// A string literal spelling the name, the `--text-refs` gate.
    has_lit: bool,
    inexact: Vec<Span>,
}

/// A `use` clause binding ANY name in this scope, the raw material the owner
/// law (`Owner { .. }`, `Kind::Old`) reads.
struct OwnerLeaf {
    name: String,
    /// The `::`-segments before the bound name.
    prefix: Vec<String>,
    chain: Vec<String>,
    block: Option<Span>,
}

/// One `old` ident inside a token stream, with the context the scope plane
/// classifies on.
struct OpaqueToken {
    span: Span,
    chain: Vec<String>,
    /// The `::`-segments before it when the NEXT token is `::` (empty = first
    /// segment); None = no `::` follows.
    prefix: Option<Vec<String>>,
    /// A `.old` member mention.
    member: bool,
}

/// One declaration of the name: the item ident's own span, and the inline
/// `mod x { .. }` blocks enclosing it, outermost first.
struct Decl {
    chain: Vec<String>,
    span: Span,
    /// What the ident declares, which decides the seat laws it reads.
    kind: DeclKind,
    /// The innermost block a function-body item is declared in: it shadows the
    /// name inside that block only. None = declared at module scope.
    block: Option<Span>,
}

enum DeclKind {
    Item,
    /// Declared in an `impl` or `trait` block, so call sites spell it as a method.
    Method,
    /// A named field of the struct/union/variant `owner`.
    Field { owner: String },
    /// A variant of the enum `owner`.
    Variant { owner: String },
}

impl DeclKind {
    /// Never a scope binding: a field or variant is nested in its item, so it
    /// neither shadows bare names nor wins the no-`--at` selection.
    fn is_member(&self) -> bool {
        matches!(self, DeclKind::Field { .. } | DeclKind::Variant { .. })
    }
}

/// One `use` clause naming the symbol, or a glob that could reach it.
struct UseLeaf {
    chain: Vec<String>,
    /// The `::`-segments before the named one; a glob's is the whole starred path.
    prefix: Vec<String>,
    /// The named identifier's own span; a glob names none.
    span: Span,
    kind: LeafKind,
    /// The whole `use` item, which is what a glob stop reports.
    item: Span,
    exported: bool,
    /// The fn body the clause sits in: it binds inside that block only.
    block: Option<Span>,
}

enum LeafKind {
    /// `use P::OLD;` names the symbol and binds `OLD` here.
    Name,
    /// `use P::OLD as local;` names the symbol and binds `local`.
    Alias,
    /// `use P::OLD::{self}` binds the module's own name; the `self` token never
    /// respells.
    SelfName,
    /// `use P::other as OLD;` binds the name to something else.
    Shadow,
    /// `use P::*;`
    Glob,
}

/// One path segment spelling the name, and the segments before it.
struct PathSeat {
    chain: Vec<String>,
    prefix: Vec<String>,
    span: Span,
    role: RefRole,
    /// The whole path is this one segment, so a block-local binding shadows it.
    bare: bool,
}

struct Scan<'a> {
    old: &'a str,
    source: &'a str,
    line_starts: &'a [u32],
    chain: Vec<String>,
    blocks: Vec<Span>,
    role: RefRole,
    out: FileScan,
}

impl Scan<'_> {
    /// A span that reads back as the old name, else a recorded `inexact`.
    fn exact(&mut self, span: proc_macro2::Span) -> Option<Span> {
        let span = syn_span(self.line_starts, span);
        let start = span.start as usize;
        match self.source.get(start..start + span.len as usize) {
            Some(text) if text == self.old => Some(span),
            _ => {
                self.out.inexact.push(span);
                None
            }
        }
    }

    fn declare(&mut self, ident: &proc_macro2::Ident, kind: DeclKind) {
        if ident != self.old {
            return;
        }
        let Some(span) = self.exact(ident.span()) else {
            return;
        };
        self.out.decls.push(Decl {
            chain: self.chain.clone(),
            span,
            kind,
            block: self.blocks.last().copied(),
        });
    }

    /// Every named field of a struct-shaped item declares under its owner: a
    /// struct-shaped variant's owner is the variant, whose path it wears.
    fn declare_fields(&mut self, fields: &syn::FieldsNamed, owner: &str) {
        for field in &fields.named {
            if let Some(ident) = &field.ident {
                self.declare(ident, DeclKind::Field { owner: owner.to_string() });
            }
        }
    }

    /// Identifier tokens in a stream the walk cannot parse, classified by their
    /// token context. A rewrite can now enter, so every span is `exact`-checked.
    fn tokens(&mut self, stream: proc_macro2::TokenStream) {
        let trees: Vec<proc_macro2::TokenTree> = stream.into_iter().collect();
        let is_colon = |tree: Option<&proc_macro2::TokenTree>| {
            matches!(tree, Some(proc_macro2::TokenTree::Punct(punct)) if punct.as_char() == ':')
        };
        let is_dot = |tree: Option<&proc_macro2::TokenTree>| {
            matches!(tree, Some(proc_macro2::TokenTree::Punct(punct)) if punct.as_char() == '.')
        };
        for (index, tree) in trees.iter().enumerate() {
            match tree {
                proc_macro2::TokenTree::Ident(ident) if ident == self.old => {
                    // `$old` names a metavariable, `'old` a label or lifetime.
                    if matches!(trees.get(index.wrapping_sub(1)), Some(token)
                        if matches!(token, proc_macro2::TokenTree::Punct(punct)
                            if punct.as_char() == '$' || punct.as_char() == '\''))
                    {
                        continue;
                    }
                    let Some(span) = self.exact(ident.span()) else {
                        continue;
                    };
                    let (prefix, member) = match is_colon(trees.get(index + 1)) {
                        true => (Some(path_prefix(&trees, index)), false),
                        false => (None, is_dot(trees.get(index.wrapping_sub(1)))),
                    };
                    self.out.opaque.push(OpaqueToken {
                        span,
                        chain: self.chain.clone(),
                        prefix,
                        member,
                    });
                }
                proc_macro2::TokenTree::Group(group) => self.tokens(group.stream()),
                proc_macro2::TokenTree::Literal(literal) => {
                    if literal.to_string() == format!("{:?}", self.old) {
                        self.out.has_lit = true;
                    }
                }
                _ => {}
            }
        }
    }

    /// A `let`/`for` binding of the name shadows the bare name inside its block.
    fn local(&mut self, pat: &syn::Pat, block: Option<Span>) {
        if binds_name(pat, self.old) {
            if let Some(block) = block {
                self.out.locals.push(block);
            }
        }
    }
}

/// The `::`-joined segments before `trees[index]`, walking back while
/// `ident : :` precedes the cursor: `crate::air::` yields `[crate, air]`.
fn path_prefix(trees: &[proc_macro2::TokenTree], index: usize) -> Vec<String> {
    let colon = |trees: &[proc_macro2::TokenTree], at: usize| {
        matches!(trees.get(at), Some(proc_macro2::TokenTree::Punct(punct)) if punct.as_char() == ':')
    };
    let mut prefix = Vec::new();
    let mut cursor = index;
    while cursor >= 3 {
        let Some(proc_macro2::TokenTree::Ident(ident)) = trees.get(cursor - 3) else {
            break;
        };
        if !(colon(trees, cursor - 1) && colon(trees, cursor - 2)) {
            break;
        }
        prefix.push(ident.to_string());
        cursor -= 3;
    }
    prefix.reverse();
    prefix
}

/// Whether a pattern binds the name, through the wrapper shapes a `let` or
/// `for` carries. Match-arm and nested-struct patterns are not walked.
fn binds_name(pat: &syn::Pat, old: &str) -> bool {
    match pat {
        syn::Pat::Ident(pat) => pat.ident == old,
        syn::Pat::Paren(pat) => binds_name(&pat.pat, old),
        syn::Pat::Reference(pat) => binds_name(&pat.pat, old),
        syn::Pat::Type(pat) => binds_name(&pat.pat, old),
        syn::Pat::Tuple(pat) => pat.elems.iter().any(|elem| binds_name(elem, old)),
        syn::Pat::Slice(pat) => pat.elems.iter().any(|elem| binds_name(elem, old)),
        syn::Pat::Or(pat) => pat.cases.iter().any(|case| binds_name(case, old)),
        _ => false,
    }
}

// ── the field typing walk ────────────────────────────────────────────────────

/// One field-shaped mention of the name the typing walk recorded.
enum FieldSite {
    /// `x.old`: the same-file type of `x`, or unknown.
    Access {
        span: Span,
        ty: Option<String>,
        /// The access is the assigned-to side of an assignment.
        write: bool,
    },
    /// `Owner { old .. }` in a literal or a pattern: the owner segment as
    /// written, the segments before it, and which shape.
    Owner {
        span: Span,
        chain: Vec<String>,
        prefix: Vec<String>,
        owner: String,
        shorthand: bool,
        pattern: bool,
    },
}

/// How one local name is typed: the same-file rules name a struct, or the
/// binding stays unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TypeBinding {
    Named(String),
    Unknown,
}

/// The same-file typing walk behind field seats, mirroring `rust_receivers.rs:9`.
/// Everything else stays Unknown, and Unknown is a stop.
struct FieldWalk<'a> {
    old: &'a str,
    line_starts: &'a [u32],
    /// Same-file fn name -> declared return type.
    rets: HashMap<String, String>,
    /// (impl self type, method) -> declared return type, `Self` resolved.
    assoc_rets: HashMap<(String, String), String>,
    /// (struct, field) -> declared type, same-file.
    fields: HashMap<(String, String), String>,
    /// Enclosing impl self types, outermost first.
    impl_stack: Vec<String>,
    scopes: Vec<HashMap<String, TypeBinding>>,
    chain: Vec<String>,
    /// Set while visiting the assigned-to side of an assignment.
    write: bool,
    out: Vec<FieldSite>,
}

/// The field-shaped spellings of `old` in one file, off the same-file typing
/// rules the receiver plane uses plus the literal's owner.
fn field_sites(parsed: &syn::File, line_starts: &[u32], old: &str) -> Vec<FieldSite> {
    let mut walk = FieldWalk {
        old,
        line_starts,
        rets: Default::default(),
        assoc_rets: Default::default(),
        fields: Default::default(),
        impl_stack: Vec::new(),
        scopes: Vec::new(),
        chain: Vec::new(),
        write: false,
        out: Vec::new(),
    };
    field_tables(&parsed.items, &mut walk.rets, &mut walk.assoc_rets, &mut walk.fields);
    syn::visit::Visit::visit_file(&mut walk, parsed);
    walk.out
}

/// The same-file type tables, the receiver plane's `tables` restated: fn
/// returns, associated returns, and struct fields, through inline mods.
fn field_tables(
    items: &[syn::Item],
    rets: &mut HashMap<String, String>,
    assoc_rets: &mut HashMap<(String, String), String>,
    fields: &mut HashMap<(String, String), String>,
) {
    for item in items {
        match item {
            syn::Item::Fn(f) => {
                if let Some(ty) = output_ty(&f.sig) {
                    rets.entry(f.sig.ident.to_string()).or_insert(ty);
                }
            }
            syn::Item::Impl(imp) => {
                let Some(self_type) = principal_ty(&imp.self_ty) else {
                    continue;
                };
                for item in &imp.items {
                    if let syn::ImplItem::Fn(f) = item {
                        let Some(ty) = output_ty(&f.sig) else {
                            continue;
                        };
                        let ty = if ty == "Self" { self_type.clone() } else { ty };
                        rets.entry(f.sig.ident.to_string()).or_insert(ty.clone());
                        assoc_rets
                            .entry((self_type.clone(), f.sig.ident.to_string()))
                            .or_insert(ty);
                    }
                }
            }
            syn::Item::Struct(s) => {
                let struct_name = s.ident.to_string();
                if let syn::Fields::Named(named) = &s.fields {
                    for field in &named.named {
                        if let (Some(field_name), Some(ty)) = (&field.ident, principal_ty(&field.ty))
                        {
                            fields
                                .entry((struct_name.clone(), field_name.to_string()))
                                .or_insert(ty);
                        }
                    }
                }
            }
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    field_tables(inner, rets, assoc_rets, fields);
                }
            }
            _ => {}
        }
    }
}

impl FieldWalk<'_> {
    /// Writes a binding, demoting to Unknown when the scopes disagree.
    fn insert(&mut self, name: String, binding: TypeBinding) {
        let held = self.lookup(&name).cloned();
        let merged = match held {
            Some(held) if held != binding => TypeBinding::Unknown,
            _ => binding,
        };
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, merged);
        }
    }

    /// The innermost scope that binds the name.
    fn lookup(&self, name: &str) -> Option<&TypeBinding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    /// `Self` in an impl body is the impl's self type.
    fn resolve_self(&self, ty: &str) -> Option<String> {
        match ty {
            "Self" => self.impl_stack.last().cloned(),
            other => Some(other.to_string()),
        }
    }

    /// Types a fn's params from annotations, falling back to a single trait
    /// bound (`T: Clone` types T as the trait).
    fn seed_params(&mut self, sig: &syn::Signature) {
        let bounds = trait_bounds_of_generics(&sig.generics);
        for input in &sig.inputs {
            let syn::FnArg::Typed(arg) = input else {
                continue;
            };
            let syn::Pat::Ident(pat) = &*arg.pat else {
                continue;
            };
            let Some(ty) = principal_ty(&arg.ty) else {
                continue;
            };
            let binding = bounds
                .get(&ty)
                .and_then(|bounds| match bounds.as_slice() {
                    [trait_name] => Some(TypeBinding::Named(trait_name.clone())),
                    _ => None,
                })
                .unwrap_or_else(|| TypeBinding::Named(ty));
            self.insert(pat.ident.to_string(), binding);
        }
    }

    /// The type a receiver expression spells: a bare ident or `self` only.
    fn base_ty(&self, expr: &syn::Expr) -> Option<String> {
        match expr {
            syn::Expr::Path(path) if path.path.segments.len() == 1 => {
                let ident = path.path.segments[0].ident.to_string();
                if ident == "self" {
                    return self.impl_stack.last().cloned();
                }
                None
            }
            _ => None,
        }
    }

    /// The type an expression carries when it sits in a `let` init: the call
    /// shapes unwrap to what they produce, a struct literal to its owner.
    fn init_ty(&self, init: Option<&syn::Expr>) -> Option<String> {
        let mut current = init?;
        loop {
            match current {
                syn::Expr::Paren(inner) => current = inner.expr.as_ref(),
                syn::Expr::Reference(inner) => current = inner.expr.as_ref(),
                syn::Expr::Try(inner) => current = inner.expr.as_ref(),
                syn::Expr::Await(inner) => current = inner.base.as_ref(),
                _ => break,
            }
        }
        match current {
            syn::Expr::Call(call) => {
                let syn::Expr::Path(path) = call.func.as_ref() else {
                    return None;
                };
                let name = path.path.segments.last()?.ident.to_string();
                let count = path.path.segments.len();
                if count == 1 {
                    return self.rets.get(&name).cloned();
                }
                let owner = path
                    .path
                    .segments
                    .iter()
                    .take(count - 1)
                    .rev()
                    .map(|segment| segment.ident.to_string())
                    .find(|segment| segment.chars().next().is_some_and(char::is_uppercase))?;
                let owner = self.resolve_self(&owner)?;
                self.assoc_rets
                    .get(&(owner.clone(), name.clone()))
                    .cloned()
                    .or_else(|| (name == "new").then_some(owner))
            }
            syn::Expr::MethodCall(call) => {
                let receiver = self.value_ty(&call.receiver)?;
                self.assoc_rets.get(&(receiver, call.method.to_string())).cloned()
            }
            // `let h = Helper { .. }` types h by the literal's owner.
            syn::Expr::Struct(literal) => {
                let owner = literal.path.segments.last()?.ident.to_string();
                self.resolve_self(&owner)
            }
            other => self.expr_ty(other),
        }
    }

    /// The type an expression carries wherever it sits.
    fn expr_ty(&self, expr: &syn::Expr) -> Option<String> {
        match expr {
            syn::Expr::Path(path) if path.path.segments.len() == 1 => {
                let ident = path.path.segments[0].ident.to_string();
                if ident == "self" {
                    return self.impl_stack.last().cloned();
                }
                match self.lookup(&ident) {
                    Some(TypeBinding::Named(ty)) => self.resolve_self(ty),
                    _ => None,
                }
            }
            syn::Expr::Field(field) => {
                let syn::Member::Named(ident) = &field.member else {
                    return None;
                };
                let base = self.expr_ty(&field.base)?;
                self.fields
                    .get(&(base, ident.to_string()))
                    .cloned()
                    .and_then(|ty| self.resolve_self(&ty))
            }
            syn::Expr::Call(_)
            | syn::Expr::MethodCall(_)
            | syn::Expr::Paren(_)
            | syn::Expr::Reference(_)
            | syn::Expr::Try(_)
            | syn::Expr::Await(_) => self.init_ty(Some(expr)),
            _ => None,
        }
    }

    /// The type of an expression an access reads through: references, parens,
    /// and one deref layer off.
    fn value_ty(&self, expr: &syn::Expr) -> Option<String> {
        let mut current = expr;
        loop {
            match current {
                syn::Expr::Reference(inner) => current = inner.expr.as_ref(),
                syn::Expr::Paren(inner) => current = inner.expr.as_ref(),
                syn::Expr::Group(inner) => current = inner.expr.as_ref(),
                syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Deref(_)) => {
                    current = unary.expr.as_ref();
                }
                syn::Expr::Path(path) => {
                    let segments = &path.path.segments;
                    if segments.len() == 1 && segments[0].ident == "self" {
                        return self.impl_stack.last().cloned();
                    }
                    let last = segments.last()?.ident.to_string();
                    let bound = match self.lookup(&last) {
                        Some(TypeBinding::Named(ty)) => self.resolve_self(ty),
                        _ => None,
                    };
                    return match bound {
                        Some(ty) => Some(ty),
                        None if self.lookup(&last).is_none()
                            && last.chars().next().is_some_and(char::is_uppercase) =>
                        {
                            Some(last)
                        }
                        None => None,
                    };
                }
                syn::Expr::Field(field) => {
                    let syn::Member::Named(ident) = &field.member else {
                        return None;
                    };
                    let base = self.base_ty(&field.base)?;
                    return self
                        .fields
                        .get(&(base, ident.to_string()))
                        .cloned()
                        .and_then(|ty| self.resolve_self(&ty));
                }
                other => return self.expr_ty(other),
            }
        }
    }

    /// Records an `Owner { .. }` site: the owner segment as written and the
    /// segments before it, for the owner law to judge.
    fn push_owner(&mut self, path: &syn::Path, ident: &proc_macro2::Ident, shorthand: bool, pattern: bool) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        let Some(owner) = segments.last().cloned() else {
            return;
        };
        self.out.push(FieldSite::Owner {
            span: syn_span(self.line_starts, ident.span()),
            chain: self.chain.clone(),
            prefix: segments[..segments.len() - 1].to_vec(),
            owner,
            shorthand,
            pattern,
        });
    }
}

impl<'ast> syn::visit::Visit<'ast> for FieldWalk<'_> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let inline = node.content.is_some();
        if inline {
            self.chain.push(node.ident.to_string());
        }
        syn::visit::visit_item_mod(self, node);
        if inline {
            self.chain.pop();
        }
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let self_type = principal_ty(&node.self_ty);
        if let Some(ty) = self_type.clone() {
            self.impl_stack.push(ty);
        }
        syn::visit::visit_item_impl(self, node);
        if self_type.is_some() {
            self.impl_stack.pop();
        }
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.scopes.push(Default::default());
        self.seed_params(&node.sig);
        syn::visit::visit_item_fn(self, node);
        self.scopes.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.scopes.push(Default::default());
        self.seed_params(&node.sig);
        syn::visit::visit_impl_item_fn(self, node);
        self.scopes.pop();
    }

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        self.scopes.push(Default::default());
        for input in &node.inputs {
            if let syn::Pat::Ident(pat) = input {
                self.insert(pat.ident.to_string(), TypeBinding::Unknown);
            }
        }
        syn::visit::visit_expr_closure(self, node);
        self.scopes.pop();
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        let bound = match &node.pat {
            syn::Pat::Ident(pat) => Some((
                pat.ident.to_string(),
                self.init_ty(node.init.as_ref().map(|init| init.expr.as_ref()))
                    .map(TypeBinding::Named)
                    .unwrap_or(TypeBinding::Unknown),
            )),
            syn::Pat::Type(pat) => match &*pat.pat {
                syn::Pat::Ident(inner) => Some((
                    inner.ident.to_string(),
                    principal_ty(&pat.ty)
                        .or_else(|| {
                            self.init_ty(node.init.as_ref().map(|init| init.expr.as_ref()))
                        })
                        .map(TypeBinding::Named)
                        .unwrap_or(TypeBinding::Unknown),
                )),
                _ => None,
            },
            _ => None,
        };
        syn::visit::visit_local(self, node);
        if let Some((name, binding)) = bound {
            self.insert(name, binding);
        }
    }

    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        self.write = true;
        syn::visit::visit_expr(self, &node.left);
        self.write = false;
        syn::visit::visit_expr(self, &node.right);
    }

    fn visit_expr_binary(&mut self, node: &'ast syn::ExprBinary) {
        let assigned = matches!(
            node.op,
            syn::BinOp::AddAssign(_)
                | syn::BinOp::SubAssign(_)
                | syn::BinOp::MulAssign(_)
                | syn::BinOp::DivAssign(_)
                | syn::BinOp::RemAssign(_)
                | syn::BinOp::BitXorAssign(_)
                | syn::BinOp::BitAndAssign(_)
                | syn::BinOp::BitOrAssign(_)
                | syn::BinOp::ShlAssign(_)
                | syn::BinOp::ShrAssign(_)
        );
        if !assigned {
            syn::visit::visit_expr_binary(self, node);
            return;
        }
        self.write = true;
        syn::visit::visit_expr(self, &node.left);
        self.write = false;
        syn::visit::visit_expr(self, &node.right);
    }

    fn visit_expr_field(&mut self, node: &'ast syn::ExprField) {
        if let syn::Member::Named(ident) = &node.member {
            if ident == self.old {
                self.out.push(FieldSite::Access {
                    span: syn_span(self.line_starts, ident.span()),
                    ty: self.value_ty(&node.base),
                    write: self.write,
                });
            }
        }
        syn::visit::visit_expr_field(self, node);
    }

    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        for field in &node.fields {
            let syn::Member::Named(ident) = &field.member else {
                continue;
            };
            if ident != self.old {
                continue;
            }
            let shorthand = matches!(&field.expr, syn::Expr::Path(value)
                if value.path.segments.len() == 1 && value.path.segments[0].ident == *ident);
            self.push_owner(&node.path, ident, shorthand, false);
        }
        syn::visit::visit_expr_struct(self, node);
    }

    fn visit_pat_struct(&mut self, node: &'ast syn::PatStruct) {
        for field in &node.fields {
            let syn::Member::Named(ident) = &field.member else {
                continue;
            };
            if ident != self.old {
                continue;
            }
            let shorthand = matches!(&*field.pat, syn::Pat::Ident(pat) if pat.ident == *ident);
            self.push_owner(&node.path, ident, shorthand, true);
        }
        syn::visit::visit_pat_struct(self, node);
    }
}

/// The name a type spells for this plane: a path's last segment, through the
/// wrappers an annotation wears. `Self` stays `Self` for the impl stack.
fn principal_ty(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Reference(inner) => principal_ty(&inner.elem),
        syn::Type::Slice(inner) => principal_ty(&inner.elem),
        syn::Type::Array(inner) => principal_ty(&inner.elem),
        syn::Type::Paren(inner) => principal_ty(&inner.elem),
        syn::Type::Group(inner) => principal_ty(&inner.elem),
        syn::Type::Ptr(inner) => principal_ty(&inner.elem),
        _ => None,
    }
}

/// A signature's declared return type.
fn output_ty(sig: &syn::Signature) -> Option<String> {
    match &sig.output {
        syn::ReturnType::Type(_, ty) => principal_ty(ty),
        syn::ReturnType::Default => None,
    }
}

/// A generic param's bound traits (`T: A + B` -> both), by param name.
fn trait_bounds_of_generics(generics: &syn::Generics) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for param in &generics.params {
        let syn::GenericParam::Type(param) = param else {
            continue;
        };
        let name = param.ident.to_string();
        for bound in &param.bounds {
            if let Some(trait_name) = single_bound_trait(bound) {
                out.entry(name.clone()).or_default().push(trait_name);
            }
        }
    }
    out
}

/// The trait a bound names, by its last segment.
fn single_bound_trait(bound: &syn::TypeParamBound) -> Option<String> {
    let syn::TypeParamBound::Trait(trait_bound) = bound else {
        return None;
    };
    let last = trait_bound.path.segments.last()?;
    Some(last.ident.to_string())
}

impl<'ast> syn::visit::Visit<'ast> for Scan<'_> {
    fn visit_item(&mut self, node: &'ast syn::Item) {
        if let Some(ident) = item_ident(node) {
            self.declare(ident, DeclKind::Item);
        }
        syn::visit::visit_item(self, node);
    }

    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.blocks
            .push(syn_span(self.line_starts, node.brace_token.span.join()));
        syn::visit::visit_block(self, node);
        self.blocks.pop();
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.declare(&node.ident, DeclKind::Item);
        match node.content.is_some() {
            true => {
                self.chain.push(node.ident.to_string());
                syn::visit::visit_item_mod(self, node);
                self.chain.pop();
            }
            false => syn::visit::visit_item_mod(self, node),
        }
    }

    fn visit_impl_item(&mut self, node: &'ast syn::ImplItem) {
        match node {
            syn::ImplItem::Fn(item) => self.declare(&item.sig.ident, DeclKind::Method),
            syn::ImplItem::Const(item) => self.declare(&item.ident, DeclKind::Item),
            syn::ImplItem::Type(item) => self.declare(&item.ident, DeclKind::Item),
            _ => {}
        }
        syn::visit::visit_impl_item(self, node);
    }

    fn visit_trait_item(&mut self, node: &'ast syn::TraitItem) {
        match node {
            syn::TraitItem::Fn(item) => self.declare(&item.sig.ident, DeclKind::Method),
            syn::TraitItem::Const(item) => self.declare(&item.ident, DeclKind::Item),
            syn::TraitItem::Type(item) => self.declare(&item.ident, DeclKind::Item),
            _ => {}
        }
        syn::visit::visit_trait_item(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        // `use ::krate::..` names a crate by an absolute spelling this walk does
        // not carry; a relative reading of it would resolve to another module.
        if node.leading_colon.is_some() {
            return;
        }
        let exported = !matches!(node.vis, syn::Visibility::Inherited);
        let item = syn_span(self.line_starts, node.span());
        let block = self.blocks.last().copied();
        let mut branches = Vec::new();
        use_branches(&node.tree, &mut Vec::new(), &mut branches);
        for branch in branches {
            let named = match branch.kind {
                LeafKind::Glob => branch.idents.len(),
                _ => branch.idents.len().saturating_sub(1),
            };
            // Every name the clause binds is owner-law material: `{self}`
            // binds the segment before it, an alias its local name.
            let (bound, bound_prefix) = match branch.kind {
                LeafKind::Alias => (branch.binds.clone(), named),
                _ => match branch.idents.get(named).map(String::as_str) {
                    Some("self") if named > 0 => {
                        (Some(branch.idents[named - 1].clone()), named - 1)
                    }
                    Some(name) => (Some(name.to_string()), named),
                    None => (None, named),
                },
            };
            if let Some(name) = bound {
                self.out.owner_leaves.push(OwnerLeaf {
                    name,
                    prefix: branch.idents[..bound_prefix].to_vec(),
                    chain: self.chain.clone(),
                    block,
                });
            }
            for (index, ident) in branch.idents.iter().enumerate().take(named) {
                if ident != self.old {
                    continue;
                }
                let Some(span) = self.exact(branch.spans[index]) else {
                    continue;
                };
                self.out.paths.push(PathSeat {
                    chain: self.chain.clone(),
                    prefix: branch.idents[..index].to_vec(),
                    span,
                    role: RefRole::Import,
                    bare: false,
                });
            }
            let names_it = branch.idents.get(named).map(String::as_str) == Some(self.old);
            // `use P::OLD::{self}`: the leaf is the `self` token, the binding is
            // the segment before it.
            if branch.idents.get(named).map(String::as_str) == Some("self")
                && matches!(branch.kind, LeafKind::Name)
                && named > 0
                && branch.idents[named - 1] == self.old
            {
                let Some(span) = self.exact(branch.spans[named - 1]) else {
                    continue;
                };
                self.out.uses.push(UseLeaf {
                    chain: self.chain.clone(),
                    prefix: branch.idents[..named - 1].to_vec(),
                    span,
                    kind: LeafKind::SelfName,
                    item,
                    exported,
                    block,
                });
                continue;
            }
            if names_it {
                let Some(span) = self.exact(branch.spans[named]) else {
                    continue;
                };
                self.out.uses.push(UseLeaf {
                    chain: self.chain.clone(),
                    prefix: branch.idents[..named].to_vec(),
                    span,
                    kind: branch.kind,
                    item,
                    exported,
                    block,
                });
                continue;
            }
            let kind = match branch.binds.as_deref() == Some(self.old) {
                true => LeafKind::Shadow,
                false => match branch.kind {
                    LeafKind::Glob => LeafKind::Glob,
                    _ => continue,
                },
            };
            self.out.uses.push(UseLeaf {
                chain: self.chain.clone(),
                prefix: branch.idents.clone(),
                span: Span::empty(),
                kind,
                item,
                exported,
                block,
            });
        }
    }

    fn visit_path(&mut self, node: &'ast syn::Path) {
        if node.leading_colon.is_none() {
            let idents: Vec<String> = node
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            for (index, segment) in node.segments.iter().enumerate() {
                if segment.ident != self.old {
                    continue;
                }
                let Some(span) = self.exact(segment.ident.span()) else {
                    continue;
                };
                self.out.paths.push(PathSeat {
                    chain: self.chain.clone(),
                    prefix: idents[..index].to_vec(),
                    span,
                    role: self.role,
                    bare: idents.len() == 1,
                });
            }
        }
        syn::visit::visit_path(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        let held = std::mem::replace(&mut self.role, RefRole::Read);
        syn::visit::visit_expr_path(self, node);
        self.role = held;
    }

    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        let held = std::mem::replace(&mut self.role, RefRole::TypeRef);
        syn::visit::visit_type_path(self, node);
        self.role = held;
    }

    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        if node.value() == self.old {
            self.out.has_lit = true;
        }
        syn::visit::visit_lit_str(self, node);
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        self.local(&node.pat, self.blocks.last().copied());
        syn::visit::visit_local(self, node);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        let body = syn_span(self.line_starts, node.body.brace_token.span.join());
        self.local(&node.pat, Some(body));
        syn::visit::visit_expr_for_loop(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == self.old {
            if let Some(span) = self.exact(node.method.span()) {
                self.out.methods.push(span);
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    /// A macro body is tokens, not a scope the plane binds: the walk classifies
    /// what it spells instead of parsing it.
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.tokens(node.tokens.clone());
    }

    /// An attribute's arguments are tokens too, and its own path is a lint or
    /// derive name, never this symbol.
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        if let syn::Meta::List(list) = &node.meta {
            self.tokens(list.tokens.clone());
        }
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        let owner = node.ident.to_string();
        if let syn::Fields::Named(named) = &node.fields {
            self.declare_fields(named, &owner);
        }
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_union(&mut self, node: &'ast syn::ItemUnion) {
        let owner = node.ident.to_string();
        self.declare_fields(&node.fields, &owner);
        syn::visit::visit_item_union(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        let owner = node.ident.to_string();
        for variant in &node.variants {
            self.declare(&variant.ident, DeclKind::Variant { owner: owner.clone() });
            let variant_owner = variant.ident.to_string();
            if let syn::Fields::Named(named) = &variant.fields {
                self.declare_fields(named, &variant_owner);
            }
        }
    }
}

fn item_ident(item: &syn::Item) -> Option<&proc_macro2::Ident> {
    match item {
        syn::Item::Struct(item) => Some(&item.ident),
        syn::Item::Enum(item) => Some(&item.ident),
        syn::Item::Union(item) => Some(&item.ident),
        syn::Item::Trait(item) => Some(&item.ident),
        syn::Item::TraitAlias(item) => Some(&item.ident),
        syn::Item::Type(item) => Some(&item.ident),
        syn::Item::Fn(item) => Some(&item.sig.ident),
        syn::Item::Const(item) => Some(&item.ident),
        syn::Item::Static(item) => Some(&item.ident),
        _ => None,
    }
}

/// One flattened `use` branch: the segments as written, and what the last one
/// does. A `Glob` branch ends at the module it stars and binds no name.
struct UseBranch {
    idents: Vec<String>,
    spans: Vec<proc_macro2::Span>,
    kind: LeafKind,
    /// The name this branch binds in the writing scope.
    binds: Option<String>,
}

fn use_branches(
    tree: &syn::UseTree,
    prefix: &mut Vec<(String, proc_macro2::Span)>,
    out: &mut Vec<UseBranch>,
) {
    let branch = |prefix: &Vec<(String, proc_macro2::Span)>,
                  leaf: Option<(String, proc_macro2::Span)>,
                  kind: LeafKind,
                  binds: Option<String>| {
        let mut idents: Vec<String> = prefix.iter().map(|(ident, _)| ident.clone()).collect();
        let mut spans: Vec<proc_macro2::Span> = prefix.iter().map(|(_, span)| *span).collect();
        if let Some((ident, span)) = leaf {
            idents.push(ident);
            spans.push(span);
        }
        UseBranch {
            idents,
            spans,
            kind,
            binds,
        }
    };
    match tree {
        syn::UseTree::Path(segment) => {
            prefix.push((segment.ident.to_string(), segment.ident.span()));
            use_branches(&segment.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Group(group) => {
            for member in &group.items {
                use_branches(member, prefix, out);
            }
        }
        syn::UseTree::Name(leaf) => {
            let name = leaf.ident.to_string();
            out.push(branch(
                prefix,
                Some((name.clone(), leaf.ident.span())),
                LeafKind::Name,
                Some(name),
            ));
        }
        syn::UseTree::Rename(leaf) => out.push(branch(
            prefix,
            Some((leaf.ident.to_string(), leaf.ident.span())),
            LeafKind::Alias,
            Some(leaf.rename.to_string()),
        )),
        syn::UseTree::Glob(_) => out.push(branch(prefix, None, LeafKind::Glob, None)),
    }
}

// ── rustc's module-file law ─────────────────────────────────────────────────

/// A non-`#[path]` `mod x;`'s directory: the declaring file's own dir when it
/// owns its directory (crate root or `mod.rs`), else `<dir>/<stem>`.
fn module_dir(rel: &str, roots: &BTreeSet<String>) -> String {
    let stem = match rel.rsplit_once('/') {
        Some((_, stem)) => stem,
        None => rel,
    };
    let owned_directly = stem == "mod" || roots.contains(rel);
    match owned_directly {
        true => rel
            .rsplit_once('/')
            .map(|(dir, _)| dir.to_string())
            .unwrap_or_default(),
        false => rel.strip_suffix(".rs").unwrap_or(rel).to_string(),
    }
}

/// The `#[path = ".."]` literals on a `mod` decl, as spans against the file's
/// line table.
fn path_attrs(attrs: &[syn::Attribute], line_starts: &[u32]) -> Vec<(Span, String)> {
    let mut out = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("path") {
            continue;
        }
        if let syn::Meta::NameValue(meta) = &attr.meta {
            if let syn::Expr::Lit(lit) = &meta.value {
                if let syn::Lit::Str(text) = &lit.lit {
                    let span = syn_span(line_starts, text.span());
                    out.push((span, text.value()));
                }
            }
        }
    }
    out
}

/// The files a `#[path = ".."]` decl names, to every module each is: the literal
/// reads against the declaring file's dir; a file two decls name is two modules.
fn path_module_table(
    cx: &RenameCx,
    roots: &BTreeSet<String>,
) -> BTreeMap<String, Vec<ModuleId>> {
    let mut named: BTreeMap<String, Vec<ModuleId>> = BTreeMap::new();
    for rel in cx.files_of(&RustSource) {
        let Some(text) = cx.text(rel) else {
            continue;
        };
        // Exact for `#[path`; nothing else in a file names a placement.
        if !text.contains("path") {
            continue;
        }
        let Ok(parsed) = syn::parse_file(&text) else {
            continue;
        };
        let line_starts = build_line_starts(&text);
        let home = module_path(rel, roots);
        path_decls(&parsed.items, &[], rel, &home, roots, &line_starts, &mut named);
    }
    for routes in named.values_mut() {
        let mut seen = BTreeSet::new();
        routes.retain(|module| seen.insert(module.clone()));
    }
    named
}

/// One file's `#[path]` decls, through inline mods: each names a file under
/// the declaring module's directory and a module at that file's rustc module.
fn path_decls(
    items: &[syn::Item],
    chain: &[String],
    rel: &str,
    home: &ModuleId,
    roots: &BTreeSet<String>,
    line_starts: &[u32],
    out: &mut BTreeMap<String, Vec<ModuleId>>,
) {
    let mut dir = module_dir(rel, roots);
    for segment in chain {
        dir.push('/');
        dir.push_str(segment);
    }
    for item in items {
        let syn::Item::Mod(decl) = item else {
            continue;
        };
        match &decl.content {
            None => {
                let mut target = home.1.clone();
                target.extend(chain.iter().cloned());
                target.push(decl.ident.to_string());
                let root = owning_root(rel, roots).unwrap_or_else(|| rel.to_string());
                for (_, value) in path_attrs(&decl.attrs, line_starts) {
                    out.entry(join_rel(&dir, &value))
                        .or_default()
                        .push((root.clone(), target.clone()));
                }
            }
            Some((_, inner)) => {
                let mut inner_chain = chain.to_vec();
                inner_chain.push(decl.ident.to_string());
                path_decls(inner, &inner_chain, rel, home, roots, line_starts, out);
            }
        }
    }
}

/// A file's crate root and its module path from it, by layout alone. A `#[path]`
/// decl breaks that reading, which drops seats rather than inventing them.
fn module_path(rel: &str, roots: &BTreeSet<String>) -> ModuleId {
    let Some(root) = owning_root(rel, roots) else {
        return (rel.to_string(), Vec::new());
    };
    if rel == root {
        return (root, Vec::new());
    }
    let base = dirname(&root);
    let tail = match base.is_empty() {
        true => Some(rel),
        false => rel.strip_prefix(&format!("{base}/")),
    };
    let Some(tail) = tail else {
        return (root, Vec::new());
    };
    let mut parts: Vec<String> = tail.split('/').map(str::to_string).collect();
    let leaf = parts.pop().unwrap_or_default();
    let name = leaf.strip_suffix(".rs").unwrap_or(&leaf);
    if name != "mod" {
        parts.push(name.to_string());
    }
    (root, parts)
}

/// The crate root `rel` answers to: the one whose directory is its deepest
/// ancestor, and itself when it is a root.
fn owning_root(rel: &str, roots: &BTreeSet<String>) -> Option<String> {
    roots
        .iter()
        .filter(|root| rel == root.as_str() || under(rel, dirname(root)))
        .max_by_key(|root| (rel == root.as_str(), dirname(root).len()))
        .cloned()
}

fn under(path: &str, dir: &str) -> bool {
    dir.is_empty() || path.starts_with(&format!("{dir}/"))
}

/// Cargo's target auto-discovery, as path shapes: the two library/binary roots,
/// the `src/bin` binaries, the integration/bench/example roots, the build script.
fn auto_crate_root(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let (Some(last), Some(parent)) = (parts.last(), parts.iter().nth_back(1)) else {
        return parts.last() == Some(&"build.rs");
    };
    match *parent {
        "src" => matches!(*last, "lib.rs" | "main.rs"),
        "bin" | "tests" | "benches" | "examples" => last.ends_with(".rs"),
        _ => *last == "build.rs",
    }
}

fn crate_roots(cx: &RenameCx) -> BTreeSet<String> {
    let mut roots: BTreeSet<String> = cx
        .files()
        .iter()
        .filter(|rel| auto_crate_root(rel))
        .cloned()
        .collect();
    for (manifest, package) in manifests(cx) {
        let dir = dirname(&manifest);
        if let Some(path) = package.lib.as_ref().and_then(|lib| lib.path.clone()) {
            roots.insert(join_rel(dir, &path));
        }
    }
    roots
}

/// A crate's identifier as a `use` writes it -> that crate's library root, so a
/// path through the package name reaches the same modules `crate::` does.
fn crate_idents(cx: &RenameCx) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (manifest, package) in manifests(cx) {
        let dir = dirname(&manifest);
        let named = package
            .lib
            .as_ref()
            .and_then(|lib| lib.name.clone())
            .or_else(|| package.package.as_ref().map(|meta| meta.name.clone()));
        let Some(named) = named else {
            continue;
        };
        let root = match package.lib.as_ref().and_then(|lib| lib.path.clone()) {
            Some(path) => join_rel(dir, &path),
            None => join_rel(dir, "src/lib.rs"),
        };
        out.insert(named.replace('-', "_"), root);
    }
    out
}

fn manifests(cx: &RenameCx) -> Vec<(String, Manifest)> {
    cx.files()
        .iter()
        .filter(|rel| stem(rel) == "Cargo" && rel.ends_with(".toml"))
        .filter_map(|rel| {
            let text = cx.text(rel)?;
            let parsed: Manifest = basic_toml::from_str(&text).ok()?;
            Some((rel.clone(), parsed))
        })
        .collect()
}

/// The two manifest keys the module law reads: the crate's own name, and a
/// `[lib]` that renames or relocates its root.
#[derive(Deserialize)]
struct Manifest {
    package: Option<ManifestPackage>,
    lib: Option<ManifestLib>,
}

#[derive(Deserialize)]
struct ManifestPackage {
    name: String,
}

#[derive(Deserialize)]
struct ManifestLib {
    name: Option<String>,
    path: Option<String>,
}
