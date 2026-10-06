//! The checker tier's loader: `cargo metadata` into a salsa db, then
//! rust-analyzer's own resolution over every supplied file. Seam: `rust_checker`.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ra_ap_hir::AsAssocItem;
use ra_ap_hir::{
    attach_db, Adt, AssocItem, Crate, Field, Function, GenericDef, HirDisplay, Impl, ModuleDef,
    PathResolution, Semantics, Trait, Type,
};
use ra_ap_ide::{Edition, NavigationTarget, RootDatabase, TryToNav};
use ra_ap_ide_db::defs::Definition;
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{ast, AstNode};
use tracing::Span;

use super::rust_checker::{CheckerAnswers, CheckerError, CheckerRef};
#[path = "8a_rust_checker_target.rs"]
mod target;
pub use target::{target_calls, TargetCall, TargetCalls};
#[path = "8d_rust_checker_rename.rs"]
mod rename;
#[path = "8b_rust_checker_target_types.rs"]
mod target_types;
pub use rename::{rename, RenameEdit, RenameFailure, RenameSeed};
#[path = "8e_rust_checker_modules.rs"]
mod modules;
pub use modules::{module_tree, module_tree_for_workspace, ModulePlace, RustModuleTree};
#[path = "8h_rust_names.rs"]
mod names;
pub use names::{
    all_module_places, module_places, resolve_method, resolve_path, resolve_path_at,
    resolve_prefix, scope_names, scope_names_at, Abstain, DefPlace, NamesHost,
};
#[path = "8f_rust_checker_body_edges.rs"]
mod body_edges;
pub use body_edges::{BodyEdge, BodyEdges, EdgeKind, WalkNode, WalkSession};
#[path = "8g_rust_checker_walk.rs"]
mod walk;
use crate::read::trace::{phase_span, record_phase, Phase};
use crate::read::tsi::{Arg, CoverageClaim, FactOut};
pub use target_types::{target_types, TargetTypeReference};
pub use walk::{demand_walk, WalkAnswer, WalkEdge, WalkQuestion};

pub struct FieldProbe {
    pub struct_name_start: u32,
    pub field_start: u32,
    pub field_name: String,
}

pub struct FieldRead {
    pub field_start: u32,
    pub path: String,
    pub access_start: u32,
}

pub fn field_reads(
    root: &Path,
    source: &Path,
    files: &[(String, PathBuf)],
    probes: &[FieldProbe],
    budget: Duration,
) -> Result<Vec<FieldRead>, CheckerError> {
    let (workspace, _) = super::rust_checker_session::checker_workspace(
        root,
        super::rust_checker::LoadMode::Types,
        files,
        budget,
    )?;
    let workspace = workspace.lock().unwrap();
    let host = &workspace.host;
    let vfs = &workspace.vfs;
    let wanted: HashMap<PathBuf, &str> = files
        .iter()
        .map(|(rel, absolute)| {
            (
                std::fs::canonicalize(absolute).unwrap_or_else(|_| absolute.clone()),
                rel.as_str(),
            )
        })
        .collect();
    let source = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let mut by_file_id = HashMap::new();
    let mut source_id = None;
    for (vfs_id, vfs_path) in vfs.iter() {
        let Some(absolute) = vfs_path.as_path() else {
            continue;
        };
        let text = absolute.to_string();
        let key = std::fs::canonicalize(&text).unwrap_or_else(|_| PathBuf::from(&text));
        let file_id = ra_ap_ide::FileId::from_raw(vfs_id.index());
        if key == source {
            source_id = Some(file_id);
        }
        if let Some(rel) = wanted.get(&key) {
            by_file_id.insert(file_id, (*rel).to_string());
        }
    }
    let source_id = source_id.ok_or_else(|| {
        CheckerError::NoWorkspace(format!(
            "{} is outside the loaded crate graph",
            source.display()
        ))
    })?;
    let db = host.raw_database();
    attach_db(db, || {
        let sema = Semantics::new(db);
        let syntax = sema.parse_guess_edition(source_id);
        let structs: HashMap<u32, ast::Struct> = syntax
            .syntax()
            .descendants()
            .filter_map(ast::Struct::cast)
            .filter_map(|item| {
                let start = u32::from(item.name()?.syntax().text_range().start());
                Some((start, item))
            })
            .collect();
        let mut reads = Vec::new();
        for probe in probes {
            let item = structs.get(&probe.struct_name_start).ok_or_else(|| {
                CheckerError::NoWorkspace(format!(
                    "moved struct at byte {} is unavailable in the loaded crate graph",
                    probe.struct_name_start
                ))
            })?;
            let strukt = sema.to_def(item).ok_or_else(|| {
                CheckerError::NoWorkspace(format!(
                    "moved struct at byte {} has no rust-analyzer definition",
                    probe.struct_name_start
                ))
            })?;
            let field = strukt
                .fields(db)
                .into_iter()
                .find(|field| field.name(db).as_str() == probe.field_name)
                .ok_or_else(|| {
                    CheckerError::NoWorkspace(format!(
                        "field {} at byte {} has no rust-analyzer definition",
                        probe.field_name, probe.field_start
                    ))
                })?;
            for (file, references) in Definition::Field(field).usages(&sema).all().references {
                let file_id = file.file_id(db);
                let Some(path) = by_file_id.get(&file_id) else {
                    continue;
                };
                for reference in references {
                    reads.push(FieldRead {
                        field_start: probe.field_start,
                        path: path.clone(),
                        access_start: u32::from(reference.range.start()),
                    });
                }
            }
        }
        Ok(reads)
    })
}

/// One corpus file the walk visits: its supplied path, its ra file id, its text.
struct WalkFile {
    path: String,
    file_id: ra_ap_ide::FileId,
    text: String,
}

pub fn answer(
    root: &Path,
    files: &[(String, PathBuf)],
    budget: Duration,
    tsi: bool,
) -> Result<CheckerAnswers, CheckerError> {
    let (workspace, load) = super::rust_checker_session::checker_workspace(
        root,
        super::rust_checker::LoadMode::Types,
        files,
        budget,
    )?;
    let workspace = workspace.lock().unwrap();
    let host = &workspace.host;
    let vfs = &workspace.vfs;
    let wanted: HashMap<PathBuf, &str> = files
        .iter()
        .map(|(supplied, absolute)| {
            let key = std::fs::canonicalize(absolute).unwrap_or_else(|_| absolute.clone());
            (key, supplied.as_str())
        })
        .collect();
    let mut by_file_id: HashMap<ra_ap_ide::FileId, &str> = HashMap::new();
    // One string per workspace file, so it is built only for a run whose
    // envelope reads it: a leaf type's origin names the file declaring it.
    let mut path_of: HashMap<ra_ap_ide::FileId, String> = HashMap::new();
    for (vfs_id, vfs_path) in vfs.iter() {
        let Some(absolute) = vfs_path.as_path() else {
            continue;
        };
        let text = absolute.to_string();
        let key = std::fs::canonicalize(&text).unwrap_or_else(|_| PathBuf::from(&text));
        let file_id = ra_ap_ide::FileId::from_raw(vfs_id.index());
        if let Some(supplied) = wanted.get(&key) {
            by_file_id.insert(file_id, supplied);
        }
        if tsi {
            path_of.insert(file_id, text);
        }
    }

    let db = host.raw_database();
    let walk_started = Instant::now();
    let _query_span =
        crate::read::trace::tracked(tracing::info_span!("rust_analyzer.queries")).entered();
    let mut answers = CheckerAnswers {
        load,
        ..CheckerAnswers::default()
    };

    // The next-solver interner reads a THREAD-attached db; without this every
    // resolve panics in hir_ty's `next_solver/interner.rs`.
    let walk_files: Vec<WalkFile> = attach_db(db, || {
        let sema = Semantics::new(db);
        by_file_id
            .iter()
            .map(|(file_id, path)| {
                let text = sema
                    .parse_guess_edition(*file_id)
                    .syntax()
                    .text()
                    .to_string();
                WalkFile {
                    path: (*path).to_string(),
                    file_id: *file_id,
                    text,
                }
            })
            .collect()
    });
    // Every destination coordinate is read in the SOURCE file's own offset
    // unit, so a nav into any corpus file needs that file's map in hand.
    let destination: HashMap<ra_ap_ide::FileId, &WalkFile> =
        walk_files.iter().map(|file| (file.file_id, file)).collect();
    answers.files_answered = walk_files.len();

    // A salsa handle shares the storage and carries a thread-local query stack,
    // so it is Send and NOT Sync: each chunk owns a moved clone, never a borrow.
    let pool = crate::read::project::extract_pool();
    let chunk_size = walk_files
        .len()
        .div_ceil(pool.current_num_threads().max(1))
        .max(1);
    let chunks: Vec<(RootDatabase, &[WalkFile])> = walk_files
        .chunks(chunk_size)
        .map(|chunk| (db.clone(), chunk))
        .collect();
    let per_file: Vec<FileAnswers> = pool.install(|| {
        use rayon::prelude::*;

        chunks
            .into_par_iter()
            .flat_map_iter(|(db, chunk)| {
                attach_db(&db, || {
                    let sema = Semantics::new(&db);
                    chunk
                        .iter()
                        .map(|file| walk_file(&sema, &destination, file))
                        .collect::<Vec<_>>()
                })
            })
            .collect()
    });

    for answered in per_file {
        answers.method_sites += answered.method_sites;
        answers.method_unresolved += answered.method_unresolved;
        answers.calls.insert(answered.path.clone(), answered.calls);
        answers.types.insert(answered.path, answered.types);
    }
    if tsi {
        // Ids are run-local across the whole workspace, so the item walk owns
        // one counter and runs after the per-file resolve rather than beside it.
        let (facts, coverage, unmodulated) = attach_db(db, || {
            let sema = Semantics::new(db);
            TsiWalk::new(db, &sema, &destination, &path_of).run(&walk_files)
        });
        answers.tsi = facts;
        answers.coverage = coverage;
        answers.unmodulated = unmodulated;
    }
    answers.walk = walk_started.elapsed();
    Ok(answers)
}

/// One file's share of the walk, kept per-worker so the fold is the only
/// contended write.
#[derive(Default)]
struct FileAnswers {
    path: String,
    calls: Vec<CheckerRef>,
    types: Vec<CheckerRef>,
    method_sites: usize,
    method_unresolved: usize,
}

/// One resolution kind over one file: the phase row it folds into, how many
/// rust-analyzer calls it made and how many of those answered.
struct SiteSpan {
    span: Span,
    calls: Cell<u64>,
    answered: Cell<u64>,
}

impl SiteSpan {
    fn new(phase: Phase) -> SiteSpan {
        SiteSpan {
            span: phase_span("rust", phase),
            calls: Cell::new(0),
            answered: Cell::new(0),
        }
    }

    /// Times exactly ONE rust-analyzer call. No guard here wraps another, so a
    /// span's micros are never a sum containing a sibling's.
    fn call<T>(&self, resolve: impl FnOnce() -> Option<T>) -> Option<T> {
        self.calls.set(self.calls.get() + 1);
        let answer = {
            let _entered = self.span.enter();
            resolve()
        };
        if answer.is_some() {
            self.answered.set(self.answered.get() + 1);
        }
        answer
    }

    fn record(&self) {
        record_phase(&self.span, 0, self.answered.get(), self.calls.get());
    }
}

/// The four rust-analyzer calls one file's walk pays for, priced apart, and the
/// nav answers already in hand.
struct SiteSpans {
    method: SiteSpan,
    call_path: SiteSpan,
    type_path: SiteSpan,
    nav: SiteSpan,
    navs: RefCell<HashMap<ModuleDef, Option<NavigationTarget>>>,
}

impl SiteSpans {
    fn new() -> SiteSpans {
        SiteSpans {
            method: SiteSpan::new(Phase::CheckerMethod),
            call_path: SiteSpan::new(Phase::CheckerCallPath),
            type_path: SiteSpan::new(Phase::CheckerTypePath),
            nav: SiteSpan::new(Phase::CheckerNav),
            navs: RefCell::new(HashMap::new()),
        }
    }

    /// One `try_to_nav` per DEFINITION, never per site: it reparses the file the
    /// destination is declared in. `checker_nav`'s `calls` counts memo MISSES.
    fn destination_of(
        &self,
        sema: &Semantics<'_, RootDatabase>,
        def: ModuleDef,
    ) -> Option<NavigationTarget> {
        if let Some(known) = self.navs.borrow().get(&def) {
            return known.clone();
        }
        let nav = self.nav.call(|| nav_of(sema, def));
        self.navs.borrow_mut().insert(def, nav.clone());
        nav
    }

    fn record(&self) {
        self.method.record();
        self.call_path.record();
        self.type_path.record();
        self.nav.record();
    }
}

/// A path under an `ast::PathType`, qualifiers included. An expression-position
/// path is the syntax leg's; the call and record arms took the call-shaped ones.
fn in_type_position(path: &ast::Path) -> bool {
    path.syntax()
        .ancestors()
        .any(|node| ast::PathType::can_cast(node.kind()))
}

fn walk_file(
    sema: &Semantics<'_, RootDatabase>,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
) -> FileAnswers {
    let source = sema.parse_guess_edition(file.file_id);
    let spans = SiteSpans::new();
    let mut out = FileAnswers {
        path: file.path.clone(),
        ..FileAnswers::default()
    };
    for node in source.syntax().descendants() {
        if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
            out.method_sites += 1;
            match method_call_ref(sema, destination, file, &spans, &call) {
                Some(reference) => out.calls.push(reference),
                None => out.method_unresolved += 1,
            }
            continue;
        }
        if let Some(call) = ast::CallExpr::cast(node.clone()) {
            if let Some(ast::Expr::PathExpr(path_expr)) = call.expr() {
                if let Some(path) = path_expr.path() {
                    if let Some(reference) = path_call_ref(sema, destination, file, &spans, &path) {
                        out.calls.push(reference);
                    }
                }
            }
            continue;
        }
        if let Some(record) = ast::RecordExpr::cast(node.clone()) {
            if let Some(path) = record.path() {
                if let Some(reference) = path_call_ref(sema, destination, file, &spans, &path) {
                    out.calls.push(reference);
                }
            }
            continue;
        }
        if let Some(path) = ast::Path::cast(node) {
            if !in_type_position(&path) {
                continue;
            }
            if let Some(reference) = type_ref(sema, destination, file, &spans, &path) {
                out.types.push(reference);
            }
        }
    }
    spans.record();
    out
}

/// `recv.m(..)`: the method the compiler dispatches to, receiver type and trait
/// resolution included. The reference range is the method identifier alone.
fn method_call_ref(
    sema: &Semantics<'_, RootDatabase>,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
    spans: &SiteSpans,
    call: &ast::MethodCallExpr,
) -> Option<CheckerRef> {
    let name_ref = call.name_ref()?;
    let function = spans.method.call(|| sema.resolve_method_call(call))?;
    let nav = spans.destination_of(sema, ModuleDef::Function(function))?;
    mint(
        sema,
        ModuleDef::Function(function),
        destination,
        file,
        name_ref.syntax().text_range(),
        &nav,
    )
}

/// `a::b::c(..)` and `Foo { .. }`: the item the trailing segment names.
fn path_call_ref(
    sema: &Semantics<'_, RootDatabase>,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
    spans: &SiteSpans,
    path: &ast::Path,
) -> Option<CheckerRef> {
    let name_ref = path.segment()?.name_ref()?;
    let PathResolution::Def(def) = spans.call_path.call(|| sema.resolve_path(path))? else {
        return None;
    };
    if matches!(def, ModuleDef::Module(_) | ModuleDef::BuiltinType(_)) {
        return None;
    }
    let nav = spans.destination_of(sema, def)?;
    mint(
        sema,
        def,
        destination,
        file,
        name_ref.syntax().text_range(),
        &nav,
    )
}

/// A path naming a type declaration, the shape `Resolve<TypeF>`'s candidates
/// carry. Anything else on the path plane is left to the syntax leg.
fn type_ref(
    sema: &Semantics<'_, RootDatabase>,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
    spans: &SiteSpans,
    path: &ast::Path,
) -> Option<CheckerRef> {
    let name_ref = path.segment()?.name_ref()?;
    let PathResolution::Def(def) = spans.type_path.call(|| sema.resolve_path(path))? else {
        return None;
    };
    if !matches!(
        def,
        ModuleDef::Adt(_) | ModuleDef::Trait(_) | ModuleDef::TypeAlias(_)
    ) {
        return None;
    }
    let nav = spans.destination_of(sema, def)?;
    let mut reference = mint(
        sema,
        def,
        destination,
        file,
        name_ref.syntax().text_range(),
        &nav,
    )?;
    reference.written = path
        .segments()
        .filter_map(|segment| segment.name_ref())
        .map(|name| name.text().to_string())
        .collect::<Vec<String>>()
        .join("::");
    Some(reference)
}

fn nav_of(sema: &Semantics<'_, RootDatabase>, def: ModuleDef) -> Option<NavigationTarget> {
    def.try_to_nav(sema).map(|nav| nav.call_site)
}

/// One nav plus one reference range -> a seam row, in rust-analyzer's byte offsets.
fn mint(
    sema: &Semantics<'_, RootDatabase>,
    def: ModuleDef,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
    reference: ra_ap_syntax::TextRange,
    nav: &NavigationTarget,
) -> Option<CheckerRef> {
    let start = u32::from(reference.start());
    let end = u32::from(reference.end());
    // A destination outside the resolve universe is an ANSWER: the empty path
    // says "resolved, and no corpus definition is it".
    let (dst_path, dst_offset) = match destination.get(&nav.file_id) {
        Some(target) => {
            let declaration = nav.focus_range.unwrap_or(nav.full_range).start();
            (target.path.clone(), u32::from(declaration))
        }
        None => (String::new(), 0),
    };
    // Outside the universe, the name is the definition's crate-qualified
    // path (`tokio::task::spawn::spawn`, `std::process::Command::new`).
    let dst_name = if dst_path.is_empty() {
        qualified(sema, def).unwrap_or_else(|| nav.name.as_str().to_string())
    } else {
        nav.name.as_str().to_string()
    };
    Some(CheckerRef {
        start,
        end,
        name: file.text.get(start as usize..end as usize)?.to_string(),
        written: String::new(),
        dst_path,
        dst_name,
        dst_offset,
    })
}

/// `crate::module::[SelfType|Trait::]name` of a definition, by its declaring module.
fn qualified(sema: &Semantics<'_, RootDatabase>, def: ModuleDef) -> Option<String> {
    let db = sema.db;
    let module = def.module(db)?;
    let krate = module.krate(db).display_name(db)?.to_string();
    let owner =
        match def {
            ModuleDef::Function(function) => function.as_assoc_item(db).and_then(|item| match item
                .container(db)
            {
                ra_ap_hir::AssocItemContainer::Trait(owner) => Some(owner.name(db)),
                ra_ap_hir::AssocItemContainer::Impl(owner) => {
                    owner.self_ty(db).as_adt().map(|adt| adt.name(db))
                }
            }),
            _ => None,
        };
    let segments = std::iter::once(krate)
        .chain(
            module
                .path_segments(db)
                .map(|name| name.display(db, Edition::CURRENT).to_string()),
        )
        .chain(owner.map(|name| name.display(db, Edition::CURRENT).to_string()))
        .chain(Some(
            def.name(db)?.display(db, Edition::CURRENT).to_string(),
        ));
    Some(segments.collect::<Vec<String>>().join("::"))
}

/// Every relation the item walk enumerates to exhaustion. A claim is emitted
/// only where the walk produced a row: `complete` over nothing says too much.
const ENUMERATED: &[&str] = &[
    "tsi.type",
    "tsi.denotes",
    "tsi.origin",
    "tsi.name",
    "tsi.product",
    "tsi.sum",
    "tsi.callable",
    "tsi.primitive",
    "tsi.parameter",
    "tsi.called",
    "tsi.argument",
    "tsi.input",
    "tsi.output",
    "rust.trait",
    "rust.impl",
    "rust.assoc",
    "rust.lifetime",
    "rust.ownership",
];

/// Every relation the walk samples rather than enumerates, with the sentence a
/// partial claim carries beside it.
const SAMPLED: &[(&str, &str)] = &[
    (
        "tsi.edge",
        "enumerated for owners declared in the supplied files",
    ),
    (
        "tsi.conforms",
        "declared impls of supplied types and traits; blanket and auto traits not enumerated",
    ),
    ("tsi.has_type", "occurrences not walked in this arc"),
    ("tsi.subtype", "not enumerated"),
    ("tsi.assignable", "not enumerated"),
    ("tsi.equivalent", "not enumerated"),
];

/// Run-local identity over one workspace walk. Rule 1 has two halves: a
/// declaration is its `ModuleDef`, a structure is its rendering inside a crate.
struct TsiWalk<'db, 'a> {
    db: &'db RootDatabase,
    sema: &'a Semantics<'db, RootDatabase>,
    destination: &'a HashMap<ra_ap_ide::FileId, &'a WalkFile>,
    path_of: &'a HashMap<ra_ap_ide::FileId, String>,
    next: u32,
    nominal: HashMap<ModuleDef, u32>,
    structural: HashMap<(Crate, String), u32>,
    described: HashSet<u32>,
    facts: Vec<FactOut>,
}

impl<'db, 'a> TsiWalk<'db, 'a> {
    fn new(
        db: &'db RootDatabase,
        sema: &'a Semantics<'db, RootDatabase>,
        destination: &'a HashMap<ra_ap_ide::FileId, &'a WalkFile>,
        path_of: &'a HashMap<ra_ap_ide::FileId, String>,
    ) -> Self {
        TsiWalk {
            db,
            sema,
            destination,
            path_of,
            next: 0,
            nominal: HashMap::new(),
            structural: HashMap::new(),
            described: HashSet::new(),
            facts: Vec::new(),
        }
    }

    /// The declarations of every module a supplied file owns, then the impls of
    /// those declarations alone: a crate's whole impl set prices the walk by it.
    fn run(mut self, files: &[WalkFile]) -> (Vec<FactOut>, Vec<CoverageClaim>, Vec<String>) {
        let mut modules: Vec<ra_ap_hir::Module> = Vec::new();
        let mut unmodulated: Vec<String> = Vec::new();
        for file in files {
            let owned: Vec<ra_ap_hir::Module> =
                self.sema.file_to_module_defs(file.file_id).collect();
            if owned.is_empty() {
                unmodulated.push(file.path.clone());
            }
            for module in owned {
                if !modules.contains(&module) {
                    modules.push(module);
                }
            }
        }
        let mut adts: Vec<Adt> = Vec::new();
        let mut traits: Vec<Trait> = Vec::new();
        for module in modules {
            let krate = module.krate(self.db);
            for def in module.declarations(self.db) {
                match def {
                    ModuleDef::Adt(item) => adts.push(item),
                    ModuleDef::Trait(item) => traits.push(item),
                    _ => {}
                }
                self.declaration(def, krate);
            }
        }
        let mut seen: HashSet<Impl> = HashSet::new();
        let mut impls: Vec<Impl> = Vec::new();
        for adt in adts {
            for item in Impl::all_for_type(self.db, adt.ty(self.db)) {
                if seen.insert(item) {
                    impls.push(item);
                }
            }
        }
        for contract in traits {
            for item in Impl::all_for_trait(self.db, contract) {
                if seen.insert(item) {
                    impls.push(item);
                }
            }
        }
        for item in impls {
            let krate = item.module(self.db).krate(self.db);
            self.implementation(item, krate);
        }
        let claims = claims(&self.facts);
        (self.facts, claims, unmodulated)
    }

    fn row(&mut self, relation: &str, args: Vec<Arg>) {
        debug_assert!(
            crate::read::tsi::registry::check(relation, &args).is_ok(),
            "{relation}: {:?}",
            crate::read::tsi::registry::check(relation, &args)
        );
        self.facts.push(FactOut {
            fact: 0,
            relation: relation.to_string(),
            args,
        });
    }

    fn fresh(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }

    /// True the first time an id is handed out for description, so a type
    /// reached twice carries one shape.
    fn first_visit(&mut self, id: u32) -> bool {
        self.described.insert(id)
    }

    /// Rule 1's nominal half. The `tsi.type` and `tsi.origin` rows are minted
    /// with the id, so every id an argument names is declared by construction.
    fn nominal(&mut self, def: ModuleDef) -> u32 {
        if let Some(id) = self.nominal.get(&def) {
            return *id;
        }
        let id = self.fresh();
        self.nominal.insert(def, id);
        self.row("tsi.type", vec![Arg::Id(id)]);
        if let Some(name) = def.name(self.db) {
            self.name(id, name.as_str());
        }
        let krate = def.module(self.db).map(|module| module.krate(self.db));
        let origin = self.origin_at(nav_of(self.sema, def), krate);
        self.row(
            "tsi.origin",
            vec![Arg::Id(id), Arg::Atom("rust".to_string()), origin],
        );
        id
    }

    /// Rule 1's structural half: two types that render alike inside one crate
    /// are one type, and the rendering is the only string the id costs.
    fn rendered(&mut self, ty: &Type<'db>, krate: Crate) -> (u32, bool) {
        let target = krate.to_display_target(self.db);
        let key = (krate, ty.display(self.db, target).to_string());
        if let Some(id) = self.structural.get(&key) {
            return (*id, false);
        }
        let id = self.fresh();
        let rendered = key.1.clone();
        self.structural.insert(key, id);
        self.row("tsi.type", vec![Arg::Id(id)]);
        self.name(id, &rendered);
        (id, true)
    }

    /// `tsi.name`: the spelling a consumer prints for a type or a symbol.
    fn name(&mut self, id: u32, text: &str) {
        self.row("tsi.name", vec![Arg::Id(id), Arg::Text(text.to_string())]);
    }

    /// A declaration is a symbol and a type at once, and `tsi.denotes` is the
    /// join a consumer follows from one to the other.
    fn declared(&mut self, def: ModuleDef) -> (u32, bool) {
        let id = self.nominal(def);
        let fresh = self.first_visit(id);
        if fresh {
            let symbol = self.fresh();
            self.row("tsi.symbol", vec![Arg::Id(symbol)]);
            if let Some(name) = def.name(self.db) {
                self.name(symbol, name.as_str());
            }
            self.row("tsi.denotes", vec![Arg::Id(symbol), Arg::Id(id)]);
        }
        (id, fresh)
    }

    /// A declaration origins at its own name's byte range, under the supplied
    /// path for a corpus file and the absolute path for any other.
    fn origin_at(&mut self, nav: Option<NavigationTarget>, krate: Option<Crate>) -> Arg {
        let fallback = || {
            let name = krate
                .and_then(|krate| krate.display_name(self.db))
                .map(|name| name.to_string())
                .unwrap_or_else(|| "rust".to_string());
            Arg::Span(name, 0, 0)
        };
        let Some(nav) = nav else { return fallback() };
        let range = nav.focus_range.unwrap_or(nav.full_range);
        let start = u32::from(range.start());
        let end = u32::from(range.end());
        let path = self
            .destination
            .get(&nav.file_id)
            .map(|target| target.path.clone())
            .or_else(|| self.path_of.get(&nav.file_id).cloned());
        match path {
            Some(path) => Arg::Span(path, start, end),
            None => fallback(),
        }
    }

    fn declaration(&mut self, def: ModuleDef, krate: Crate) {
        match def {
            ModuleDef::Adt(Adt::Struct(item)) => {
                let (id, fresh) = self.declared(def);
                if !fresh {
                    return;
                }
                self.generics(id, GenericDef::from(item), krate);
                self.row("tsi.product", vec![Arg::Id(id)]);
                self.fields(id, item.fields(self.db), krate);
            }
            ModuleDef::Adt(Adt::Union(item)) => {
                let (id, fresh) = self.declared(def);
                if !fresh {
                    return;
                }
                self.generics(id, GenericDef::from(item), krate);
                self.row("tsi.product", vec![Arg::Id(id)]);
                self.fields(id, item.fields(self.db), krate);
            }
            ModuleDef::Adt(Adt::Enum(item)) => {
                let (id, fresh) = self.declared(def);
                if !fresh {
                    return;
                }
                self.generics(id, GenericDef::from(item), krate);
                self.row("tsi.sum", vec![Arg::Id(id)]);
                for (position, variant) in item.variants(self.db).into_iter().enumerate() {
                    let owned = self.nominal(ModuleDef::EnumVariant(variant));
                    if self.first_visit(owned) {
                        self.row("tsi.product", vec![Arg::Id(owned)]);
                        self.fields(owned, variant.fields(self.db), krate);
                    }
                    let edge = self.fresh();
                    let label = variant.name(self.db).as_str().to_string();
                    self.row(
                        "tsi.edge",
                        vec![
                            Arg::Id(edge),
                            Arg::Id(id),
                            Arg::Text(label),
                            Arg::Id(owned),
                            Arg::Int(position as i64),
                        ],
                    );
                }
            }
            ModuleDef::Trait(item) => {
                let (id, fresh) = self.declared(def);
                if !fresh {
                    return;
                }
                self.generics(id, GenericDef::from(item), krate);
                self.row("rust.trait", vec![Arg::Id(id)]);
                for assoc in item.items(self.db) {
                    self.assoc_item(id, assoc, krate);
                }
            }
            ModuleDef::Function(item) => self.callable(item, krate),
            _ => {}
        }
    }

    /// A trait's own associated type has no right-hand side, so its target is
    /// the alias declaration itself: an opaque id that still origins at a name.
    fn assoc_item(&mut self, owner: u32, assoc: AssocItem, krate: Crate) {
        match assoc {
            AssocItem::TypeAlias(alias) => {
                let target = if alias.has_type(self.db) {
                    self.type_id(&alias.ty(self.db), krate)
                } else {
                    self.nominal(ModuleDef::TypeAlias(alias))
                };
                let name = alias.name(self.db).as_str().to_string();
                self.row(
                    "rust.assoc",
                    vec![Arg::Id(owner), Arg::Text(name), Arg::Id(target)],
                );
            }
            AssocItem::Function(item) => self.callable(item, krate),
            AssocItem::Const(_) => {}
        }
    }

    fn implementation(&mut self, item: Impl, krate: Crate) {
        let Some(contract) = item.trait_(self.db) else {
            return;
        };
        let Some(adt) = item.self_ty(self.db).as_adt() else {
            return;
        };
        let owner = self.nominal(ModuleDef::Adt(adt));
        let contract = self.nominal(ModuleDef::Trait(contract));
        let id = self.fresh();
        self.row(
            "rust.impl",
            vec![Arg::Id(id), Arg::Id(owner), Arg::Id(contract)],
        );
        self.row(
            "tsi.conforms",
            vec![
                Arg::Id(owner),
                Arg::Id(contract),
                Arg::Atom("declared".to_string()),
            ],
        );
        for assoc in item.items(self.db) {
            self.assoc_item(owner, assoc, krate);
        }
    }

    fn callable(&mut self, item: Function, krate: Crate) {
        let (id, fresh) = self.declared(ModuleDef::Function(item));
        if !fresh {
            return;
        }
        self.generics(id, GenericDef::from(item), krate);
        self.row("tsi.callable", vec![Arg::Id(id)]);
        for (position, param) in item.params_without_self(self.db).into_iter().enumerate() {
            let param = self.type_id(param.ty(), krate);
            self.row(
                "tsi.input",
                vec![Arg::Id(id), Arg::Int(position as i64), Arg::Id(param)],
            );
        }
        let produced = self.type_id(&item.ret_type(self.db), krate);
        self.row(
            "tsi.output",
            vec![Arg::Id(id), Arg::Int(0), Arg::Id(produced)],
        );
    }

    /// Each field is one edge plus the word its type spells about who owns the
    /// bytes behind it; a borrow and a smart pointer both target their pointee.
    fn fields(&mut self, owner: u32, fields: Vec<Field>, krate: Crate) {
        for field in fields {
            let declared = field.ty(self.db);
            let (ownership, target) = ownership_of(self.db, &declared);
            let target = self.type_id(&target, krate);
            let edge = self.fresh();
            let label = field.name(self.db).as_str().to_string();
            self.row(
                "tsi.edge",
                vec![
                    Arg::Id(edge),
                    Arg::Id(owner),
                    Arg::Text(label),
                    Arg::Id(target),
                    Arg::Int(field.index() as i64),
                ],
            );
            self.row(
                "rust.ownership",
                vec![Arg::Id(edge), Arg::Atom(ownership.to_string())],
            );
        }
    }

    /// rust-analyzer exposes no variance for a type parameter, so the position
    /// carries `unspecified` rather than a word the compiler never said.
    fn generics(&mut self, owner: u32, def: GenericDef, krate: Crate) {
        let declared: Vec<ra_ap_hir::TypeParam> = def
            .type_or_const_params(self.db)
            .into_iter()
            .filter_map(|param| param.as_type_param(self.db))
            .filter(|param| !param.is_implicit(self.db))
            .collect();
        for (position, param) in declared.into_iter().enumerate() {
            let (id, fresh) = self.rendered(&param.ty(self.db), krate);
            // Two owners writing the same parameter name render alike and are one
            // id, so the bounds ride the id rather than the owner that reached it.
            if fresh {
                let nav = param.try_to_nav(self.sema).map(|nav| nav.call_site);
                let origin = self.origin_at(nav, Some(krate));
                self.row(
                    "tsi.origin",
                    vec![Arg::Id(id), Arg::Atom("rust".to_string()), origin],
                );
                for (rank, bound) in param.trait_bounds(self.db).into_iter().enumerate() {
                    let bound = self.nominal(ModuleDef::Trait(bound));
                    let edge = self.fresh();
                    self.row(
                        "tsi.edge",
                        vec![
                            Arg::Id(edge),
                            Arg::Id(id),
                            Arg::Text("bound".to_string()),
                            Arg::Id(bound),
                            Arg::Int(rank as i64),
                        ],
                    );
                }
            }
            self.row(
                "tsi.parameter",
                vec![
                    Arg::Id(id),
                    Arg::Id(owner),
                    Arg::Int(position as i64),
                    Arg::Atom("unspecified".to_string()),
                ],
            );
        }
        for param in def.lifetime_params(self.db) {
            let name = param.name(self.db);
            let name = name.as_str().trim_start_matches('\'').to_string();
            self.row("rust.lifetime", vec![Arg::Id(owner), Arg::Atom(name)]);
        }
    }

    /// The id for one type. A bare declaration takes rule 1's nominal id; an
    /// application takes its rendering's, and declares its parts beside it.
    fn type_id(&mut self, ty: &Type<'db>, krate: Crate) -> u32 {
        if let Some(builtin) = ty.as_builtin() {
            let id = self.nominal(ModuleDef::BuiltinType(builtin));
            if self.first_visit(id) {
                let name = builtin.name().as_str().to_string();
                self.row("tsi.primitive", vec![Arg::Id(id), Arg::Atom(name)]);
            }
            return id;
        }
        let arguments: Vec<Type<'db>> = ty.type_arguments().collect();
        if let Some(adt) = ty.as_adt() {
            if arguments.is_empty() {
                return self.nominal(ModuleDef::Adt(adt));
            }
            let (id, fresh) = self.rendered(ty, krate);
            if fresh {
                let nav = nav_of(self.sema, ModuleDef::Adt(adt));
                let origin = self.origin_at(nav, Some(krate));
                self.row(
                    "tsi.origin",
                    vec![Arg::Id(id), Arg::Atom("rust".to_string()), origin],
                );
                let constructor = self.nominal(ModuleDef::Adt(adt));
                let list = self.fresh();
                self.row(
                    "tsi.called",
                    vec![Arg::Id(id), Arg::Id(constructor), Arg::Id(list)],
                );
                for (position, argument) in arguments.iter().enumerate() {
                    let argument = self.type_id(argument, krate);
                    self.row(
                        "tsi.argument",
                        vec![Arg::Id(list), Arg::Int(position as i64), Arg::Id(argument)],
                    );
                }
            }
            return id;
        }
        let (id, fresh) = self.rendered(ty, krate);
        if !fresh {
            return id;
        }
        let origin = self.origin_at(None, Some(krate));
        self.row(
            "tsi.origin",
            vec![Arg::Id(id), Arg::Atom("rust".to_string()), origin],
        );
        if let Some(callable) = ty.as_callable(self.db) {
            self.row("tsi.callable", vec![Arg::Id(id)]);
            for (position, param) in callable.params().into_iter().enumerate() {
                let param = self.type_id(param.ty(), krate);
                self.row(
                    "tsi.input",
                    vec![Arg::Id(id), Arg::Int(position as i64), Arg::Id(param)],
                );
            }
            let produced = self.type_id(&callable.return_type(), krate);
            self.row(
                "tsi.output",
                vec![Arg::Id(id), Arg::Int(0), Arg::Id(produced)],
            );
        }
        id
    }
}

/// The word a field's type spells about who owns the bytes behind it, and the
/// type the edge then targets: a wrapper is the word, never a node of its own.
fn ownership_of<'db>(db: &'db RootDatabase, ty: &Type<'db>) -> (&'static str, Type<'db>) {
    if ty.is_reference() {
        let word = if ty.is_mutable_reference() {
            "exclusive"
        } else {
            "shared"
        };
        return (word, ty.strip_reference());
    }
    if let Some(adt) = ty.as_adt() {
        let word = match adt.name(db).as_str() {
            "Box" => "owned",
            "Rc" | "Arc" => "shared",
            _ => return ("owned", ty.clone()),
        };
        if let Some(inner) = ty.type_arguments().next() {
            return (word, inner);
        }
    }
    ("owned", ty.clone())
}

fn claims(facts: &[FactOut]) -> Vec<CoverageClaim> {
    let emitted: HashSet<&str> = facts.iter().map(|fact| fact.relation.as_str()).collect();
    ENUMERATED
        .iter()
        .filter(|relation| emitted.contains(*relation))
        .map(|relation| CoverageClaim {
            relation: (*relation).to_string(),
            complete: true,
            diagnostic: None,
        })
        .chain(SAMPLED.iter().map(|(relation, detail)| CoverageClaim {
            relation: (*relation).to_string(),
            complete: false,
            diagnostic: Some((*detail).to_string()),
        }))
        .collect()
}
