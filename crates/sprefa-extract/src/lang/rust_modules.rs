//! @comment-ok: module header, the seam list every lang file opens with
//! The Rust module plane: the language's OWN name resolution (`use`, `pub
//! use`, `mod`), run once per file set, so `Resolve<CallF>`/`Resolve<TypeF>`
//! bind an imported name through it instead of a corpus-wide name guess.
//! Mirrors `ts_resolve.rs`'s ECMAScript ResolveExport plane.
//!
//! The phase-1 Rust output carries these module facts into project resolve.
//! Its flat `Specifier` rows (`rust.rs:1642`) collapse
//! `use a::b::{self}` and `use a::b::*` onto one (name, module,
//! kind=Reexport) shape, and an inline `mod x { .. }` mints NO row at all
//! (`rust.rs:1690`, documented `NO ROW`). `hafley_scm` extracts the syntax
//! rows from the phase-1 parse; this file resolves those rows across files.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use crate::seams::DefIndex;
use crate::shape::{ContentId, FamilyTag, Span, ZERO_CONTENT_ID};

use super::rust::{crate_root_of, module_segments, module_target};
use super::rust_receivers::ImplEntry;

// ── module facts from phase-1 syntax rows ────────────────────────────────────

/// What one `use` leaf binds a local name to. `qualifier` is the source
/// module's path as written (`crate`/`self`/`super` kept literal).
#[derive(Clone, Debug, PartialEq, Eq)]
struct UseBinding {
    local: String,
    qualifier: Vec<String>,
    asked: String,
    reexport: bool,
}

/// A bare `use a::b::*;` / `pub use a::b::*;`: no local name, only a star hop
/// candidate for names the qualifier module's own resolve asks.
#[derive(Clone, Debug, PartialEq, Eq)]
struct StarImport {
    qualifier: Vec<String>,
    reexport: bool,
}

/// One file's `use`/`mod` facts, carried from phase 1 into project resolve.
#[derive(Clone, Debug, Default)]
pub struct RustModuleFacts {
    uses: Vec<UseBinding>,
    stars: Vec<StarImport>,
    /// `mod x { .. }`: no file of its own, so `use x::f` binds in this blob.
    inline_mods: BTreeSet<String>,
    /// `mod x;` / `#[path = "y.rs"] mod x;`: name plus the path literal.
    mod_decls: Vec<(String, Option<String>)>,
    /// Every impl block's (self type, fn name, fn def span), for the corpus
    /// receiver leg's (T, m) table.
    impls: Vec<ImplEntry>,
    /// Every enum's (name, variant name + def span): a `T::f()` whose `f` is
    /// a variant of corpus enum `T` names the VARIANT, not the enum.
    enums: Vec<(String, Vec<(String, Span)>)>,
    /// Every trait's (name, fn name, fn def span, default body?) for the
    /// trait dispatch table: declared (no body) and default (body present)
    /// fns alike bind to the trait's own def.
    traits: Vec<TraitEntry>,
    /// Every `type X = ..` def span. An alias rides the shared `DefIndex` as a
    /// type entity and is never the item a `X(..)` call constructs.
    aliases: Vec<Span>,
    pub(crate) macro_invocations: Vec<(Span, String)>,
}

/// One trait declaration's fn set.
#[derive(Clone, Debug)]
pub(crate) struct TraitEntry {
    pub(crate) name: String,
    pub(crate) fns: Vec<TraitFn>,
}

/// One fn of a trait: `default` marks a fn with a body.
#[derive(Clone, Debug)]
pub(crate) struct TraitFn {
    pub(crate) name: String,
    pub(crate) span: Span,
    pub(crate) default: bool,
}

/// Standalone fallback for callers without a phase-1 Rust output.
pub fn rust_module_facts(path: &str, content: &[u8]) -> Option<RustModuleFacts> {
    if !path.ends_with(".rs") {
        return None;
    }
    let text = std::str::from_utf8(content).ok()?;
    let parsed = hafley_scm::lang::rust::parse_rust_syntax(text).ok()?;
    Some(rust_module_facts_from_parsed(&parsed.file, &parsed.line_starts))
}

/// The module facts off the extract pass's own syn parse, so no second parse.
pub(crate) fn rust_module_facts_from_parsed(parsed: &syn::File, line_starts: &[u32]) -> RustModuleFacts {
    let rows = hafley_scm::lang::rust::module_resolution_rows(parsed, line_starts);
    RustModuleFacts {
        uses: rows.uses.into_iter().map(|row| UseBinding {
            local: row.local,
            qualifier: row.qualifier,
            asked: row.asked,
            reexport: row.reexport,
        }).collect(),
        stars: rows.stars.into_iter().map(|row| StarImport {
            qualifier: row.qualifier,
            reexport: row.reexport,
        }).collect(),
        inline_mods: rows.inline_mods.into_iter().collect(),
        mod_decls: rows.mod_decls,
        impls: rows.impls.into_iter().map(|row| ImplEntry {
            self_type: row.self_type,
            trait_name: row.trait_name,
            methods: row.methods.into_iter().map(|(name, range)| (
                name,
                Span { start: range.start, len: range.end - range.start },
            )).collect(),
        }).collect(),
        enums: rows.enums.into_iter().map(|row| (
            row.name,
            row.variants.into_iter().map(|(name, range)| (
                name,
                Span { start: range.start, len: range.end - range.start },
            )).collect(),
        )).collect(),
        traits: rows.traits.into_iter().map(|row| TraitEntry {
            name: row.name,
            fns: row.methods.into_iter().map(|method| TraitFn {
                name: method.name,
                span: Span { start: method.range.start, len: method.range.end - method.range.start },
                default: method.default,
            }).collect(),
        }).collect(),
        aliases: rows.aliases.into_iter().map(|range| Span {
            start: range.start,
            len: range.end - range.start,
        }).collect(),
        macro_invocations: hafley_scm::lang::rust::macro_invocation_rows_from_parsed(parsed, line_starts)
            .into_iter()
            .map(|row| (Span { start: row.range.start, len: row.range.end - row.range.start }, row.name))
            .collect(),
    }
}

/// The two Cargo.toml keys the module law reads: the crate's own name, and a
/// `[lib]` that renames or relocates its root.
#[derive(serde::Deserialize)]
pub(crate) struct CargoManifest {
    package: Option<CargoPackage>,
    lib: Option<CargoLib>,
}

#[derive(serde::Deserialize)]
struct CargoPackage {
    name: String,
}

#[derive(serde::Deserialize)]
struct CargoLib {
    name: Option<String>,
    path: Option<String>,
}

impl CargoManifest {
    pub(crate) fn parse(text: &str) -> Option<CargoManifest> {
        basic_toml::from_str(text).ok()
    }

    /// `[lib] name`, else the package name, `-` as `_`.
    pub(crate) fn ident(&self) -> Option<String> {
        self.lib
            .as_ref()
            .and_then(|lib| lib.name.clone())
            .or_else(|| self.package.as_ref().map(|package| package.name.clone()))
            .map(|name| name.replace('-', "_"))
    }

    /// A `[lib] path` override, relative to the manifest's directory.
    pub(crate) fn explicit_lib_path(&self) -> Option<String> {
        self.lib.as_ref().and_then(|lib| lib.path.clone())
    }

    /// The library root relative to the manifest's directory.
    pub(crate) fn lib_path(&self) -> String {
        self.explicit_lib_path()
            .unwrap_or_else(|| "src/lib.rs".to_string())
    }
}

/// Crate identifier -> corpus library root, from the Cargo.toml (read off
/// disk) of each crate directory the corpus's `.rs` files sit in.
fn crate_libs(corpus: &[(String, ContentId)]) -> HashMap<String, String> {
    let roots: BTreeSet<String> = corpus
        .iter()
        .filter(|(path, _)| path.ends_with(".rs"))
        .filter_map(|(path, _)| {
            crate_root_of(path).or_else(|| path.starts_with("src/").then(String::new))
        })
        .collect();
    let in_corpus: std::collections::HashSet<&str> =
        corpus.iter().map(|(path, _)| path.as_str()).collect();
    let mut out = HashMap::new();
    for root in roots {
        let Some(parsed) = std::fs::read_to_string(normalize_join(&root, "Cargo.toml"))
            .ok()
            .and_then(|text| CargoManifest::parse(&text))
        else {
            continue;
        };
        let lib = normalize_join(&root, &parsed.lib_path());
        if let (Some(ident), true) = (parsed.ident(), in_corpus.contains(lib.as_str())) {
            out.insert(ident, lib);
        }
    }
    out
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Where `mod x;` in `path` looks for `x.rs`: a mod-rs file or a crate root
/// under `bin`/`examples`/`tests`/`benches` owns its dir, `foo.rs` owns `foo/`.
fn mod_dir(path: &str) -> String {
    let dir = parent_dir(path);
    let file = path.rsplit_once('/').map_or(path, |(_, file)| file);
    let stem = file.strip_suffix(".rs").unwrap_or(file);
    let dir_name = dir.rsplit_once('/').map_or(dir, |(_, name)| name);
    let mod_rs = matches!(stem, "mod" | "lib" | "main" | "build")
        || matches!(dir_name, "bin" | "examples" | "tests" | "benches");
    match (mod_rs, dir.is_empty()) {
        (true, _) => dir.to_string(),
        (false, true) => stem.to_string(),
        (false, false) => format!("{dir}/{stem}"),
    }
}

/// `dir` joined with a `#[path = "lit"]` literal, `.`/`..` collapsed.
fn normalize_join(dir: &str, literal: &str) -> String {
    let combined = if dir.is_empty() {
        literal.to_string()
    } else {
        format!("{dir}/{literal}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in combined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

// ── the module plane proper ──────────────────────────────────────────────────

/// std::prelude::v1's trait set: in scope in every module without an import.
const PRELUDE_TRAITS: &[&str] = &[
    "AsMut",
    "AsRef",
    "Clone",
    "Copy",
    "Default",
    "DoubleEndedIterator",
    "Drop",
    "Eq",
    "ExactSizeIterator",
    "Extend",
    "From",
    "Fn",
    "FnMut",
    "FnOnce",
    "Into",
    "IntoIterator",
    "Iterator",
    "Ord",
    "PartialEq",
    "PartialOrd",
    "Send",
    "Sized",
    "Sync",
    "ToOwned",
    "ToString",
    "Unpin",
];

/// How an import binding reached its target; `Module` is a `mod x;` file
/// edge. Rust has no `default` export form.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ResolvedImportKind {
    Local,
    Indirect,
    Star,
    Namespace,
    Module,
}

impl ResolvedImportKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            ResolvedImportKind::Local => "local",
            ResolvedImportKind::Indirect => "indirect",
            ResolvedImportKind::Star => "star",
            ResolvedImportKind::Namespace => "namespace",
            ResolvedImportKind::Module => "module",
        }
    }

    /// One more hop's arm folded in: star outranks indirect outranks local.
    fn promote(self, other: ResolvedImportKind) -> ResolvedImportKind {
        use ResolvedImportKind::*;
        match (self, other) {
            (Star, _) | (_, Star) => Star,
            (Indirect, _) | (_, Indirect) => Indirect,
            _ => Local,
        }
    }
}

/// One `use` binding resolved to a corpus declaration (or a whole module for
/// a namespace binding). What `Resolve<CallF>`/`Resolve<TypeF>` bind through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedImport {
    pub local: String,
    /// The name asked of the source module; `"*"` for a namespace binding.
    pub name: String,
    pub target_path: String,
    pub target_blob: ContentId,
    /// `Span::anchor(0)` for a namespace binding: no one def span applies.
    pub target_span: Span,
    pub target_name: Option<String>,
    pub kind: ResolvedImportKind,
    pub hops: u32,
}

/// The `resolved_import` wire row: `ResolvedImport` with blob/span dropped.
pub struct ImportRow {
    pub local: String,
    pub name: String,
    pub target_path: String,
    pub target_name: Option<String>,
    pub kind: ResolvedImportKind,
    pub hops: u32,
}

/// One qualifier's home file: the corpus file whose module path IS it.
/// `Ambiguous` when 2+ files tie (the kink-4 discipline), `External` when the
/// qualifier names a module no corpus file spells (an external crate).
enum HomeFile {
    Unique(String),
    None,
    Ambiguous,
    External,
}

/// One name's resolution inside a module: a declaration, a whole submodule
/// (`use a::b;` where `b` is a module, not an item), a star tie, or nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Resolution {
    Binding {
        blob: ContentId,
        span: Span,
        name: String,
        kind: ResolvedImportKind,
        hops: u32,
    },
    Module {
        file: String,
        hops: u32,
    },
    Ambiguous,
    None,
}

impl Resolution {
    fn promoted(self, arm: ResolvedImportKind) -> Resolution {
        match self {
            Resolution::Binding {
                blob,
                span,
                name,
                kind,
                hops,
            } => Resolution::Binding {
                blob,
                span,
                name,
                kind: kind.promote(arm),
                hops: hops + 1,
            },
            Resolution::Module { file, hops } => Resolution::Module {
                file,
                hops: hops + 1,
            },
            other => other,
        }
    }

    /// `promoted`, but a hop that reached nothing carries nothing forward
    /// (an absent name from one star must not shadow a later star's hit).
    fn promoted_option(self, arm: ResolvedImportKind) -> Option<Resolution> {
        (!matches!(self, Resolution::None)).then(|| self.promoted(arm))
    }
}

/// Whether two star arms offer the SAME target: (blob, span) for a binding,
/// the file path for a module.
fn same_target(a: &Resolution, b: &Resolution) -> bool {
    match (a, b) {
        (
            Resolution::Binding {
                blob: b1, span: s1, ..
            },
            Resolution::Binding {
                blob: b2, span: s2, ..
            },
        ) => b1 == b2 && s1 == s2,
        (Resolution::Module { file: f1, .. }, Resolution::Module { file: f2, .. }) => f1 == f2,
        _ => false,
    }
}

/// The outcome of a module-qualified call's prefix resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModuleCallTarget {
    Target(ContentId, Span),
    /// The prefix names a module no corpus file spells: an external crate.
    External,
    Miss,
}

/// THE corpus Rust module plane, built ONCE per refresh in `resolve_project`.
#[derive(Default)]
pub struct RustModuleIndex {
    facts: HashMap<String, RustModuleFacts>,
    blobs: HashMap<String, ContentId>,
    paths: HashMap<ContentId, String>,
    /// Corpus-relative module path per file, `#[path]` overrides applied.
    module_paths: HashMap<String, Vec<String>>,
    /// Crate identifier -> library root file, for `own_crate::x` paths.
    crate_libs: HashMap<String, String>,
    /// Module path's LAST segment -> candidate files, the fan-out filter
    /// before the full suffix check (`ModuleTarget::covers`).
    by_last_segment: HashMap<String, Vec<String>>,
    /// Every segment of every module path: `module_call`'s external test.
    segment_names: HashSet<String>,
    /// blob -> (span, name, family) of every def in it.
    defs: HashMap<ContentId, Vec<(Span, String, FamilyTag)>>,
    /// (blob, span) pairs several def NAMES share: one macro expansion's
    /// items all report the macro call's own span, so the span names nothing.
    collapsed: BTreeSet<(ContentId, Span)>,
    /// (blob, span) of every `type X = ..` declaration.
    aliases: BTreeSet<(ContentId, Span)>,
    /// file -> its WHOLE export table, built once and reused by every
    /// importer (a per-name walk over a many-star barrel is quadratic).
    tables: Mutex<HashMap<String, std::sync::Arc<ExportTable>>>,
    /// file -> its WHOLE local scope (the export table plus non-reexport
    /// globs), for a bare name with no explicit `use` in the SAME file.
    scope_tables: Mutex<HashMap<String, std::sync::Arc<ExportTable>>>,
    /// (self type, fn name) -> every corpus impl site of the pair, with the
    /// impl's trait name where the block is `impl Trait for T`.
    impl_methods: HashMap<(String, String), Vec<ImplMethodTarget>>,
    /// (enum name, variant name) -> declaring file path plus the variant's
    /// own def span.
    enum_variants: HashMap<(String, String), Vec<(String, Span)>>,
    /// trait name -> every fn def it declares or defaults, one site per
    /// (file, fn): the same trait NAME can be declared by several files, so
    /// a target pick needs the site's own blob.
    trait_fns: HashMap<String, Vec<TraitFnSite>>,
    /// (trait name, fn name) -> every corpus `impl Trait for T` fn of the pair.
    trait_impl_fns: HashMap<(String, String), Vec<(ContentId, Span)>>,
    /// self type -> trait names an `impl Trait for T` block names.
    type_traits: HashMap<String, Vec<String>>,
    /// Every self type some corpus impl block names, inherent or trait: the
    /// types the receiver plane can answer for.
    impl_types: std::collections::HashSet<String>,
}

/// One (file, fn) site a trait declares or defaults. The blob rides along so
/// a bare trait name shared by several files still resolves per file.
#[derive(Clone, Debug)]
pub(crate) struct TraitFnSite {
    pub(crate) blob: ContentId,
    pub(crate) fn_name: String,
    pub(crate) span: Span,
    pub(crate) default: bool,
}

/// One corpus impl site of a (self type, fn name) pair. `trait_name` is None
/// for an inherent `impl T`.
#[derive(Clone, Debug)]
pub(crate) struct ImplMethodTarget {
    pub(crate) blob: ContentId,
    pub(crate) span: Span,
    pub(crate) trait_name: Option<String>,
}

type ExportTable = HashMap<String, Resolution>;

impl RustModuleIndex {
    /// `files` is every `.rs` input's facts; `corpus` is EVERY input's (path,
    /// blob), any language, so target lookups stay lang-agnostic.
    pub fn build(
        files: Vec<(String, RustModuleFacts)>,
        corpus: &[(String, ContentId)],
        def_index: &DefIndex,
    ) -> RustModuleIndex {
        let mut index = RustModuleIndex {
            crate_libs: crate_libs(corpus),
            ..RustModuleIndex::default()
        };
        for (path, blob) in corpus {
            index.blobs.insert(path.clone(), blob.clone());
            index
                .paths
                .entry(blob.clone())
                .or_insert_with(|| path.clone());
            if path.ends_with(".rs") {
                index
                    .module_paths
                    .insert(path.clone(), module_segments(path));
            }
        }
        for (path, facts) in &files {
            let dir = parent_dir(path);
            for (name, path_attr) in &facts.mod_decls {
                let Some(literal) = path_attr else { continue };
                let target = normalize_join(dir, literal);
                if index.blobs.contains_key(&target) {
                    let mut segments = module_segments(path);
                    segments.push(name.clone());
                    index.module_paths.insert(target, segments);
                }
            }
        }
        for (path, segments) in &index.module_paths {
            index.segment_names.extend(segments.iter().cloned());
            if let Some(last) = segments.last() {
                index
                    .by_last_segment
                    .entry(last.clone())
                    .or_default()
                    .push(path.clone());
            }
        }
        for (name, sites) in &def_index.map {
            for site in sites {
                index.defs.entry(site.blob.clone()).or_default().push((
                    site.span,
                    name.clone(),
                    site.family,
                ));
            }
        }
        let mut collapsed: BTreeSet<(ContentId, Span)> = BTreeSet::new();
        for (blob, defs) in &index.defs {
            let mut first: HashMap<Span, &str> = HashMap::new();
            for (span, name, _) in defs {
                match first.get(span) {
                    Some(seen) if *seen != name.as_str() => {
                        collapsed.insert((blob.clone(), *span));
                    }
                    Some(_) => {}
                    None => {
                        first.insert(*span, name.as_str());
                    }
                }
            }
        }
        index.collapsed = collapsed;
        for (path, facts) in &files {
            if let Some(blob) = index.blobs.get(path) {
                for span in &facts.aliases {
                    index.aliases.insert((blob.clone(), *span));
                }
            }
        }
        for (path, facts) in &files {
            let Some(blob) = index.blobs.get(path) else {
                continue;
            };
            for entry in &facts.impls {
                index.impl_types.insert(entry.self_type.clone());
                for (name, span) in &entry.methods {
                    index
                        .impl_methods
                        .entry((entry.self_type.clone(), name.clone()))
                        .or_default()
                        .push(ImplMethodTarget {
                            blob: blob.clone(),
                            span: *span,
                            trait_name: entry.trait_name.clone(),
                        });
                }
            }
            for (name, variants) in &facts.enums {
                for (variant, span) in variants {
                    index
                        .enum_variants
                        .entry((name.clone(), variant.clone()))
                        .or_default()
                        .push((path.clone(), *span));
                }
            }
            if let Some(blob) = index.blobs.get(path) {
                for entry in &facts.traits {
                    for f in &entry.fns {
                        index
                            .trait_fns
                            .entry(entry.name.clone())
                            .or_default()
                            .push(TraitFnSite {
                                blob: blob.clone(),
                                fn_name: f.name.clone(),
                                span: f.span,
                                default: f.default,
                            });
                    }
                }
            }
            for entry in &facts.impls {
                let Some(trait_name) = &entry.trait_name else {
                    continue;
                };
                index
                    .type_traits
                    .entry(entry.self_type.clone())
                    .or_default()
                    .push(trait_name.clone());
                if let Some(blob) = index.blobs.get(path) {
                    for (name, span) in &entry.methods {
                        index
                            .trait_impl_fns
                            .entry((trait_name.clone(), name.clone()))
                            .or_default()
                            .push((blob.clone(), *span));
                    }
                }
            }
        }
        index.facts = files.into_iter().collect();
        index
    }

    /// The ONE def site an impl block names for (self type, fn); 2+ settle by
    /// inherent-beats-trait with the trait-in-scope filter.
    pub(crate) fn impl_target(
        &self,
        self_type: &str,
        method: &str,
        caller: Option<&str>,
    ) -> Option<(ContentId, Span)> {
        let sites = self
            .impl_methods
            .get(&(self_type.to_string(), method.to_string()))?
            .as_slice();
        let pick = |site: &ImplMethodTarget| Some((site.blob.clone(), site.span));
        match sites {
            [only] => pick(only),
            many => {
                let inherent: Vec<&ImplMethodTarget> = many
                    .iter()
                    .filter(|site| site.trait_name.is_none())
                    .collect();
                match inherent.as_slice() {
                    [one] => pick(one),
                    [] => {
                        let caller = caller?;
                        let survivors: Vec<&ImplMethodTarget> = many
                            .iter()
                            .filter(|site| {
                                site.trait_name.as_deref().is_some_and(|trait_name| {
                                    self.trait_in_scope(caller, trait_name)
                                })
                            })
                            .collect();
                        match survivors.as_slice() {
                            [one] => pick(one),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
        }
    }

    /// A trait's presence in the caller's scope: prelude, explicit `use`
    /// (even an external trait), or the scope table's globs and locals.
    fn trait_in_scope(&self, caller: &str, trait_name: &str) -> bool {
        if PRELUDE_TRAITS.contains(&trait_name) {
            return true;
        }
        let Some(facts) = self.facts.get(caller) else {
            return false;
        };
        if facts.uses.iter().any(|binding| binding.local == trait_name) {
            return true;
        }
        let mut stack = Vec::new();
        !matches!(
            self.wildcard_scope(caller, trait_name, &mut stack),
            Resolution::None
        )
    }

    /// The enum def a `T::f` constructor names when `f` is a variant of
    /// exactly one corpus enum `T`: the call binds the enum, not a method.
    pub(crate) fn variant_ctor_target(
        &self,
        type_name: &str,
        variant: &str,
    ) -> Option<(ContentId, Span)> {
        let [(only, span)] = self
            .enum_variants
            .get(&(type_name.to_string(), variant.to_string()))?
            .as_slice()
        else {
            return None;
        };
        let blob = self.blobs.get(only)?;
        Some((blob.clone(), *span))
    }

    /// Whether `name` is a trait the corpus declares.
    pub(crate) fn is_trait(&self, name: &str) -> bool {
        self.trait_fns.contains_key(name)
    }

    /// Whether the corpus declares an impl block for `name`, or `name` is a
    /// declared trait: a `Named` receiver outside this set is std/external.
    pub(crate) fn is_impl_known(&self, name: &str) -> bool {
        self.impl_types.contains(name) || self.trait_fns.contains_key(name)
    }

    /// Which of `candidates` (distinct blobs declaring one trait name) the
    /// caller binds: its own file when it declares the trait, else the file
    /// its `use` of the name resolves to, else None (an `unresolved{reason}`
    /// row, never a guess across files).
    fn bound_trait_blob(
        &self,
        caller: Option<&str>,
        trait_name: &str,
        candidates: &[ContentId],
    ) -> Option<ContentId> {
        if candidates.len() == 1 {
            return Some(candidates[0].clone());
        }
        let caller = caller?;
        if let Some(own) = self.blobs.get(caller) {
            if candidates.iter().any(|c| c == own) {
                return Some(own.clone());
            }
        }
        let facts = self.facts.get(caller)?;
        if !facts.uses.iter().any(|binding| binding.local == trait_name) {
            return None;
        }
        let Ok(Some(found)) = self.explicit_binding(caller, trait_name) else {
            return None;
        };
        candidates
            .iter()
            .find(|c| **c == found.target_blob)
            .cloned()
    }

    /// The trait's own fn def for `fn_name`, declared or defaulted (classes
    /// 12 and 6): a `T::f()` / `x.m()` whose T is a corpus trait binds here.
    pub(crate) fn trait_fn_target(
        &self,
        trait_name: &str,
        fn_name: &str,
        caller: Option<&str>,
    ) -> Option<(ContentId, Span)> {
        let matched: Vec<&TraitFnSite> = self
            .trait_fns
            .get(trait_name)?
            .iter()
            .filter(|site| site.fn_name == fn_name)
            .collect();
        let blobs: Vec<ContentId> = {
            let mut blobs: Vec<ContentId> = matched.iter().map(|site| site.blob.clone()).collect();
            blobs.dedup();
            blobs
        };
        let blob = self.bound_trait_blob(caller, trait_name, &blobs)?;
        let site = matched.iter().find(|site| site.blob == blob)?;
        Some((site.blob.clone(), site.span))
    }

    /// The ONE corpus impl of `trait_name` defining `fn_name` (class 12's
    /// impl-first rule); when several files impl the trait, the caller's own
    /// file or its `use`-bound file wins, 2+ left stay unbound.
    pub(crate) fn trait_impl_target(
        &self,
        trait_name: &str,
        fn_name: &str,
        caller: Option<&str>,
    ) -> Option<(ContentId, Span)> {
        let sites = self
            .trait_impl_fns
            .get(&(trait_name.to_string(), fn_name.to_string()))?;
        let mut blobs: Vec<ContentId> = sites.iter().map(|(blob, _)| blob.clone()).collect();
        blobs.dedup();
        let blob = self.bound_trait_blob(caller, trait_name, &blobs)?;
        let mut hits = sites.iter().filter(|(b, _)| *b == blob);
        let (blob, span) = hits.next()?;
        hits.next()
            .map(|_| ())
            .map_or_else(|| Some((blob.clone(), *span)), |_| None)
    }

    /// The one trait default body providing `fn_name` for a type `type_name`
    /// implements (classes 4 and 8): no impl defines the fn, the trait does.
    /// A trait name declared by several files binds per the caller's own
    /// file or `use`-bound file, else unbound.
    pub(crate) fn trait_default_target(
        &self,
        type_name: &str,
        fn_name: &str,
        caller: Option<&str>,
    ) -> Option<(ContentId, Span)> {
        let traits = self.type_traits.get(type_name)?;
        let mut hit: Option<(ContentId, Span)> = None;
        for trait_name in traits {
            let Some(sites) = self.trait_fns.get(trait_name) else {
                continue;
            };
            let sites = sites
                .iter()
                .filter(|site| site.fn_name == fn_name && site.default)
                .cloned()
                .collect::<Vec<TraitFnSite>>();
            let blobs: Vec<ContentId> = {
                let mut blobs: Vec<ContentId> =
                    sites.iter().map(|site| site.blob.clone()).collect();
                blobs.dedup();
                blobs
            };
            let blob = self.bound_trait_blob(caller, trait_name, &blobs);
            let Some(blob) = blob else { continue };
            for site in sites.iter().filter(|site| site.blob == blob) {
                let target = (site.blob.clone(), site.span);
                if hit.is_some() && hit.as_ref() != Some(&target) {
                    return None;
                }
                hit = Some(target);
            }
        }
        hit
    }

    /// The corpus file `mod name;` in `path` loads: `#[path]` against the
    /// file's dir, else `name.rs` then `name/mod.rs` under `mod_dir`.
    fn mod_file(&self, path: &str, name: &str, path_attr: Option<&str>) -> Option<String> {
        if let Some(literal) = path_attr {
            let target = normalize_join(parent_dir(path), literal);
            return self.blobs.contains_key(&target).then_some(target);
        }
        let dir = mod_dir(path);
        [format!("{name}.rs"), format!("{name}/mod.rs")]
            .into_iter()
            .map(|file| normalize_join(&dir, &file))
            .find(|target| self.blobs.contains_key(target))
    }

    /// One `module` row per corpus `mod x;` and per other corpus crate a path
    /// names, then every resolved `use` binding; an ambiguous or
    /// corpus-external binding has no row.
    pub fn bindings(&self, path: &str) -> Vec<ImportRow> {
        let Some(facts) = self.facts.get(path) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        let module_row = |name: &str, target: String| ImportRow {
            local: String::new(),
            name: name.to_string(),
            target_path: target,
            target_name: None,
            kind: ResolvedImportKind::Module,
            hops: 1,
        };
        for (name, path_attr) in &facts.mod_decls {
            if let Some(target) = self.mod_file(path, name, path_attr.as_deref()) {
                rows.push(module_row(name, target));
            }
        }
        let heads: BTreeSet<&str> = facts
            .uses
            .iter()
            .map(|binding| binding.qualifier.first().unwrap_or(&binding.asked).as_str())
            .chain(facts.stars.iter().filter_map(|star| star.qualifier.first().map(String::as_str)))
            .collect();
        for head in heads {
            match self.crate_libs.get(head) {
                Some(lib) if lib != path => rows.push(module_row(head, lib.clone())),
                _ => {}
            }
        }
        for binding in &facts.uses {
            if let Ok(Some(found)) = self.explicit_binding(path, &binding.local) {
                rows.push(ImportRow {
                    local: found.local,
                    name: found.name,
                    target_path: found.target_path,
                    target_name: found.target_name,
                    kind: found.kind,
                    hops: found.hops,
                });
            }
        }
        rows
    }

    /// A bare name: an explicit `use` binding, else any glob's wildcard scope.
    pub fn target(&self, path: &str, local: &str) -> Option<(ContentId, Span)> {
        match self.explicit_binding(path, local) {
            Ok(Some(found)) => return callable_target(found),
            Err(()) => return None,
            Ok(None) => {}
        }
        let mut stack = Vec::new();
        let resolution = self.wildcard_scope(path, local, &mut stack);
        self.finish(local, local, resolution)
            .ok()
            .flatten()
            .and_then(callable_target)
    }

    /// Whether a def coordinate is a COLLAPSED span: one macro expansion's
    /// items all report the macro call's span, so it names nothing.
    pub fn is_collapsed(&self, blob: &ContentId, span: Span) -> bool {
        self.collapsed.contains(&(blob.clone(), span))
    }

    /// The call plane's type-facet fallback exists for tuple-struct and variant
    /// constructors, whose def IS the constructed item; an alias's is not.
    pub fn is_alias(&self, blob: &ContentId, span: Span) -> bool {
        self.aliases.contains(&(blob.clone(), span))
    }

    /// Whether any corpus file's module path ends in `segment`. A qualifier no
    /// corpus module answers (`std::result`) names an EXTERNAL declaration.
    pub fn names_a_module(&self, segment: &str) -> bool {
        self.by_last_segment.contains_key(segment)
    }

    /// `target` re-aimed at the TYPE facet: the export table prefers the call
    /// facet, and a type reference to it emits a nameless row.
    pub fn type_target(&self, path: &str, local: &str) -> Option<(ContentId, Span)> {
        let (blob, span) = self.target(path, local)?;
        let defs = self.defs.get(&blob)?;
        let bound_name = defs
            .iter()
            .find(|(def_span, _, _)| *def_span == span)
            .map(|(_, name, _)| name.as_str())?;
        let declared = defs
            .iter()
            .find(|(_, name, family)| name == bound_name && *family == FamilyTag::Type);
        Some(declared.map_or((blob.clone(), span), |(span, _, _)| (blob, *span)))
    }

    /// `local`'s EXPLICIT `use` binding in `path`. `Err(())` is AMBIGUOUS: a
    /// glob-hop conflict the drops channel carries.
    fn explicit_binding(&self, path: &str, local: &str) -> Result<Option<ResolvedImport>, ()> {
        let Some(facts) = self.facts.get(path) else {
            return Ok(None);
        };
        let Some(binding) = facts
            .uses
            .iter()
            .find(|use_binding| use_binding.local == local)
        else {
            return Ok(None);
        };
        let mut stack = Vec::new();
        let resolution = self
            .resolve_qualified(
                path,
                &binding.qualifier,
                &binding.asked,
                &mut stack,
                &mut Vec::new(),
            )
            .0;
        self.finish(local, &binding.asked, resolution)
    }

    /// `name` as ANY glob in `path` brings it into scope, reexported or not
    /// (unlike `export_table`'s star leg, which only follows a REEXPORT glob).
    fn wildcard_scope(&self, path: &str, name: &str, stack: &mut Vec<String>) -> Resolution {
        self.local_scope_table(path, stack)
            .0
            .get(name)
            .cloned()
            .unwrap_or(Resolution::None)
    }

    /// `path`'s own export table, plus every name a non-reexport glob adds
    /// that the export table does not already carry. Built ONCE per file.
    fn local_scope_table(
        &self,
        path: &str,
        stack: &mut Vec<String>,
    ) -> (std::sync::Arc<ExportTable>, bool) {
        if let Some(hit) = self
            .scope_tables
            .lock()
            .expect("rust scope tables")
            .get(path)
        {
            return (hit.clone(), true);
        }
        let (public, mut complete) = self.export_table(path, stack);
        let Some(facts) = self.facts.get(path) else {
            return (public, complete);
        };
        let mut table = (*public).clone();
        let starred = self.star_contributions(
            path,
            facts.stars.iter().filter(|star| !star.reexport),
            &table,
            stack,
            &mut complete,
        );
        for (name, resolution) in starred {
            table.entry(name).or_insert(resolution);
        }
        let table = std::sync::Arc::new(table);
        if complete {
            self.scope_tables
                .lock()
                .expect("rust scope tables")
                .insert(path.to_string(), table.clone());
        }
        (table, complete)
    }

    fn finish(
        &self,
        local: &str,
        asked: &str,
        resolution: Resolution,
    ) -> Result<Option<ResolvedImport>, ()> {
        match resolution {
            Resolution::Ambiguous => Err(()),
            Resolution::None => Ok(None),
            Resolution::Module { file, hops } => Ok(Some(ResolvedImport {
                local: local.to_string(),
                name: "*".to_string(),
                target_blob: self.blobs.get(&file).cloned().unwrap_or(ZERO_CONTENT_ID),
                target_path: file,
                target_span: Span::anchor(0),
                target_name: None,
                kind: ResolvedImportKind::Namespace,
                hops,
            })),
            Resolution::Binding {
                blob,
                span,
                name,
                kind,
                hops,
            } => Ok(Some(ResolvedImport {
                local: local.to_string(),
                name: asked.to_string(),
                target_path: self.paths.get(&blob).cloned().unwrap_or_default(),
                target_blob: blob,
                target_span: span,
                target_name: Some(name),
                kind,
                hops,
            })),
        }
    }

    /// Namespace wins first (`asked` itself names a submodule of
    /// `qualifier`); else `asked` is an item of `qualifier`'s home module.
    fn resolve_qualified(
        &self,
        from: &str,
        qualifier: &[String],
        asked: &str,
        stack: &mut Vec<String>,
        seen: &mut Vec<String>,
    ) -> (Resolution, bool) {
        let mut full = qualifier.to_vec();
        full.push(asked.to_string());
        if let HomeFile::Unique(file) = self.home_file(from, &full, seen, stack) {
            return (Resolution::Module { file, hops: 1 }, true);
        }
        match self.home_file(from, qualifier, seen, stack) {
            HomeFile::Unique(file) => self.resolve_in_module(&file, asked, stack),
            HomeFile::None | HomeFile::External => (Resolution::None, true),
            HomeFile::Ambiguous => (Resolution::Ambiguous, true),
        }
    }

    /// A one-segment qualifier naming THIS file's own inline `mod` is a
    /// same-blob hit; a bare declared or `use`-bound head resolves relative
    /// to the caller; else a corpus-wide suffix search on the module path.
    fn home_file(
        &self,
        from: &str,
        qualifier: &[String],
        seen: &mut Vec<String>,
        stack: &mut Vec<String>,
    ) -> HomeFile {
        if qualifier.is_empty() {
            return HomeFile::None;
        }
        if let [only] = qualifier {
            if self
                .facts
                .get(from)
                .is_some_and(|facts| facts.inline_mods.contains(only))
            {
                return HomeFile::Unique(from.to_string());
            }
        }
        if !matches!(qualifier[0].as_str(), "crate" | "self" | "super") {
            if let Some(home) = self.declared_home(from, qualifier) {
                return home;
            }
            if let Some(home) = self.bound_home(from, qualifier, seen, stack) {
                return home;
            }
            if let Some(lib) = self.crate_libs.get(&qualifier[0]) {
                if qualifier.len() == 1 {
                    return HomeFile::Unique(lib.clone());
                }
                let mut full = module_segments(lib);
                full.extend(qualifier[1..].iter().cloned());
                return self.exact_module(&full);
            }
        }
        let refs: Vec<&str> = qualifier.iter().map(String::as_str).collect();
        let Some(target) = module_target(from, &refs) else {
            return HomeFile::None;
        };
        // Bucket by the RESOLVED target's own last segment: `super`/`self`/
        // `crate` never appear in a file's own module path.
        let Some(last) = target.suffix.last() else {
            return HomeFile::None;
        };
        let candidates: Vec<&String> = self
            .by_last_segment
            .get(last)
            .into_iter()
            .flatten()
            .filter(|path| {
                self.module_paths
                    .get(*path)
                    .is_some_and(|segments| target.covers(segments))
            })
            .collect();
        match candidates.as_slice() {
            [] => HomeFile::None,
            [only] => HomeFile::Unique((*only).clone()),
            _ => HomeFile::Ambiguous,
        }
    }

    /// The files whose module path IS `full`, settled by the kink-4 rule.
    fn exact_module(&self, full: &[String]) -> HomeFile {
        let hits: Vec<&String> = self
            .module_paths
            .iter()
            .filter(|(_, segments)| segments.as_slice() == full)
            .map(|(path, _)| path)
            .collect();
        match hits.as_slice() {
            [] => HomeFile::None,
            [only] => HomeFile::Unique((*only).clone()),
            _ => HomeFile::Ambiguous,
        }
    }

    /// A bare head the caller's own file declares (`mod x;`): the module path
    /// is the caller's own path extended by the qualifier.
    fn declared_home(&self, from: &str, qualifier: &[String]) -> Option<HomeFile> {
        let facts = self.facts.get(from)?;
        let declared = facts.inline_mods.contains(&qualifier[0])
            || facts
                .mod_decls
                .iter()
                .any(|(name, _)| name == &qualifier[0]);
        if !declared {
            return None;
        }
        let mut full = module_segments(from);
        full.extend(qualifier.iter().cloned());
        Some(self.exact_module(&full))
    }

    /// A bare head a `use` binding names; a binding from outside the crate
    /// naming no corpus module is External.
    /// `seen` carries the binding heads already followed on this query: a
    /// `use b::a; use a::b;` pair would otherwise recurse forever.
    /// `stack` carries the export tables open above this query: a fresh one
    /// here lost the re-export cycle guard (issue rust-module-export-cycle).
    fn bound_home(
        &self,
        from: &str,
        qualifier: &[String],
        seen: &mut Vec<String>,
        stack: &mut Vec<String>,
    ) -> Option<HomeFile> {
        let facts = self.facts.get(from)?;
        let binding = facts
            .uses
            .iter()
            .find(|binding| binding.local == qualifier[0])?;
        if binding.qualifier.is_empty() && binding.asked == binding.local {
            return Some(HomeFile::External);
        }
        if seen.iter().any(|head| head == &qualifier[0]) {
            return None;
        }
        seen.push(qualifier[0].clone());
        let home = match self
            .resolve_qualified(from, &binding.qualifier, &binding.asked, stack, seen)
            .0
        {
            Resolution::Module { file, .. } => {
                if qualifier.len() == 1 {
                    HomeFile::Unique(file)
                } else {
                    let mut full = module_segments(&file);
                    full.extend(qualifier[1..].iter().cloned());
                    self.exact_module(&full)
                }
            }
            Resolution::Ambiguous => HomeFile::Ambiguous,
            _ => {
                let external_source = binding
                    .qualifier
                    .first()
                    .is_none_or(|head| !matches!(head.as_str(), "crate" | "self" | "super"));
                return if external_source {
                    Some(HomeFile::External)
                } else {
                    Some(HomeFile::None)
                };
            }
        };
        Some(home)
    }

    /// The outcome of a module-qualified call `qualifier::callee` from
    /// `from`: a corpus def, an external module, or a miss.
    pub(crate) fn module_call(
        &self,
        from: &str,
        qualifier: &[String],
        callee: &str,
    ) -> ModuleCallTarget {
        if !matches!(qualifier[0].as_str(), "crate" | "self" | "super")
            && !qualifier[0].is_empty()
            && self.facts.get(from).is_none_or(|facts| {
                !facts.inline_mods.contains(&qualifier[0])
                    && !facts
                        .mod_decls
                        .iter()
                        .any(|(name, _)| name == &qualifier[0])
                    && !facts
                        .uses
                        .iter()
                        .any(|binding| binding.local == qualifier[0])
            })
            && !self.segment_names.contains(&qualifier[0])
        {
            return ModuleCallTarget::External;
        }
        match self.home_file(from, qualifier, &mut Vec::new(), &mut Vec::new()) {
            HomeFile::Unique(file) => {
                let mut stack = Vec::new();
                match self.resolve_in_module(&file, callee, &mut stack).0 {
                    Resolution::Binding { blob, span, .. } => ModuleCallTarget::Target(blob, span),
                    _ => ModuleCallTarget::Miss,
                }
            }
            HomeFile::Ambiguous | HomeFile::None => ModuleCallTarget::Miss,
            // The prefix's own home resolves outside the corpus (`use
            // std::mem;` then `mem::take`): no name-match leg can bind.
            HomeFile::External => ModuleCallTarget::External,
        }
    }

    /// `name`'s resolution inside `file`'s WHOLE export table (built once).
    fn resolve_in_module(
        &self,
        file: &str,
        name: &str,
        stack: &mut Vec<String>,
    ) -> (Resolution, bool) {
        let (table, complete) = self.export_table(file, stack);
        (
            table.get(name).cloned().unwrap_or(Resolution::None),
            complete,
        )
    }

    /// `file`'s WHOLE export table, each name settled ONCE regardless of how
    /// many importers ask; cached outside any re-export cycle.
    fn export_table(
        &self,
        file: &str,
        stack: &mut Vec<String>,
    ) -> (std::sync::Arc<ExportTable>, bool) {
        if let Some(hit) = self.tables.lock().expect("rust module tables").get(file) {
            return (hit.clone(), true);
        }
        if stack.iter().any(|open| open == file) {
            return (std::sync::Arc::new(ExportTable::new()), false);
        }
        let Some(facts) = self.facts.get(file) else {
            return (std::sync::Arc::new(ExportTable::new()), true);
        };
        let mut table = ExportTable::new();
        if let Some(blob) = self.blobs.get(file) {
            let defs = self.defs.get(blob).map(Vec::as_slice).unwrap_or(&[]);
            for (span, name, family) in named_defs(defs) {
                let better = table
                    .get(name)
                    .is_none_or(|existing| !matches!(existing, Resolution::Binding { .. }))
                    || *family == FamilyTag::Call;
                if better {
                    table.insert(
                        name.clone(),
                        Resolution::Binding {
                            blob: blob.clone(),
                            span: *span,
                            name: name.clone(),
                            kind: ResolvedImportKind::Local,
                            hops: 0,
                        },
                    );
                }
            }
        }
        stack.push(file.to_string());
        let mut complete = true;
        for reexport in facts.uses.iter().filter(|binding| binding.reexport) {
            if table.contains_key(&reexport.local) {
                continue;
            }
            let (sub, sub_complete) = self.resolve_qualified(
                file,
                &reexport.qualifier,
                &reexport.asked,
                stack,
                &mut Vec::new(),
            );
            complete &= sub_complete;
            if let Some(found) = sub.promoted_option(ResolvedImportKind::Indirect) {
                table.insert(reexport.local.clone(), found);
            }
        }
        let starred = self.star_contributions(
            file,
            facts.stars.iter().filter(|star| star.reexport),
            &table,
            stack,
            &mut complete,
        );
        for (name, resolution) in starred {
            table.entry(name).or_insert(resolution);
        }
        stack.pop();
        let table = std::sync::Arc::new(table);
        if complete {
            self.tables
                .lock()
                .expect("rust module tables")
                .insert(file.to_string(), table.clone());
        }
        (table, complete)
    }

    /// Every name a set of star imports contributes, ambiguous where two
    /// disagree, EXCLUDING names `existing` already settles.
    fn star_contributions<'a>(
        &self,
        file: &str,
        stars: impl Iterator<Item = &'a StarImport>,
        existing: &ExportTable,
        stack: &mut Vec<String>,
        complete: &mut bool,
    ) -> ExportTable {
        let mut starred = ExportTable::new();
        for star in stars {
            let HomeFile::Unique(target) = self.home_file(file, &star.qualifier, &mut Vec::new(), stack)
            else {
                continue;
            };
            let (sub_table, sub_complete) = self.export_table(&target, stack);
            *complete &= sub_complete;
            for (name, resolution) in sub_table.iter() {
                if existing.contains_key(name) {
                    continue;
                }
                let Some(promoted) = resolution.clone().promoted_option(ResolvedImportKind::Star)
                else {
                    continue;
                };
                starred.insert(
                    name.clone(),
                    match starred.get(name) {
                        None => promoted,
                        Some(incumbent)
                            if matches!(incumbent, Resolution::Ambiguous)
                                || matches!(promoted, Resolution::Ambiguous)
                                || !same_target(incumbent, &promoted) =>
                        {
                            Resolution::Ambiguous
                        }
                        Some(incumbent) => incumbent.clone(),
                    },
                );
            }
        }
        starred
    }
}

/// A namespace binding names a module, not a def: no call/type site binds
/// through one.
fn callable_target(found: ResolvedImport) -> Option<(ContentId, Span)> {
    (found.kind != ResolvedImportKind::Namespace).then_some((found.target_blob, found.target_span))
}

/// One file's defs minus the ones at a COLLAPSED span. Every def spliced out
/// of one macro expansion reports the macro call's own span, so such a span
/// names nothing; a def there survives only when its name has no other site.
fn named_defs(
    defs: &[(Span, String, FamilyTag)],
) -> impl Iterator<Item = &(Span, String, FamilyTag)> {
    let mut first: HashMap<Span, &str> = HashMap::new();
    let mut shared: BTreeSet<Span> = BTreeSet::new();
    for (span, name, _) in defs {
        match first.get(span) {
            Some(seen) if *seen != name.as_str() => {
                shared.insert(*span);
            }
            Some(_) => {}
            None => {
                first.insert(*span, name.as_str());
            }
        }
    }
    let clean: BTreeSet<&str> = defs
        .iter()
        .filter(|(span, _, _)| !shared.contains(span))
        .map(|(_, name, _)| name.as_str())
        .collect();
    defs.iter()
        .filter(move |(span, name, _)| !shared.contains(span) || !clean.contains(name.as_str()))
}
