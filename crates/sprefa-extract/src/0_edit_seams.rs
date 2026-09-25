use crate::types::Span;
use crate::types::LangKind;
use crate::move_cx::MoveCx;
use crate::types::Source;
use std::collections::BTreeSet;
use std::fmt;
use crate::rename_cx::{RenameCx, RenameRequest};

/// One import-shaped reference a move respells. `literal` and `text` cover it
/// AS WRITTEN, quotes included: a respell reproduces the quote style.
pub struct ImportRef {
    /// Project-relative path of the file that writes the reference.
    pub importer: String,
    pub literal: Span,
    /// The bytes `literal` spans.
    pub text: String,
    /// Project-relative path the reference names, pre-move.
    pub target: String,
    pub kind: ImportRefKind,
}

/// The vocabulary of one `ImportRef`. Core = the kinds two or more languages
/// construct today; a kind one language owns lives in that language's rehome
/// file as an `Ext(LangKind)` constant, so a new language never edits this list.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImportRefKind {
    /// A module import (`use`, `import`, `:- use_module`).
    Import,
    /// A quoted path literal outside an import form.
    PathLiteral,
    /// A package.json / Cargo.toml target line.
    ManifestTarget,
    /// A kind one language owns; tag never equals a core tag (railed in
    /// tests/7_import_ref_kind.rs), so `as_str` stays injective.
    Ext(LangKind),
}

impl ImportRefKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            ImportRefKind::Import => "import",
            ImportRefKind::PathLiteral => "path_literal",
            ImportRefKind::ManifestTarget => "manifest_target",
            ImportRefKind::Ext(ext) => ext.tag,
        }
    }
}

/// One respelled literal: the bytes soopy's Replace writes.
pub struct Respell {
    pub file: String,
    pub span: Span,
    pub text: String,
    /// The stdout line this respell reports itself with. soopy's own preview
    /// covers a staged edit, so only a report a preview does not carry is set.
    pub receipt: Option<String>,
}

/// One `ryi cleave` run's plan: what leaves SRC, what lands in DEST, and who
/// gets respelled. A dry run prints these rows and writes nothing.
#[derive(Debug, Default)]
pub struct CleavePlan {
    /// Root-relative path the item leaves.
    pub src: String,
    /// Root-relative path the item lands in, created when it is missing.
    pub dest: String,
    /// The item's declared name.
    pub item: String,
    /// The item's whole top-level declaration in SRC, export keyword included.
    pub item_span: Span,
    /// Specifiers SRC carries that the item needs, in SRC byte order.
    pub travelling: Vec<CleaveSpecifier>,
    /// Specifiers nothing left in SRC references once the item leaves.
    pub orphans: Vec<CleaveSpecifier>,
    /// Files importing `SRC#ITEM`, in path order.
    pub callers: Vec<String>,
    /// Same-file private helpers `--drag` pulls along, in SRC byte order.
    pub dragged: Vec<CleaveDrag>,
    /// Passes the drag fixpoint ran. 1 when the first pass dragged nothing.
    pub drag_iterations: u32,
    /// Names the item calls that no specifier and no declaration answer. A
    /// non-empty list declines the run.
    pub unresolved: Vec<String>,
}

/// One import specifier a cleave moves or drops. `module` is SRC's spelling,
/// `dest_module` the same target respelled against DEST's directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleaveSpecifier {
    /// The local name the specifier binds.
    pub name: String,
    pub module: String,
    pub dest_module: String,
    /// The specifier that binds `name` in SRC.
    pub span: Span,
    /// `package`, `relative`, or `carried` when DEST already imports it.
    pub kind: &'static str,
}

/// One same-file private helper the item reaches under `--drag`. A helper
/// nothing left in SRC references is `moved`; a shared one is `exported`.
#[derive(Clone, Debug)]
pub struct CleaveDrag {
    pub name: String,
    /// The helper's whole top-level declaration in SRC.
    pub span: Span,
    /// The one-based fixpoint pass that claimed it.
    pub iteration: u32,
    /// `moved` when it travels to DEST, `exported` when it stays in SRC, gains
    /// `export`, and DEST imports it. A helper is never copied.
    pub action: &'static str,
}

/// One rewrite a language primitive proposes: `span`'s bytes become `text`.
/// An empty `text` is a deletion; a zero-length `span` is an insertion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub text: String,
}

/// The text a language cannot be edited without. A verb plans from fact rows
/// and asks here for the three spellings no fact carries.
pub trait Cleave: Source + Sync + Send {
    /// `text`'s declaration at `decl` with its export marker on or off. None
    /// when it already reads that way.
    fn edit_export(&self, text: &str, decl: Span, on: bool) -> Option<Edit>;

    /// `text`'s import of `module` rewritten to bind exactly `names`: added
    /// when there is none, removed whole when `names` is empty. None: no change.
    fn edit_import(&self, text: &str, names: &[String], module: &str) -> Option<Edit>;

    /// `edit_import`, with an added import declared as visibly as the one
    /// that bound `like` from `like_module` (a re-export stays a re-export).
    fn edit_import_like(
        &self,
        text: &str,
        names: &[String],
        module: &str,
        _like: &str,
        _like_module: &str,
    ) -> Option<Edit> {
        self.edit_import(text, names, module)
    }

    /// How a file at `from_path` spells `to_path` as a module. The corpus
    /// supplies declarations such as Rust's `#[path] mod name`.
    fn spell_module(&self, cx: &MoveCx, from_path: &str, to_path: &str) -> String;

    /// Imports in a parent module may be consumed through a child's glob.
    /// Such imports stay until that cross-module use is resolved explicitly.
    fn imports_visible_to_children(&self, _cx: &MoveCx, _src: &str) -> bool {
        false
    }

    /// The edit that declares a DEST this cleave creates, in the file that must
    /// name it (Rust's parent `mod`), as (file, edit). None: nothing declares files.
    fn declare_new_file(&self, _cx: &MoveCx, _src: &str, _dest: &str) -> Option<(String, Edit)> {
        None
    }

    /// The edit that makes an existing DEST's module public, as (file, edit),
    /// when another crate now names it. None: already public, or no such decl.
    fn publish_module(&self, _cx: &MoveCx, _dest: &str) -> Option<(String, Edit)> {
        None
    }
}

/// What one language answers when a file it owns moves. Held `&'static` in the
/// `rehomes()` roster beside `sources()`; one impl per language, no mutable state.
pub trait Rehome: Source + Sync + Send {
    /// Every reference this language owns that `cx`'s batch can reach, one parse
    /// per file. Batch-gated: a resolver call is a syscall per specifier.
    fn import_refs(&self, cx: &MoveCx) -> Vec<ImportRef>;

    /// The literal text for `reference` once `cx`'s batch lands (importer AND
    /// target may both move). None = unchanged.
    fn respell(&self, cx: &MoveCx, reference: &ImportRef) -> Option<Respell>;

    /// The file name whose stem stands for its directory ("mod" for Rust,
    /// "index" for TS). None: no directory-standing file in this language.
    fn directory_stem(&self) -> Option<&'static str> {
        None
    }

    /// The names a batch can be reached by: every moved file's stem, plus the
    /// directory name of a moved directory-standing file, which is the module
    /// name a decl (or a directory-form specifier) spells.
    fn moved_names(&self, cx: &MoveCx) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for old in cx.moved().keys() {
            if !crate::move_cx::owned_by(old, self) {
                continue;
            }
            let own = crate::move_cx::stem(old);
            if Some(own.as_str()) == self.directory_stem() {
                names.insert(crate::move_cx::stem(crate::move_cx::dirname(old)));
            }
            names.insert(own);
        }
        names
    }
}

/// The manifest leg of a move: languages whose package files name paths
/// (Cargo.toml, package.json).
pub trait RehomeManifests: Sync + Send {
    /// The manifest carriers this language owns, project-relative, in path order.
    fn manifests(&self, cx: &MoveCx) -> Vec<String>;

    /// Manifest targets, as `ImportRef`s of kind `ManifestTarget`, so the one
    /// `respell` arm handles them too.
    fn manifest_refs(&self, cx: &MoveCx) -> Vec<ImportRef>;
}

/// The shim leg of a move: a reexport module left at the old path.
pub trait RehomeShim: Sync + Send {
    /// None when `old` cannot be read.
    fn shim(&self, cx: &MoveCx, old: &str, new: &str) -> Option<String>;
}

/// The text-refs leg of a move: spellings a build output wears beyond the
/// source path itself.
pub trait RehomeTextSpellings: Sync + Send {
    /// Extra `(old, new)` pairs for the `--text-refs` report to scan plain text for.
    fn text_spellings(&self, cx: &MoveCx, old: &str, new: &str) -> Vec<(String, String)>;
}

/// The plan-check leg of a move: reasons a batch cannot be planned at all.
pub trait RehomePlanCheck: Sync + Send {
    /// Any row stops the run before a stage is built; the core never sees a
    /// panic from an arm.
    fn plan_errors(&self, cx: &MoveCx) -> Vec<String>;
}

/// One `rehomes()` roster row: the core every language answers, and the legs
/// only some languages carry. A `None` leg is the language saying "no such
/// thing here", visible in the roster rather than hidden in a default method.
#[derive(Clone, Copy)]
pub struct RehomeArm {
    pub core: &'static dyn Rehome,
    pub manifests: Option<&'static dyn RehomeManifests>,
    pub shim: Option<&'static dyn RehomeShim>,
    pub text_spellings: Option<&'static dyn RehomeTextSpellings>,
    pub plan_check: Option<&'static dyn RehomePlanCheck>,
}

impl RehomeArm {
    pub fn name(&self) -> &'static str {
        self.core.name()
    }
}

/// Where one occurrence of a symbol sits. `span` covers EXACTLY the identifier
/// token: no quotes, no path prefix, no surrounding expression.
pub struct SymbolRef {
    /// Project-relative path of the file that writes the occurrence.
    pub file: String,
    pub span: Span,
    pub role: RefRole,
    /// The bytes at `span` as the arm read them. The core re-reads the tree and
    /// asserts equality before staging; a mismatch is a plan error, not a skip.
    pub text: String,
}

/// What one occurrence does with the symbol. One-for-one with SCIP's
/// `OccurrenceRole` (:1685), so the verify leg compares without a translation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RefRole {
    Definition,
    /// The imported name in `import {OLD}`.
    Import,
    /// The exported name in `export {OLD}` / `export {x as OLD}`.
    Export,
    Read,
    Write,
    /// A type-position mention; SCIP folds this into READ_ACCESS.
    TypeRef,
}

/// One occurrence a rename reports and never rewrites: where it sits, and the
/// form that reaches the symbol there.
#[derive(Debug)]
pub struct SymbolSeat {
    pub file: String,
    pub span: Span,
    /// One-based, resolved at construction against the file's line table.
    pub line: u32,
    /// The file this route reaches, when the seat is about a route. Empty otherwise.
    pub reaches: String,
    pub form: &'static str,
}

/// One site `rename` found and declined to plan. Sibling of `Unresolved` for
/// the rename verb: the plan stays complete for every site the arm typed.
#[derive(Debug)]
pub struct RenameAbstain {
    pub file: String,
    pub span: Span,
    /// The name under rename.
    pub symbol: String,
    /// `UnresolvedReason::as_str` vocabulary.
    pub reason: &'static str,
    /// Source text of the receiver expression.
    pub receiver: String,
}

/// Why an arm will not plan. A partial rename compiles less often than no
/// rename at all, so an arm stops instead of emitting a subset.
#[derive(Debug)]
pub enum RenameStop {
    /// `old` names more than one declaration in `anchor`; `at` disambiguates.
    Ambiguous {
        anchor: String,
        old: String,
        sites: Vec<Span>,
    },
    /// `old` names no declaration in `anchor`.
    NotFound { anchor: String, old: String },
    /// A reference the arm found but cannot span exactly.
    Inexact {
        file: String,
        span: Span,
        why: &'static str,
    },
    /// Every reference reachable only through a runtime form (computed member,
    /// dynamic import, string key). One seat at a time hides the next repair.
    Dynamic(Vec<SymbolSeat>),
}

impl fmt::Display for RenameStop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenameStop::Ambiguous { anchor, old, sites } => {
                let offsets: Vec<String> =
                    sites.iter().map(|site| site.start.to_string()).collect();
                write!(
                    formatter,
                    "{anchor} declares {old} more than once, at bytes {}",
                    offsets.join(", ")
                )
            }
            RenameStop::NotFound { anchor, old } => {
                write!(formatter, "{anchor} declares no {old}")
            }
            RenameStop::Inexact { file, span, why } => {
                write!(formatter, "{file} byte {}: {why}", span.start)
            }
            RenameStop::Dynamic(seats) => {
                let lines: Vec<String> = seats
                    .iter()
                    .map(|seat| match seat.reaches.is_empty() {
                        true => format!(
                            "{}:{}: {} reaches the symbol at runtime",
                            seat.file, seat.line, seat.form
                        ),
                        false => format!(
                            "{}:{}: {} reaches {} at runtime",
                            seat.file, seat.line, seat.form, seat.reaches
                        ),
                    })
                    .collect();
                formatter.write_str(&lines.join("\n"))
            }
        }
    }
}

impl std::error::Error for RenameStop {}

/// What one language answers when a symbol it owns is renamed. Sibling to
/// `Rehome`, held `&'static` in the `renames()` roster; no mutable state.
pub trait Rename: Source + Sync + Send {
    /// Every occurrence of `request`'s symbol this language owns, across
    /// `cx.files()`. One parse per file that can reach the anchor.
    fn symbol_refs(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
    ) -> Result<Vec<SymbolRef>, RenameStop>;

    /// `symbol_refs` plus every site the arm found and declined to plan. An arm
    /// that records no abstains keeps this default.
    fn symbol_refs_and_abstains(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
    ) -> Result<(Vec<SymbolRef>, Vec<RenameAbstain>), RenameStop> {
        self.symbol_refs(cx, request).map(|refs| (refs, Vec::new()))
    }

    /// The replacement bytes for one occurrence. None = unchanged (an aliased
    /// import `{OLD as local}` leaves `local` alone).
    fn respell_symbol(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
        reference: &SymbolRef,
    ) -> Option<Respell>;

    /// Spellings of the old name this language's corpus wears outside the scope
    /// plane, for the `--text-refs` report. NEVER rewritten.
    fn text_spellings(&self, _cx: &RenameCx, _request: &RenameRequest) -> Vec<(String, String)> {
        Vec::new()
    }
}
