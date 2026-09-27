//! `impl Rehome for KotlinSource`: every question `extract move` asks a
//! language, answered for Kotlin. Import headers, package facts, and relocation
//! spellings come from `hafley_scm::read::lang::kotlin_modules`.
//! @comment-ok: module header, the seam list every lang file opens with
//!
//! A Kotlin import is `package.Decl` and the `package` declaration is truth
//! (the directory is advisory), so a move changes imports only when the new
//! directory implies a new package under the SAME source root the old file sat
//! in. The source root is derived from the old path plus the declared package;
//! a layout that disagrees with the package is a named stop, never a guess
//! (v5 `src/ktpath.rs:24-50`).
//!
//! Four trait methods stay at their defaults, by decision rather than omission:
//! `manifests`/`manifest_refs` (a Gradle or Maven build file names source ROOTS
//! and never one `.kt` file, so no move can invalidate a target row),
//! `shim` (`--shim` answers `kotlin has no shim form` at `0_move.rs:129`; a
//! `typealias` left at the old path only forwards types, not functions or
//! properties, so it is NOT a shim) and `text_spellings` (a `.class` under
//! `build/` is a build's spelling, not one the corpus carries).
//!
//! The `warn` and `error` lines this arm prints lead the plan table rather than
//! following it: they are read off during `Plan::build`, which runs before
//! `0_move.rs:64` prints `root`.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;

use crate::edit_seams::ImportRef;
use crate::edit_seams::ImportRefKind;
use crate::edit_seams::Rehome;
use crate::edit_seams::Respell;
use crate::lang::KotlinSource;
use crate::move_cx::{owned_by, MoveCx};
use crate::project::extract_pool;
use crate::types::LangKind;
use hafley_scm::read::lang::kotlin_modules::{
    kt_move_facts, kt_move_package, kt_move_plan, kt_rewrite_import, kt_rewrite_import_path,
    KtMoveFacts, KtMovePlan,
};
use hafley_scm::span::Span;

/// The moved file's own `package a.b` declaration, a kind only Kotlin constructs.
pub const PACKAGE_DECL: ImportRefKind = ImportRefKind::Ext(LangKind {
    lang: "kotlin",
    tag: "package_decl",
});

impl Rehome for KotlinSource {
    fn import_refs(&self, cx: &MoveCx) -> Vec<ImportRef> {
        let (plans, stops) = plans(cx);
        for stop in &stops {
            println!("error {stop}");
        }
        if plans.is_empty() {
            return Vec::new();
        }
        let corpus = cx.files_of(self);
        let packages: Vec<&str> = plans.iter().map(|plan| plan.old_package.as_str()).collect();

        // Read and parse fan out; the merge below stays sequential over `corpus`
        // in path order, so the ref order is rel order.
        let scans: Vec<Option<FileScan>> = extract_pool().install(|| {
            corpus
                .par_iter()
                .map(|rel| {
                    let bytes = cx.read(rel)?;
                    if !carries_package(&bytes, &packages) {
                        return None;
                    }
                    if let Some(plan) = plans.iter().find(|plan| plan.old_rel == *rel) {
                        return Some(FileScan {
                            facts: plan.facts.clone(),
                            text: String::from_utf8(bytes).ok()?,
                        });
                    }
                    scan_file(rel, String::from_utf8(bytes).ok()?)
                })
                .collect()
        });

        let mut refs = Vec::new();
        for plan in &plans {
            refs.push(ImportRef {
                importer: plan.old_rel.clone(),
                literal: plan.package_span,
                text: plan.old_package.clone(),
                target: plan.old_rel.clone(),
                kind: PACKAGE_DECL,
            });
        }
        let mut wildcards: BTreeMap<&str, usize> = BTreeMap::new();
        let mut bare: BTreeMap<&str, usize> = BTreeMap::new();
        for (rel, scan) in corpus.iter().zip(&scans) {
            let Some(scan) = scan else { continue };
            for plan in &plans {
                if *rel != plan.old_rel
                    && scan
                        .facts
                        .package
                        .as_ref()
                        .map(|package| package.name.as_str())
                        == Some(plan.old_package.as_str())
                {
                    let count = bare_uses(&scan.text, &plan.top_level);
                    if count > 0 {
                        *bare.entry(plan.old_rel.as_str()).or_default() += count;
                    }
                }
                for row in &scan.facts.imports {
                    // A wildcard may still cover the moved decls and the package
                    // may hold other files: counted, never rewritten.
                    if row.wildcard {
                        if row.path == plan.old_package {
                            *wildcards.entry(plan.old_rel.as_str()).or_default() += 1;
                        }
                        continue;
                    }
                    if kt_rewrite_import(plan, &row.path).is_none() {
                        continue;
                    }
                    refs.push(ImportRef {
                        importer: rel.to_string(),
                        literal: Span {
                            start: row.span.start,
                            len: row.path.len() as u32,
                        },
                        text: row.path.clone(),
                        target: plan.old_rel.clone(),
                        kind: ImportRefKind::Import,
                    });
                }
            }
        }
        for plan in &plans {
            if let Some(count) = wildcards.get(plan.old_rel.as_str()) {
                println!(
                    "warn {}: {count} wildcard import(s) of {} left alone; the moved decls may need explicit imports of {}",
                    plan.old_rel, plan.old_package, plan.new_package
                );
            }
            if let Some(count) = bare.get(plan.old_rel.as_str()) {
                println!(
                    "warn {}: {count} same-package bare use(s) of a moved decl left alone; the file leaves {} for {}",
                    plan.old_rel, plan.old_package, plan.new_package
                );
            }
        }
        tracing::debug!(corpus = corpus.len(), refs = refs.len(), "move kotlin refs");
        refs
    }

    fn respell(&self, cx: &MoveCx, reference: &ImportRef) -> Option<Respell> {
        // Single-reference compatibility path. The move planner uses
        // `plan_respells` for the whole batch without reparsing here.
        let plan = plan_move(cx, &reference.target, cx.destination(&reference.target)?).ok()?;
        respell_with_plan(reference, &plan)
    }

    fn plan_respells(&self, cx: &MoveCx, extra_refs: &[ImportRef]) -> Vec<Respell> {
        let references = self.import_refs(cx);
        let packages: BTreeMap<String, (String, String)> = references
            .iter()
            .filter(|reference| reference.kind == PACKAGE_DECL)
            .filter_map(|reference| {
                let new = cx.destination(&reference.target)?;
                Some((
                    reference.target.clone(),
                    (
                        reference.text.clone(),
                        kt_move_package(&reference.target, new, &reference.text)?,
                    ),
                ))
            })
            .collect();
        let mut respells: Vec<Respell> = references
            .iter()
            .filter_map(|reference| {
                let (old_package, new_package) = packages.get(&reference.target)?;
                let text = match reference.kind {
                    PACKAGE_DECL => new_package.clone(),
                    ImportRefKind::Import => {
                        kt_rewrite_import_path(old_package, new_package, &reference.text)?
                    }
                    _ => return None,
                };
                (text != reference.text).then(|| Respell {
                    file: reference.importer.clone(),
                    span: reference.literal,
                    text,
                    receipt: None,
                })
            })
            .collect();
        respells.extend(
            extra_refs
                .iter()
                .filter_map(|reference| self.respell(cx, reference)),
        );
        respells
    }
}

fn respell_with_plan(reference: &ImportRef, plan: &KtMovePlan) -> Option<Respell> {
    let text = match reference.kind {
        PACKAGE_DECL => plan.new_package.clone(),
        ImportRefKind::Import => kt_rewrite_import(plan, &reference.text)?,
        _ => return None,
    };
    (text != reference.text).then(|| Respell {
        file: reference.importer.clone(),
        span: reference.literal,
        text,
        receipt: None,
    })
}

// ── the move plan ───────────────────────────────────────────────────────────

/// One moved Kotlin file resolved to the import rewrite it implies.
/// Every Kotlin move this run makes, and the named stop for each one whose
/// package cannot be derived. A stop rewrites nothing; the file still moves.
fn plans(cx: &MoveCx) -> (Vec<KtMovePlan>, Vec<String>) {
    let mut plans = Vec::new();
    let mut stops = Vec::new();
    for (old, new) in cx.moved() {
        if !owned_by(old, &KotlinSource) {
            continue;
        }
        match plan_move(cx, old, new) {
            Ok(plan) => plans.push(plan),
            Err(stop) => stops.push(stop),
        }
    }
    (plans, stops)
}

fn plan_move(cx: &MoveCx, old: &str, new: &str) -> Result<KtMovePlan, String> {
    let text = cx
        .text(old)
        .ok_or_else(|| format!("{old}: not readable as UTF-8 kotlin"))?;
    kt_move_plan(old, new, text.as_bytes())
}

// ── the owned Kotlin syntax rows ───────────────────────────────────────────

struct FileScan {
    facts: KtMoveFacts,
    text: String,
}

fn scan_file(path: &str, text: String) -> Option<FileScan> {
    let facts = kt_move_facts(path, text.as_bytes())?;
    Some(FileScan { facts, text })
}

// ── the corpus filter and the bare-use count ────────────────────────────────

/// Whether a file can name the batch at all. A superset filter: an importer
/// writes the old package, and so does a peer declaring itself in it.
fn carries_package(bytes: &[u8], packages: &[&str]) -> bool {
    packages
        .iter()
        .any(|package| memchr::memmem::find(bytes, package.as_bytes()).is_some())
}

/// How many of `decls` a same-package peer spells bare. A bare use breaks when
/// the file it names leaves the package, and it is not an import to rewrite.
fn bare_uses(text: &str, decls: &BTreeSet<String>) -> usize {
    decls.iter().filter(|decl| mentions(text, decl)).count()
}

fn mentions(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let needle = name.as_bytes();
    if needle.is_empty() {
        return false;
    }
    let mut at = 0;
    while let Some(hit) = memchr::memmem::find(&bytes[at..], needle) {
        let start = at + hit;
        let end = start + needle.len();
        let before = start == 0 || !is_word_byte(bytes[start - 1]);
        let after = end == bytes.len() || !is_word_byte(bytes[end]);
        if before && after {
            return true;
        }
        at = start + 1;
    }
    false
}

fn is_word_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric()
}
