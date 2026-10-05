//! `ryi cleave <SRC>#<ITEM> <DEST>`: one item leaves SRC and lands in DEST,
//! carrying the specifiers it needs and respelling every importer. The plan is
//! fact rows only; the `Cleave` roster spells the three edits they cannot.
//! @comment-ok: module header, the seam list every bin arm opens with

use super::cleave_fields::widen_private_fields;
#[path = "7c_cleave_root_items.rs"]
mod root_items;
#[path = "7g_cleave_empty_sources.rs"]
mod empty_sources;
#[path = "7b_cleave_ts_imports.rs"]
mod ts_imports;
use root_items::root_item_spans;
#[path = "7e_cleave_fact_source.rs"]
mod fact_source;
#[path = "7f_cleave_package_callers.rs"]
mod package;
#[path = "7d_cleave_scope_rows.rs"]
mod scope;
use crate::cli::CleaveArgs;
use fact_source::{CleaveFactSource, FastFacts, SlowTsFacts};
use package::package_callers;
use scope::scope_rows;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use sprefa_extract::edit_seams::CleaveDrag;
use sprefa_extract::edit_seams::CleavePlan;
use sprefa_extract::edit_seams::CleaveSpecifier;
use sprefa_extract::move_stage::{
    content_id, print_previews_with as print_previews, run_verify_command, stage_and_commit,
    state_root_for, Mirror, VerifyJournal,
};
use sprefa_extract::{
    cleave_for, directory_path, directory_source, dispatch, flatten_each, replace_action,
    resolve_project, scm_facts, Cleave, FamilyMask, FlatFact, MoveCx, ResolveArms, ResolveRequest,
    Respell, ScipMode, ScipRecords, Span,
};

const PRODUCER: &str = "extract-cleave";

/// Outside-the-corpus traits a method call needs imported (no prelude entry).
const IMPORTED_TRAITS: [&str; 16] = [
    "Read",
    "Write",
    "BufRead",
    "Seek",
    "FromStr",
    "Hash",
    "Hasher",
    "Error",
    "Borrow",
    "BorrowMut",
    "FromIterator",
    "Extend",
    "Any",
    "Itertools",
    "Future",
    "Stream",
];

const SCOPE: &str = "not supported: cross-language cleave, moving a type with its impl blocks";

pub fn run(cli: CleaveArgs, home: Option<&Path>) -> Result<(), crate::RyiExit> {
    if cli.verify.is_some() && !cli.commit {
        return Err("--verify needs --commit".to_string().into());
    }
    if let Some(list) = cli.list.as_deref() {
        return run_list(&cli, list, home);
    }
    let plan = Plan::build(&cli)?;
    let state = state_root_for(cli.state.as_deref(), home, &[plan.root.as_path()])?;

    crate::outln!("root {}", plan.root.display());
    crate::outln!(
        "plan {}#{} -> {}",
        plan.rows.src,
        plan.rows.item,
        plan.rows.dest
    );
    if !plan.rows.unresolved.is_empty() {
        for name in &plan.rows.unresolved {
            crate::outln!("ungraded {name}");
        }
        crate::outln!(
            "next: ryi graph --uses {} {}",
            plan.rows.unresolved[0],
            plan.root.display()
        );
        if !cli.drag {
            return Ok(());
        }
        return Err(crate::RyiExit::new(
            2,
            format!(
                "cleave --drag left ungraded names: {}",
                plan.rows.unresolved.join(", ")
            ),
        ));
    }
    for row in &plan.rows.travelling {
        crate::outln!(
            "travel {} from {} as {} ({})",
            row.name,
            row.module,
            row.dest_module,
            row.kind
        );
    }
    for row in &plan.rows.orphans {
        crate::outln!("orphan {} from {}", row.name, row.module);
    }
    for row in &plan.rows.dragged {
        crate::outln!("drag {} {} pass {}", row.name, row.action, row.iteration);
    }
    crate::outln!("drag fixpoint {} passes", plan.rows.drag_iterations);
    for caller in &plan.rows.callers {
        crate::outln!("caller {caller}");
    }

    let stages = plan.stages()?;
    let mirror = Mirror::build(&plan.root, &stages)?;
    let mut dry_previews = Vec::with_capacity(stages.len());
    for stage in &stages {
        dry_previews.push(stage_and_commit(
            mirror.root(),
            &state,
            stage,
            soopy::Durability::DryRun,
        )?);
    }
    plan.validate_preview(mirror.root())?;
    if cli.slow && plan.arm.name() == "ts" {
        let paths = plan.touched().into_iter().chain(plan.created()).collect();
        sprefa_extract::edit::ts7_cleave_diagnostics::check_preview(&plan.root, mirror.root(), &paths)?;
    }
    match cli.commit {
        true => {
            let journal =
                VerifyJournal::capture(&plan.root, &[], &plan.created(), &plan.touched())?;
            for stage in &stages {
                let (id, previews) =
                    stage_and_commit(&plan.root, &state, stage, soopy::Durability::Durable)?;
                print_previews(&previews, "", |line| crate::outln!("{line}"));
                crate::outln!("stage {id} committed");
            }
            verify_after_commit(&plan, &state, cli.verify.as_deref(), &journal)?;
        }
        false => {
            for (id, previews) in dry_previews {
                print_previews(&previews, "", |line| crate::outln!("{line}"));
                crate::outln!("stage {id} dry run, tree untouched");
            }
        }
    }
    if cli.text_refs {
        report_text_refs(&plan);
    }
    if cli.json {
        crate::outln!("{}", plan_json(&plan.rows));
    }
    Ok(())
}

/// `--list`: every row planned in order over ONE corpus walk and ONE resolve,
/// each row reading the texts the rows before it wrote, landed as ONE stage.
fn run_list(cli: &CleaveArgs, list: &Path, home: Option<&Path>) -> Result<(), crate::RyiExit> {
    let rows = read_cleave_list(list)?;
    let (first, _) = split_target(&rows[0].0)?;
    let root = plan_root(cli.root.as_ref(), &first)?;
    let state = state_root_for(cli.state.as_deref(), home, &[root.as_path()])?;
    let mut cx = MoveCx::open_with_untracked(&root, cli.root.is_some())?;
    let mut imports = Imports::read(&cx, &root)?;
    let mut sources = BTreeSet::new();
    let mut imported_before: BTreeMap<String, BTreeSet<(String, String)>> = BTreeMap::new();
    crate::outln!("root {}", root.display());
    for (target, dest) in &rows {
        let plan = Plan::build_with(cx, &mut imports, target, dest, cli.drag, cli.slow)?;
        print_plan(&plan);
        if !plan.rows.unresolved.is_empty() {
            if !cli.drag {
                return Ok(());
            }
            return Err(format!(
                "cleave --drag left ungraded names in {}#{}; the batch stops before any write",
                plan.rows.src, plan.rows.item
            )
            .into());
        }
        for row in &plan.source.specifiers {
            if plan.source.refs_outside(&row.name, &[]) > 0 {
                imported_before
                    .entry(plan.rows.src.clone())
                    .or_default()
                    .insert((module_key(&row.name, &row.module), row.name.clone()));
            }
        }
        sources.insert(plan.rows.src.clone());
        let edits = plan.land()?;
        imports.land(&plan, &edits);
        cx = plan.cx;
        for (rel, (text, _)) in edits {
            cx.overlay(&rel, text);
        }
    }
    drop_batch_unused_imports(&mut cx, &imports, &imported_before)?;
    let deleted = empty_sources::remove(&mut cx, &sources)?;
    let _ = std::fs::remove_dir_all(overlay_scratch());
    for (rel, text) in cx.overlaid() {
        if rel.ends_with(".rs") {
            hafley_scm::lang::rust::parse_rust_file(text)
                .map_err(|error| format!("cleave batch leaves invalid Rust in {rel}: {error}"))?;
        }
    }
    let (stages, touched, created) = batch_stages(&cx, &deleted)?;
    match cli.commit {
        true => {
            let journal = VerifyJournal::capture(&root, &[], &created, &touched)?;
            for stage in &stages {
                let (id, previews) =
                    stage_and_commit(&root, &state, stage, soopy::Durability::Durable)?;
                print_previews(&previews, "", |line| crate::outln!("{line}"));
                crate::outln!("stage {id} committed");
            }
            if let Some(command) = cli.verify.as_deref() {
                match run_verify_command(&root, command)? {
                    Some(0) => crate::outln!("verify ok"),
                    code => {
                        let reason =
                            code.map_or_else(|| "timeout".to_string(), |rc| rc.to_string());
                        let count = journal.restore(&root, &state, &[])?;
                        crate::outln!("verify failed (rc={reason}): rolled back {count} files");
                        return Err(crate::RyiExit::new(3, String::new()));
                    }
                }
            }
        }
        false => {
            let mirror = Mirror::build(&root, &stages)?;
            for stage in &stages {
                let (id, previews) =
                    stage_and_commit(mirror.root(), &state, stage, soopy::Durability::DryRun)?;
                print_previews(&previews, "", |line| crate::outln!("{line}"));
                crate::outln!("stage {id} dry run, tree untouched");
            }
        }
    }
    Ok(())
}

/// Revisit only imports the batch's source files used before a row landed.
/// Earlier rows can keep an import that a later row makes unused.
fn drop_batch_unused_imports(
    cx: &mut MoveCx,
    imports: &Imports,
    imported_before: &BTreeMap<String, BTreeSet<(String, String)>>,
) -> Result<(), String> {
    for (rel, candidates) in imported_before {
        let Some(arm) = cleave_for(rel) else { continue };
        if arm.imports_visible_to_children(cx, rel) {
            continue;
        }
        let facts = FileFacts::open(cx, rel, true)?;
        let mut text = facts.text.clone();
        for (module, names) in facts.modules() {
            let kept: Vec<String> = names
                .iter()
                .filter(|name| {
                    !candidates.contains(&(module.clone(), (*name).clone()))
                        || facts.refs_outside(name, &[]) > 0
                        || facts.method_scope(name, imports.maybe_trait(cx, rel, name), &[])
                        || facts.specifiers.iter().any(|row| {
                            row.name == **name && module_key(&row.name, &row.module) == module && facts.reexports(row.span)
                        })
                })
                .cloned()
                .collect();
            if kept.len() == names.len() {
                continue;
            }
            if let Some(edit) = arm.edit_import(&text, &kept, &module) {
                let candidate = apply(&text, &edit);
                // Import facts were read before this file's preceding batch
                // edits. Keep a valid overlay when a stale grouping proposes
                // a rewrite that no longer forms a Rust use item.
                if !rel.ends_with(".rs") || hafley_scm::lang::rust::parse_rust_file(&candidate).is_ok() {
                    text = candidate;
                }
            }
        }
        if text != facts.text {
            cx.overlay(rel, text)
        }
    }
    Ok(())
}

/// Every file the batch rewrote as one whole-text Replace, every file it made
/// as one Create; then the touched and created paths for a verify rollback.
#[allow(clippy::type_complexity)]
fn batch_stages(
    cx: &MoveCx,
    deleted: &BTreeSet<String>,
) -> Result<(Vec<Vec<soopy::SourceAction>>, Vec<String>, Vec<String>), String> {
    let root = cx.root();
    let identity = soopy::SourceRoot::open_directory(root)
        .map_err(|error| format!("open root {}: {error}", root.display()))?
        .directory()
        .identity
        .clone();
    let producer = soopy::ActionProducer::unordered(PRODUCER);
    let (mut replaced, mut made) = (Vec::new(), Vec::new());
    let (mut touched, mut created) = (Vec::new(), Vec::new());
    for (rel, text) in cx.overlaid() {
        match std::fs::read(cx.abs(rel)) {
            Ok(bytes) => {
                if bytes == text.as_bytes() {
                    continue;
                }
                let source = directory_source(&identity, rel);
                if deleted.contains(rel) {
                    replaced.push(soopy::SourceAction::Delete { source, expected: content_id(root, rel)? });
                    touched.push(rel.clone());
                    continue;
                }
                let edit = soopy::TextEdit {
                    range: soopy::ActionSpan {
                        source: source.clone(),
                        start: 0,
                        end: bytes.len() as u64,
                    },
                    replacement: text.clone().into_bytes(),
                    producer: producer.clone(),
                };
                replaced.push(replace_action(source, content_id(root, rel)?, vec![edit]));
                touched.push(rel.clone());
            }
            Err(_) => {
                made.push(soopy::SourceAction::Create {
                    path: directory_path(rel),
                    bytes: text.clone().into_bytes(),
                });
                created.push(rel.clone());
            }
        }
    }
    replaced.extend(made);
    let stages = if replaced.is_empty() { Vec::new() } else { vec![replaced] };
    Ok((stages, touched, created))
}

fn read_cleave_list(path: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let text = std::fs::read_to_string(sprefa_extract::io_path(path))
        .map_err(|error| format!("read cleave list {}: {error}", path.display()))?;
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((target, dest)) = line.split_once('\t') else {
            return Err(format!(
                "{}:{}: a cleave list row is `SRC#ITEM<TAB>DEST`",
                path.display(),
                index + 1
            ));
        };
        rows.push((target.trim().to_string(), PathBuf::from(dest.trim())));
    }
    match rows.is_empty() {
        true => Err(format!("{} holds no cleave rows", path.display())),
        false => Ok(rows),
    }
}

/// Files whose own specifier rows import `item` by SRC's module path, which a
/// resolve can miss: a test crate's package spelling, or a nested `use` beside a re-export.

/// The `use` statement holding `offset`: where it starts (its visibility
/// included) and how deep its line is indented; nested iff indented.
fn use_statement(text: &str, offset: u32) -> Option<(usize, usize)> {
    let before = &text[..offset as usize];
    let keyword = before.rfind("use ")?;
    let line_start = before[..keyword]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let indent =
        text[line_start..].len() - text[line_start..].trim_start_matches([' ', '\t']).len();
    Some((line_start + indent, indent))
}

/// Every `pub use SRC::*;` another file writes, answered with a `pub use
/// DEST::ITEM;` line after it, so paths through that glob keep resolving.
fn glob_reexports(
    cx: &MoveCx,
    arm: &dyn Cleave,
    src: &str,
    dest: &str,
    item: &str,
) -> Vec<(String, Span, String)> {
    let forms = |spelled: &str| {
        [
            format!("pub use {spelled}::*;"),
            format!("pub use {}::*;", spelled.trim_start_matches("crate::")),
        ]
    };
    let mut out = Vec::new();
    for rel in cx.files() {
        if rel == src || cleave_for(rel).map(|other| other.name()) != Some(arm.name()) {
            continue;
        }
        let Some(text) = cx.text(rel) else {
            continue;
        };
        if !text.contains("::*;") {
            continue;
        }
        let (Some(dest_spelled), Some(src_spelled)) =
            (arm.spell_module(cx, rel, dest), arm.spell_module(cx, rel, src))
        else {
            continue;
        };
        if forms(&dest_spelled)
            .iter()
            .any(|form| text.contains(form.as_str()))
        {
            continue;
        }
        let Some(at) = forms(&src_spelled)
            .iter()
            .find_map(|form| text.find(form.as_str()))
        else {
            continue;
        };
        let line_end = text[at..]
            .find('\n')
            .map_or(text.len(), |newline| at + newline + 1);
        out.push((
            rel.clone(),
            Span::anchor(line_end as u32),
            format!("pub use {dest_spelled}::{item};\n"),
        ));
    }
    out
}

/// Serde attributes name functions and modules inside strings
/// (`#[serde(with = "arc_str")]`); each path's head is a free name there.
fn serde_paths(text: &str) -> Vec<(String, Span)> {
    let Ok(file) = hafley_scm::lang::rust::parse_rust_file(text) else {
        return Vec::new();
    };
    let mut scan = SerdePathScan {
        text,
        paths: std::array::from_fn(|_| Vec::new()),
    };
    syn::visit::Visit::visit_file(&mut scan, &file);
    scan.paths.into_iter().flatten().collect()
}

struct SerdePathScan<'a> {
    // The order matches the former key-by-key scan.
    paths: [Vec<(String, Span)>; 4],
    text: &'a str,
}

impl<'ast> syn::visit::Visit<'ast> for SerdePathScan<'_> {
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if attribute.path().is_ident("serde") {
            let _ = attribute.parse_nested_meta(|meta| {
                let index = ["with", "serialize_with", "deserialize_with", "default"]
                    .iter()
                    .position(|key| meta.path.is_ident(key));
                if let Some(index) = index {
                    let literal: syn::LitStr = meta.value()?.parse()?;
                    let head: String = literal
                        .value()
                        .chars()
                        .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                        .collect();
                    if !head.is_empty() {
                        let range = literal.span().byte_range();
                        let token = self.text.get(range.clone()).unwrap_or_default();
                        if let Some(quote) = token.find('"').filter(|quote| *quote == 0) {
                            let start = range.start + quote + 1;
                            if self.text.get(start..start + head.len()) == Some(head.as_str()) {
                                self.paths[index].push((
                                    head.clone(),
                                    span_of(start as u32, (start + head.len()) as u32),
                                ));
                            }
                        }
                    }
                } else if meta.input.peek(syn::Token![=]) {
                    let _: syn::Expr = meta.value()?.parse()?;
                } else if meta.input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let _: proc_macro2::TokenStream = content.parse()?;
                }
                Ok(())
            });
        }
        syn::visit::visit_attribute(self, attribute);
    }
}

/// A top-level `mod name { .. }` block, whole lines: scope rows carry no
/// declaration for an inline module, and a serde path can name one.
fn inline_mod(text: &str, name: &str) -> Option<Decl> {
    let (start, exported) = ["\npub mod ", "\npub(crate) mod ", "\nmod "]
        .iter()
        .find_map(|head| {
            let at = text.find(&format!("{head}{name} {{"))? + 1;
            Some((at, head.contains("pub")))
        })?;
    let open = start + text[start..].find('{')?;
    let mut depth = 0usize;
    for (offset, ch) in text[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = open + offset + 1;
                    let end = text[end..]
                        .find('\n')
                        .map_or(text.len(), |newline| end + newline + 1);
                    return Some(Decl {
                        name: name.to_string(),
                        span: span_of(start as u32, end as u32),
                        exported,
                        type_only: false,
                    });
                }
            }
            _ => {}
        }
    }
    None
}

/// The single form's two positionals.
fn single(cli: &CleaveArgs) -> Result<(&str, &Path), String> {
    match (cli.target.as_deref(), cli.dest.as_deref()) {
        (Some(target), Some(dest)) => Ok((target, dest)),
        _ => Err("ryi cleave takes SRC#ITEM DEST, or --list TSV".to_string()),
    }
}

/// Where a batch writes overlaid texts for readers that only take paths.
fn overlay_scratch() -> PathBuf {
    std::env::temp_dir().join(format!("ryi-cleave-overlay-{}", std::process::id()))
}

/// The plan rows a batch prints per row before its one stage.
fn print_plan(plan: &Plan) {
    crate::outln!(
        "plan {}#{} -> {}",
        plan.rows.src,
        plan.rows.item,
        plan.rows.dest
    );
    for name in &plan.rows.unresolved {
        crate::outln!("ungraded {name}");
    }
    for row in &plan.rows.travelling {
        crate::outln!(
            "travel {} from {} as {} ({})",
            row.name,
            row.module,
            row.dest_module,
            row.kind
        );
    }
    for row in &plan.rows.orphans {
        crate::outln!("orphan {} from {}", row.name, row.module);
    }
    for row in &plan.rows.dragged {
        crate::outln!("drag {} {} pass {}", row.name, row.action, row.iteration);
    }
    for caller in &plan.rows.callers {
        crate::outln!("caller {caller}");
    }
}

/// Keep-if-pass: a non-zero or timed-out checker walks every touched path back
/// to its pre-run bytes, deletes the DEST this run created, and exits 3.
fn verify_after_commit(
    plan: &Plan,
    state: &Path,
    command: Option<&str>,
    journal: &VerifyJournal,
) -> Result<(), crate::RyiExit> {
    let Some(command) = command else {
        return Ok(());
    };
    match run_verify_command(&plan.root, command)? {
        Some(0) => crate::outln!("verify ok"),
        code => {
            let reason = code.map_or_else(|| "timeout".to_string(), |rc| rc.to_string());
            let count = journal.restore(&plan.root, state, &[])?;
            crate::outln!("verify failed (rc={reason}): rolled back {count} files");
            return Err(crate::RyiExit::new(3, String::new()));
        }
    }
    Ok(())
}

/// Lines naming the item in files no arm owns and no plan edit covers.
/// Rewriting text carriers is out of scope for `move` and for this verb.
fn report_text_refs(plan: &Plan) {
    let edited: BTreeSet<String> = plan.touched().into_iter().collect();
    let mut ignore = ignore::gitignore::GitignoreBuilder::new(&plan.root);
    let _ = ignore.add(plan.root.join(".ryiignore"));
    let ignore = ignore.build().ok();
    for rel in plan.cx.files() {
        if cleave_for(rel).is_some()
            || edited.contains(rel)
            || ignore.as_ref().is_some_and(|rules| {
                rules
                    .matched_path_or_any_parents(plan.cx.abs(rel), false)
                    .is_ignore()
            })
        {
            continue;
        }
        let Some(text) = plan.cx.text(rel) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if line.contains(&plan.rows.item) {
                crate::outln!(
                    "text-ref {rel}:{} {} -> {}",
                    index + 1,
                    plan.rows.src,
                    plan.rows.dest
                );
            }
        }
    }
}

fn plan_json(rows: &CleavePlan) -> String {
    let specifier = |row: &CleaveSpecifier| {
        serde_json::json!({
            "name": row.name,
            "module": row.module,
            "dest_module": row.dest_module,
            "span": { "start": row.span.start, "len": row.span.len },
            "kind": row.kind,
        })
    };
    serde_json::json!({
        "record": "cleave_plan",
        "src": rows.src,
        "dest": rows.dest,
        "item": rows.item,
        "item_span": { "start": rows.item_span.start, "len": rows.item_span.len },
        "travelling": rows.travelling.iter().map(specifier).collect::<Vec<_>>(),
        "orphans": rows.orphans.iter().map(specifier).collect::<Vec<_>>(),
        "callers": rows.callers,
        "dragged": rows.dragged.iter().map(|row| serde_json::json!({
            "name": row.name,
            "span": { "start": row.span.start, "len": row.span.len },
            "iteration": row.iteration,
            "action": row.action,
        })).collect::<Vec<_>>(),
        "drag_iterations": rows.drag_iterations,
        "unresolved": rows.unresolved,
    })
    .to_string()
}

/// One cleave, planned whole before a byte moves.
struct Plan {
    root: PathBuf,
    cx: MoveCx,
    arm: &'static dyn Cleave,
    rows: CleavePlan,
    source: FileFacts,
    cfg_prefix: String,
    /// The file whose `#[path]` decl includes SRC; a new DEST is declared beside it.
    declarer: Option<String>,
    /// None when DEST does not exist yet and this run creates it.
    dest_facts: Option<FileFacts>,
    /// The item's text and each moved helper's, in SRC byte order.
    moving_text: Vec<String>,
    /// The names DEST must bind per module, in SRC order: the travelling
    /// specifiers it does not already carry, then the exported helpers.
    dest_imports: Vec<(String, Vec<String>)>,
    callers: Vec<FileFacts>,
    /// Each caller's module spelling for SRC, beside its own facts.
    caller_modules: Vec<String>,
    /// A star barrel still reads the moved export through SRC.
    keep_source_export: bool,
    /// Call sites naming the item through a module path, as (file, span).
    qualified: Vec<(String, Span)>,
    /// `pub use DEST::ITEM;` lines landing after each `pub use SRC::*;`.
    reexports: Vec<(String, Span, String)>,
    /// DEST's module spelling from SRC, each caller and each qualified site's file.
    dest_spellings: BTreeMap<String, String>,
}

impl Plan {
    fn validate_preview(&self, root: &Path) -> Result<(), String> {
        if !self.rows.src.ends_with(".rs") {
            return Ok(());
        }
        let paths: BTreeSet<String> = self.touched().into_iter().chain(self.created()).collect();
        for rel in paths {
            let path = root.join(&rel);
            if !path.exists() {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("read cleave preview {rel}: {error}"))?;
            hafley_scm::lang::rust::parse_rust_file(&text).map_err(|error| {
                let at = error.span().start();
                let line = text.lines().nth(at.line.saturating_sub(1)).unwrap_or("");
                format!(
                    "cleave preview has invalid Rust in {rel}:{}:{}: {error}; line: {line}",
                    at.line,
                    at.column + 1
                )
            })?;
        }
        Ok(())
    }

    fn build(cli: &CleaveArgs) -> Result<Self, String> {
        let (target, dest) = single(cli)?;
        let (src, _) = split_target(target)?;
        let root = plan_root(cli.root.as_ref(), &src)?;
        let cx = MoveCx::open_with_untracked(&root, cli.root.is_some())?;
        let mut imports = Imports::read(&cx, &root)?;
        Self::build_with(cx, &mut imports, target, dest, cli.drag, cli.slow)
    }

    /// One row planned over a corpus walk and a resolve another row may share.
    fn build_with(
        mut cx: MoveCx,
        imports: &mut Imports,
        target: &str,
        dest: &Path,
        drag: bool,
        slow: bool,
    ) -> Result<Self, String> {
        let root = cx.root().to_path_buf();
        let (src, item) = split_target(target)?;
        let src = super::source_move::within_root(&root, &anchor_file_in(&cx, &src)?)?;
        let dest = super::source_move::within_root(
            &root,
            &super::source_move::canonical_unborn(&super::source_move::absolute(dest)?),
        )?;
        if !cx.contains(&src) {
            return Err(format!("cleave source is outside the corpus: {src}"));
        }
        if src == dest {
            return Err(format!("{src} is both the source and the destination"));
        }
        let arm = cleave_for(&src).ok_or_else(|| out_of_scope(&src))?;
        let landing = cleave_for(&dest).ok_or_else(|| out_of_scope(&dest))?;
        if arm.name() != landing.name() {
            return Err(format!(
                "{src} -> {dest} crosses languages; cross-language cleave is out of scope"
            ));
        }
        let declarer = match imports.rust_routes.declaring_files(&src) {
            [] => None,
            [one] => Some(one.clone()),
            many => {
                return Err(format!(
                    "{src} is included by #[path] from {} files ({}); a new file beside it needs one declaring module",
                    many.len(),
                    many.join(", ")
                ))
            }
        };
        if arm.name() == "rust" {
            if !cx.contains(&dest) {
                if let Some((parent, edit)) = arm.declare_new_file(&cx, &src, &dest, declarer.as_deref()) {
                    let text = apply(&cx.text(&parent).unwrap_or_default(), &edit);
                    cx.plan_text(&parent, text);
                }
                cx.plan_text(&dest, String::new());
            }
            if sprefa_extract::edit::rust_module_tree::places(&cx, &src)?.is_empty() {
                return Err(format!(
                    "cleave source {src} is in no module of the Cargo workspace rust-analyzer loaded from {}",
                    sprefa_extract::edit::rust_module_tree::searched_manifest(&cx, &src)?.display()
                ));
            }
            if sprefa_extract::edit::rust_module_tree::places(&cx, &dest)
                .map_err(|reason| format!("cleave destination {dest} is not declared by a Rust module: {reason}"))?
                .is_empty() {
                return Err(format!(
                    "cleave destination {dest} is not declared by a Rust module; declare it or choose a declared module path"
                ));
            }
        }

        let mut source = FileFacts::open(&cx, &src, true)?;
        let facts: &dyn CleaveFactSource = if slow && arm.name() == "ts" {
            &SlowTsFacts
        } else {
            &FastFacts
        };
        facts.load(&cx, &src, &item, &mut source, imports)?;
        if let Some(row) = source
            .unsupported_macros
            .iter()
            .find(|row| row.name == item)
        {
            return Err(format!(
                "{src} item {item} is generated by unsupported {}! macro",
                row.macro_name
            ));
        }
        let item_decl = source
            .decls
            .iter()
            .find(|decl| decl.name == item)
            .ok_or_else(|| format!("{src} declares no {item}"))?
            .clone();
        let cfg_prefix = item_cfg_prefix(&src, &source.text, &item_decl)?;

        let (mut dragged, drag_iterations) = source.drag_fixpoint(&item_decl, drag);
        if arm.name() == "ts" {
            for row in dragged.iter().filter(|row| row.action == "exported") {
                if source.mutable_bindings.iter().any(|span| inside(*span, row.span)) {
                    return Err(format!(
                        "cleave refuses mutable binding {} left in {src}; moving {item} would import that binding into {dest}",
                        row.name
                    ));
                }
            }
        }
        let travelling_types: BTreeSet<String> = std::iter::once(item.clone())
            .chain(
                dragged
                    .iter()
                    .filter(|row| row.action == "moved")
                    .map(|row| row.name.clone()),
            )
            .collect();
        for (self_ty, span) in &source.impls {
            if travelling_types.contains(self_ty) && !inside(*span, item_decl.span) {
                dragged.push(CleaveDrag {
                    name: format!("impl {self_ty}"),
                    span: *span,
                    iteration: drag_iterations,
                    action: "moved",
                });
            }
        }
        let mut moving: Vec<Span> = vec![item_decl.span];
        moving.extend(
            dragged
                .iter()
                .filter(|row| row.action == "moved")
                .map(|row| row.span),
        );

        let dest_facts = match cx.contains(&dest) {
            true => Some(FileFacts::open(&cx, &dest, true)?),
            false => None,
        };
        let dest_bound: BTreeSet<&str> = dest_facts
            .iter()
            .flat_map(|facts| facts.decls.iter())
            .map(|decl| decl.name.as_str())
            .collect();
        // Slow TS asks tsc (the planned edits' diagnostics); everything else checks here.
        if let Some(facts) = dest_facts.as_ref().filter(|_| !(slow && arm.name() == "ts")) {
            let arriving = std::iter::once(item.as_str()).chain(
                dragged
                    .iter()
                    .filter(|row| row.action == "moved" && !row.name.starts_with("impl "))
                    .map(|row| row.name.as_str()),
            );
            for name in arriving {
                let declared = dest_bound.contains(name);
                let imported_elsewhere = facts.specifiers.iter().any(|row| {
                    row.name == name && imports.target(&dest, &row.name) != Some(src.as_str())
                });
                if declared || imported_elsewhere {
                    return Err(format!(
                        "{dest} already {} {name}; moving {src}#{item} there would declare it twice",
                        if declared { "declares" } else { "imports" }
                    ));
                }
            }
        }
        let carried: BTreeSet<(String, String)> = dest_facts
            .iter()
            .flat_map(|facts| facts.specifiers.iter())
            .map(|row| (row.name.clone(), module_key(&row.name, &row.module)))
            .collect();

        let mut travelling = Vec::new();
        let mut orphans = Vec::new();
        let mut glob_unresolved = BTreeSet::new();
        for row in source
            .specifiers
            .iter()
            .filter(|row| !row.glob && row.kind != "reexport")
        {
            let dest_module = match imports.target(&src, &row.name) {
                Some(target) => {
                    let asked = row.module.rsplit("::").next().unwrap_or(&row.name);
                    let written = arm
                        .respell_relative(&cx, &src, &dest, &row.module)
                        .unwrap_or_else(|| row.module.clone());
                    if arm.name() != "rust"
                        || (cx.contains(&dest)
                            && imports.route_reaches(&dest, &written, &row.name, target))
                    {
                        module_key(asked, &written)
                    } else {
                        spell(arm, &cx, &dest, target)?
                    }
                }
                None => arm
                    .respell_relative(&cx, &src, &dest, &row.module)
                    .unwrap_or_else(|| row.module.clone()),
            };
            let kind = match (
                carried.contains(&(row.name.clone(), module_key(&row.name, &dest_module))),
                imports.target(&src, &row.name).is_some(),
            ) {
                (true, _) => "carried",
                (false, true) => "relative",
                (false, false) => "package",
            };
            let plan_row = CleaveSpecifier {
                name: row.name.clone(),
                module: row.module.clone(),
                dest_module,
                span: row.span,
                kind,
            };
            let already_dest = imports.target(&src, &row.name) == Some(dest.as_str())
                || module_key(&row.name, &row.module) == spell(arm, &cx, &src, &dest)?;
            if source.refs_in(&row.name, &moving) > 0
                && !dest_bound.contains(row.name.as_str())
                && !already_dest
            {
                travelling.push(plan_row.clone());
            }
            if source.refs_in(&row.name, &moving) > 0
                && source.refs_outside(&row.name, &moving) == 0
                && !source.reexports(row.span)
                && !source.method_scope(
                    &row.name,
                    imports.maybe_trait(&cx, &src, &row.name),
                    &moving,
                )
            {
                orphans.push(plan_row);
            }
        }
        for glob in source.specifiers.iter().filter(|row| row.glob) {
            let line_start = source.text[..glob.span.start as usize]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            if source.text[line_start..].starts_with(char::is_whitespace) {
                continue;
            }
            let Some(parent) = glob_parent(&cx, &src, &glob.module) else {
                continue;
            };
            let provider = FileFacts::open(&cx, &parent, true)?;
            for row in provider.specifiers.iter().filter(|row| !row.glob) {
                if source.refs_in(&row.name, &moving) == 0
                    || travelling.iter().any(|held| held.name == row.name)
                    || dest_bound.contains(row.name.as_str())
                    || imports.target(&parent, &row.name) == Some(dest.as_str())
                {
                    continue;
                }
                let target = imports.target(&parent, &row.name);
                let dest_module = match target {
                    Some(path) => spell(arm, &cx, &dest, path)?,
                    None => row.module.clone(),
                };
                let kind = match (
                    carried.contains(&(row.name.clone(), module_key(&row.name, &dest_module))),
                    target.is_some(),
                ) {
                    (true, _) => "carried",
                    (false, true) => "relative",
                    (false, false) => "package",
                };
                travelling.push(CleaveSpecifier {
                    name: row.name.clone(),
                    module: row.module.clone(),
                    dest_module,
                    span: row.span,
                    kind,
                });
            }
            for decl in &provider.decls {
                if source.refs_in(&decl.name, &moving) == 0
                    || travelling.iter().any(|held| held.name == decl.name)
                {
                    continue;
                }
                if !decl.exported {
                    glob_unresolved.insert(format!("{} from {parent} is private", decl.name));
                    continue;
                }
                let dest_module = spell(arm, &cx, &dest, &parent)?;
                let kind = if carried
                    .contains(&(decl.name.clone(), module_key(&decl.name, &dest_module)))
                {
                    "carried"
                } else {
                    "relative"
                };
                travelling.push(CleaveSpecifier {
                    name: decl.name.clone(),
                    module: parent.clone(),
                    dest_module,
                    span: decl.span,
                    kind,
                });
            }
        }

        let carried_names: BTreeSet<&str> = travelling
            .iter()
            .map(|row| row.name.as_str())
            .chain(dragged.iter().map(|row| row.name.as_str()))
            .chain(dest_bound.iter().copied())
            .collect();
        let mut unresolved = source.ungraded(&moving, &carried_names);
        unresolved.extend(glob_unresolved);
        if !unresolved.is_empty() {
            return Ok(Plan {
                root,
                cx,
                arm,
                rows: CleavePlan {
                    src,
                    dest,
                    item,
                    item_span: item_decl.span,
                    drag_iterations,
                    unresolved,
                    ..CleavePlan::default()
                },
                source,
                cfg_prefix,
                declarer: None,
                dest_facts: None,
                moving_text: Vec::new(),
                dest_imports: Vec::new(),
                callers: Vec::new(),
                caller_modules: Vec::new(),
                keep_source_export: false,
                qualified: Vec::new(),
                reexports: Vec::new(),
                dest_spellings: BTreeMap::new(),
            });
        }

        let src_module = spell(arm, &cx, &dest, &src)?;
        let mut dest_imports: Vec<(String, Vec<String>)> = Vec::new();
        let wanted = travelling
            .iter()
            .filter(|row| row.kind != "carried")
            .map(|row| {
                // `use a::b as c` is the row (c, a::b): it lands as (a, "b as c").
                let written = row.module.rsplit("::").next().unwrap_or(&row.module);
                if written == row.name || !row.module.contains("::") {
                    return (row.name.clone(), row.dest_module.clone());
                }
                let parent = row
                    .dest_module
                    .strip_suffix(written)
                    .and_then(|head| head.strip_suffix("::"))
                    .unwrap_or(&row.dest_module);
                (format!("{written} as {}", row.name), parent.to_string())
            })
            .chain(
                dragged
                    .iter()
                    .filter(|row| row.action == "exported")
                    .filter(|row| {
                        !carried.contains(&(row.name.clone(), module_key(&row.name, &src_module)))
                    })
                    .map(|row| (row.name.clone(), src_module.clone())),
            );
        for (name, module) in wanted {
            match dest_imports.iter_mut().find(|(held, _)| *held == module) {
                Some((_, names)) => names.push(name),
                None => {
                    let mut names: Vec<String> = dest_facts
                        .iter()
                        .flat_map(|facts| facts.specifiers.iter())
                        .filter(|row| module_key(&row.name, &row.module) == module)
                        .map(|row| row.name.clone())
                        .collect();
                    names.push(name);
                    dest_imports.push((module, names));
                }
            }
        }

        let mut callers = imports.callers(&src, &item);
        let mut keep_source_export = false;
        if let (true, Some(resolver)) = (arm.name() == "ts", imports.ts_resolver.as_ref()) {
            for rel in &imports.barrels(&src) {
                if rel == &src || rel == &dest {
                    continue;
                }
                let facts = FileFacts::open(&cx, rel, false)?;
                // Named consumers are repointed below. A star barrel keeps
                // its module route and therefore still needs SRC's export.
                if let Some(modules) = sprefa_extract::lang::ts_resolve::module_facts(rel, facts.text.as_bytes()) {
                    keep_source_export |= modules.star_exports.iter().any(|module| {
                        resolver.resolve(&cx.abs(rel), module).is_some_and(|path| path == cx.abs(&src))
                    });
                }
                if facts.specifiers.iter().any(|row| {
                    row.kind == "reexport"
                        && row.imported.as_deref().unwrap_or(&row.name) == item
                        && resolver.resolve(&cx.abs(rel), &row.module).is_some_and(|path| path == cx.abs(&src))
                }) {
                    callers.push(rel.clone());
                }
            }
        }
        callers.extend(package_callers(&cx, arm, &src, &item, &callers)?);
        callers.sort();
        callers.dedup();
        let mut views = Vec::with_capacity(callers.len());
        let mut caller_modules = Vec::with_capacity(callers.len());
        for caller in &callers {
            let facts = FileFacts::open(&cx, caller, false)?;
            // The row spelling SRC itself, never one relayed through a re-export.
            let direct = spell(arm, &cx, caller, &src)?;
            caller_modules.push(
                facts
                    .specifiers
                    .iter()
                    .filter(|row| row.name == item || row.imported.as_deref() == Some(item.as_str()))
                    .find(|row| module_key(&item, &row.module) == direct)
                    .or_else(|| facts.specifiers.iter().find(|row| row.name == item || row.imported.as_deref() == Some(item.as_str())))
                    .map_or_else(String::new, |row| row.module.clone()),
            );
            views.push(facts);
        }
        moving.sort_by_key(|span| span.start);
        let mut moving_text: Vec<String> = moving
            .iter()
            .map(|span| source.slice(*span).to_string())
            .collect();
        let checker =
            slow || hafley_scm::read::lang::rust_checker::warm_workspace_available(cx.root(), hafley_scm::read::lang::rust_checker::Tier::Slow);
        widen_private_fields(
            &cx,
            &src,
            &dest,
            &source.text,
            &moving,
            &mut moving_text,
            checker,
        )?;
        if source.refs_outside(&item, &moving) > 0 || !callers.is_empty() {
            if let Some(first) = moving_text.first_mut() {
                for start in statement_starts(first, span_of(0, first.len() as u32), &item) {
                    if let Some(edit) = arm.edit_export(first, Span::anchor(start), true) {
                        *first = apply(first, &edit);
                    }
                }
            }
        }
        cross_package_stop(
            &cx,
            arm.name(),
            &src,
            &dest,
            &item,
            &callers,
            source.refs_outside(&item, &moving) > 0,
            &dest_imports,
            &travelling,
        )?;
        let qualified = imports.qualified(&cx, &src, &item);
        let reexports = glob_reexports(&cx, arm, &src, &dest, &item);
        let mut dest_spellings = BTreeMap::new();
        for rel in std::iter::once(&src).chain(&callers).chain(qualified.iter().map(|(rel, _)| rel)) {
            if !dest_spellings.contains_key(rel) {
                dest_spellings.insert(rel.clone(), spell(arm, &cx, rel, &dest)?);
            }
        }
        Ok(Plan {
            root,
            cx,
            arm,
            rows: CleavePlan {
                src,
                dest,
                item,
                item_span: item_decl.span,
                travelling,
                orphans,
                callers,
                dragged,
                drag_iterations,
                unresolved,
            },
            source,
            cfg_prefix,
            declarer,
            dest_facts,
            moving_text,
            dest_imports,
            callers: views,
            caller_modules,
            keep_source_export,
            qualified,
            reexports,
            dest_spellings,
        })
    }

    /// The texts this row leaves, per file, beside each edit as (old span, new
    /// length) so a shared resolve can re-aim its spans.
    #[allow(clippy::type_complexity)]
    fn land(&self) -> Result<BTreeMap<String, (String, Vec<(Span, u32)>)>, String> {
        let mut by_file: BTreeMap<String, Vec<Respell>> = BTreeMap::new();
        for respell in self.respells() {
            by_file
                .entry(respell.file.clone())
                .or_default()
                .push(respell);
        }
        let mut out = BTreeMap::new();
        for (rel, edits) in by_file {
            let text = self.cx.text(&rel).ok_or_else(|| format!("read {rel}"))?;
            let edits = Self::normalize_row_edits(&rel, edits)?;
            let shifts: Vec<(Span, u32)> = edits
                .iter()
                .map(|edit| (edit.span, edit.text.len() as u32))
                .collect();
            let mut rewritten = String::with_capacity(text.len());
            let mut cursor = 0;
            // All spans address the original text, including inserts at range boundaries.
            for edit in &edits {
                let start = edit.span.start as usize;
                let end = edit.span.end() as usize;
                rewritten.push_str(&text[cursor..start]);
                rewritten.push_str(&edit.text);
                cursor = end;
            }
            rewritten.push_str(&text[cursor..]);
            out.insert(rel, (rewritten, shifts));
        }
        if self.dest_facts.is_none() {
            let (_, block) = self.import_block("");
            let text = match block.is_empty() {
                true => self.moving_text.join("\n"),
                false => format!("{block}\n{}", self.moving_text.join("\n")),
            };
            out.insert(self.rows.dest.clone(), (text, Vec::new()));
        }
        Ok(out)
    }

    /// Match the stage planner's equivalent-edit normalization before a row
    /// becomes the next row's in-memory source text.
    fn normalize_row_edits(rel: &str, mut edits: Vec<Respell>) -> Result<Vec<Respell>, String> {
        edits.sort_by_key(|edit| (edit.span.start, edit.span.len));
        // Import pruning can propose different text for a shared use line;
        // retain the last projection before checking independent overlaps.
        edits.dedup_by(|later, earlier| {
            if later.span == earlier.span {
                earlier.text = later.text.clone();
                true
            } else {
                false
            }
        });
        if edits.windows(2).any(|pair| {
            pair[0].span.end() > pair[1].span.start || pair[0].span.start == pair[1].span.start
        }) {
            return Err(format!("{rel}: cleave row has overlapping edits"));
        }
        Ok(edits)
    }

    /// Every byte this cleave rewrites, as one `Respell` per span, in (file,
    /// offset) order. Nothing here touches the tree; the stages do.
    fn respells(&self) -> Vec<Respell> {
        let mut out = self.source_respells();
        out.extend(self.dest_respells());
        out.extend(self.caller_respells());
        for (file, span, text) in &self.reexports {
            out.push(Respell {
                receipt: Some(format!("re-export {file}: {}", text.trim())),
                file: file.clone(),
                span: *span,
                text: format!("{}{text}", self.cfg_prefix),
            });
        }
        if let Some((file, edit)) = self.new_file_decl() {
            out.push(Respell {
                receipt: Some(format!("declare {}: {}", self.rows.dest, edit.text.trim())),
                file,
                span: edit.span,
                text: edit.text,
            });
        }
        if let Some((file, edit)) = self.publish_decl() {
            out.push(Respell {
                receipt: Some(format!("publish {} for another crate", self.rows.dest)),
                file,
                span: edit.span,
                text: edit.text,
            });
        }
        for (rel, span) in &self.qualified {
            let Some(written) = self.cx.text(rel).and_then(|text| {
                text.get(span.start as usize..span.end() as usize)
                    .map(str::to_string)
            }) else {
                continue;
            };
            let module = written.rsplit_once("::").map_or("", |(module, _)| module);
            let spelling = as_written(
                &self.cx,
                &self.rows.dest,
                module,
                self.dest_spellings[&**rel].clone(),
            );
            out.push(Respell {
                receipt: Some(format!(
                    "path {rel}: {written} -> {spelling}::{}",
                    self.rows.item
                )),
                file: rel.clone(),
                span: *span,
                text: format!("{spelling}::{}", self.rows.item),
            });
        }
        out.sort_by(|left, right| {
            left.file
                .cmp(&right.file)
                .then(left.span.start.cmp(&right.span.start))
        });
        out
    }

    /// SRC loses the moving spans and every specifier nothing left references,
    /// and gains an export on each helper that stays behind for DEST.
    fn source_respells(&self) -> Vec<Respell> {
        let mut cuts: Vec<Span> = vec![self.rows.item_span];
        cuts.extend(
            self.rows
                .dragged
                .iter()
                .filter(|row| row.action == "moved")
                .map(|row| row.span),
        );
        let moving = cuts.clone();
        let orphaned: BTreeSet<&str> = if self
            .arm
            .imports_visible_to_children(&self.cx, &self.rows.src)
        {
            BTreeSet::new()
        } else {
            self.rows
                .orphans
                .iter()
                .map(|row| row.name.as_str())
                .collect()
        };
        let mut edits: Vec<Respell> = Vec::new();
        if self.arm.name() == "ts"
            && self.keep_source_export
            && self.source.decls.iter().any(|decl| decl.name == self.rows.item && decl.exported)
        {
            let module = self.dest_spellings[&self.rows.src].clone();
            let type_head = if self.source.decls.iter().any(|decl| decl.name == self.rows.item && decl.type_only) { " type" } else { "" };
            let (quote, semicolon, _) = sprefa_extract::edit::ts_mutate::import_style(&self.source.text);
            edits.push(Respell {
                file: self.rows.src.clone(),
                span: Span::anchor(self.rows.item_span.start),
                text: format!("export{type_head} {{ {} }} from {quote}{module}{quote}{}\n", self.rows.item, if semicolon { ";" } else { "" }),
                receipt: Some(format!("public API {} keeps {}", self.rows.src, self.rows.item)),
            });
        }
        for row in self
            .rows
            .dragged
            .iter()
            .filter(|row| row.action == "exported")
        {
            for start in statement_starts(&self.source.text, row.span, &row.name) {
                let Some(edit) = self.arm.edit_export(&self.source.text, Span::anchor(start), true)
                else {
                    continue;
                };
                edits.push(Respell {
                    file: self.rows.src.clone(),
                    span: edit.span,
                    text: edit.text,
                    receipt: Some(format!("export {} stays in {}", row.name, self.rows.src)),
                });
            }
        }
        for (module, names) in self.source.modules() {
            let kept: Vec<String> = names
                .iter()
                .filter(|name| !orphaned.contains(name.as_str()))
                .cloned()
                .collect();
            if kept.len() == names.len() {
                continue;
            }
            let Some(edit) = self.arm.edit_import(&self.source.text, &kept, &module) else {
                continue;
            };
            match edit.text.is_empty() {
                true => cuts.push(edit.span),
                false => edits.push(Respell {
                    file: self.rows.src.clone(),
                    span: edit.span,
                    text: edit.text,
                    receipt: None,
                }),
            }
        }
        if self.source.refs_outside(&self.rows.item, &moving) > 0 {
            let module = self.dest_spellings[&self.rows.src].clone();
            let mut names = self
                .source
                .modules()
                .into_iter()
                .find(|(written, _)| written == &module)
                .map_or_else(Vec::new, |(_, names)| names);
            if !names.contains(&self.rows.item) {
                names.push(self.rows.item.clone());
            }
            if let Some(edit) = self.arm.edit_import_item(
                &self.source.text,
                &names,
                &module,
                &self.rows.item,
                self.source
                    .decls
                    .iter()
                    .any(|decl| decl.name == self.rows.item && decl.type_only),
            ) {
                edits.push(Respell {
                    file: self.rows.src.clone(),
                    span: edit.span,
                    text: format!("{}{text}", self.cfg_prefix, text = edit.text),
                    receipt: Some(format!(
                        "source {} keeps {} via {module}",
                        self.rows.src, self.rows.item
                    )),
                });
            }
        }
        let mut out: Vec<Respell> = absorb(&self.source.text, cuts)
            .into_iter()
            .map(|span| Respell {
                file: self.rows.src.clone(),
                span,
                text: String::new(),
                receipt: None,
            })
            .collect();
        // An insert anchored inside a cut (the last `use` line was orphaned)
        // becomes that cut's replacement, so the two never overlap.
        for edit in edits {
            let host = out.iter_mut().find(|cut| {
                edit.span.len == 0
                    && cut.span.start <= edit.span.start
                    && edit.span.start < cut.span.end()
            });
            match host {
                Some(cut) => {
                    cut.text.push_str(&edit.text);
                    cut.receipt = edit.receipt.or(cut.receipt.take());
                }
                None => out.push(edit),
            }
        }
        out
    }

    /// DEST gains the imports it does not already carry, then the moving text.
    /// The import block is rewritten whole, so two new modules never anchor at
    /// one offset.
    fn dest_respells(&self) -> Vec<Respell> {
        let Some(facts) = self.dest_facts.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let (at, block) = self.import_block(&facts.text);
        if facts.text.is_empty() {
            let moving = self.moving_text.join("\n");
            let text = if block.is_empty() {
                moving
            } else {
                format!("{block}\n{moving}")
            };
            out.push(Respell {
                file: self.rows.dest.clone(),
                span: Span::anchor(0),
                text,
                receipt: None,
            });
            return out;
        }
        if block != facts.text[..at as usize] {
            out.push(Respell {
                file: self.rows.dest.clone(),
                span: Span { start: 0, len: at },
                text: block,
                receipt: None,
            });
        }
        out.push(Respell {
            file: self.rows.dest.clone(),
            span: Span::anchor(facts.text.len() as u32),
            text: format!("\n{}", self.moving_text.join("\n")),
            receipt: None,
        });
        out
    }

    /// DEST's import region after every wanted module lands in it, beside the
    /// length of the region it replaces.
    fn import_block(&self, text: &str) -> (u32, String) {
        let at = self
            .dest_facts
            .as_ref()
            .map_or(0, |facts| facts.import_region(text));
        let mut block = text[..at as usize].to_string();
        for (module, names) in &self.dest_imports {
            if self.arm.name() == "ts" {
                block = ts_imports::add(
                    &self.source,
                    self.dest_facts.as_ref(),
                    &block,
                    module,
                    names,
                );
            } else if let Some(edit) = self.arm.edit_import(&block, names, module) {
                block = apply(&block, &edit);
            }
        }
        // DEST that imported the item now declares it: that import goes.
        for (module, names) in self.dest_facts.iter().flat_map(|facts| facts.modules()) {
            if !names.contains(&self.rows.item) {
                continue;
            }
            let kept: Vec<String> = names
                .into_iter()
                .filter(|name| *name != self.rows.item)
                .collect();
            if let Some(edit) = self.arm.edit_import(&block, &kept, &module) {
                block = apply(&block, &edit);
            }
        }
        (at, block)
    }

    /// Every importer of `SRC#ITEM` re-aimed at DEST: the SRC import loses the
    /// item, and one naming DEST lands beside it.
    fn caller_respells(&self) -> Vec<Respell> {
        let mut out = Vec::new();
        let spellings = self.rows.callers.iter().zip(&self.callers);
        for ((rel, facts), module) in spellings.zip(&self.caller_modules) {
            if *rel == self.rows.dest {
                continue;
            }
            if self.arm.name() == "ts" {
                let spelling = as_written(
                    &self.cx,
                    &self.rows.dest,
                    module,
                    self.dest_spellings[&**rel].clone(),
                );
                out.extend(ts_imports::caller(
                    facts,
                    rel,
                    &self.rows.item,
                    module,
                    &spelling,
                    self.source
                        .decls
                        .iter()
                        .any(|decl| decl.name == self.rows.item && decl.type_only),
                ));
                continue;
            }
            let top_level = !rel.ends_with(".rs")
                || facts.specifiers.iter().any(|row| {
                    row.name == self.rows.item
                        && row.module == *module
                        && use_statement(&facts.text, row.span.start)
                            .is_some_and(|(_, indent)| indent == 0)
                });
            if !top_level {
                out.extend(self.nested_respells(rel, facts));
                continue;
            }
            let kept: Vec<String> = facts
                .specifiers
                .iter()
                .filter(|row| {
                    row.module == *module
                        && row.name != self.rows.item
                        && (rel.ends_with(".rs") || row.kind == "named")
                })
                .map(|row| row.name.clone())
                .collect();
            if let Some(edit) = self.arm.edit_import(&facts.text, &kept, module) {
                out.push(Respell {
                    file: rel.clone(),
                    span: edit.span,
                    text: edit.text,
                    receipt: Some(format!("caller {rel}: {module} loses {}", self.rows.item)),
                });
            }
            let spelling = as_written(
                &self.cx,
                &self.rows.dest,
                module,
                self.dest_spellings[&**rel].clone(),
            );
            let mut landing: Vec<String> = facts
                .specifiers
                .iter()
                .filter(|row| {
                    self.cfg_prefix.is_empty()
                        && module_key(&row.name, &row.module) == spelling
                        && (rel.ends_with(".rs") || (row.kind == "named" && !row.type_only))
                })
                .map(|row| row.name.clone())
                .collect();
            landing.push(self.rows.item.clone());
            let edit = if self
                .source
                .decls
                .iter()
                .any(|decl| decl.name == self.rows.item && decl.type_only)
            {
                self.arm
                    .edit_import_item(&facts.text, &landing, &spelling, &self.rows.item, true)
            } else {
                self.arm
                    .edit_import_like(&facts.text, &landing, &spelling, &self.rows.item, module)
            };
            if let Some(edit) = edit {
                out.push(Respell {
                    file: rel.clone(),
                    span: edit.span,
                    text: format!("{}{text}", self.cfg_prefix, text = edit.text),
                    receipt: Some(format!("caller {rel}: {} -> {spelling}", self.rows.item)),
                });
            }
            if rel.ends_with(".rs") {
                out.extend(self.nested_respells(rel, facts));
            }
        }
        out
    }

    /// An indented `use` naming the item (inside `mod tests { .. }` or a body)
    /// is respelled on its own statement text, then placed back at its offset.
    fn nested_respells(&self, rel: &str, facts: &FileFacts) -> Vec<Respell> {
        let item = &self.rows.item;
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for row in facts.specifiers.iter().filter(|row| row.name == *item) {
            let Some((at, indent_len)) = use_statement(&facts.text, row.span.start) else {
                continue;
            };
            let indent = &facts.text[at - indent_len..at];
            if indent_len == 0 || !seen.insert(at) {
                continue;
            }
            let end = facts.text[at..]
                .find(';')
                .map_or(facts.text.len(), |semi| at + semi + 1);
            let statement = format!("{}\n", &facts.text[at..end]);
            let within = |start: u32| at as u32 <= start && (start as usize) < end;
            let kept: Vec<String> = facts
                .specifiers
                .iter()
                .filter(|other| {
                    within(other.span.start) && other.module == row.module && other.name != *item
                })
                .map(|other| other.name.clone())
                .collect();
            let spelling = as_written(
                &self.cx,
                &self.rows.dest,
                &row.module,
                self.dest_spellings[rel].clone(),
            );
            let edits = [
                self.arm.edit_import(&statement, &kept, &row.module),
                self.arm.edit_import_like(
                    &statement,
                    std::slice::from_ref(item),
                    &spelling,
                    item,
                    &row.module,
                ),
            ];
            for (index, edit) in edits
                .into_iter()
                .enumerate()
                .filter_map(|(index, edit)| edit.map(|edit| (index, edit)))
            {
                let replacement = if index == 1 {
                    format!("{}{text}", self.cfg_prefix, text = edit.text)
                } else {
                    edit.text
                };
                let at_line_start = statement[..edit.span.start as usize].ends_with('\n');
                let text = match edit.span.len {
                    0 => replacement
                        .lines()
                        .enumerate()
                        .map(|(index, line)| match index == 0 && !at_line_start {
                            true => format!("{line}\n"),
                            false => format!("{indent}{line}\n"),
                        })
                        .collect(),
                    _ => replacement,
                };
                // A whole-statement drop takes its indentation along.
                let (start, len) = match edit.span.start == 0 && text.is_empty() {
                    true => (at - indent.len(), edit.span.len as usize + indent.len()),
                    false => (at + edit.span.start as usize, edit.span.len as usize),
                };
                out.push(Respell {
                    file: rel.to_string(),
                    span: Span {
                        start: start as u32,
                        len: len as u32,
                    },
                    text,
                    receipt: Some(format!("caller {rel}: nested use of {item} -> {spelling}")),
                });
            }
        }
        out
    }

    /// One soopy stage of Replace actions, plus a Create stage when DEST is new.
    fn stages(&self) -> Result<Vec<Vec<soopy::SourceAction>>, String> {
        if self.rows.src.ends_with(".rs") {
            let mut cx = self.cx.clone();
            for (rel, (text, _)) in self.land()? { cx.overlay(&rel, text); }
            let deleted = empty_sources::remove(&mut cx, &BTreeSet::from([self.rows.src.clone()]))?;
            if !deleted.is_empty() { return Ok(batch_stages(&cx, &deleted)?.0); }
        }
        let identity = soopy::SourceRoot::open_directory(&self.root)
            .map_err(|error| format!("open root {}: {error}", self.root.display()))?
            .directory()
            .identity
            .clone();
        let producer = soopy::ActionProducer::unordered(PRODUCER);
        let mut by_file: BTreeMap<String, Vec<soopy::TextEdit>> = BTreeMap::new();
        for respell in self.respells() {
            let source = directory_source(&identity, &respell.file);
            let start = respell.span.start as u64;
            by_file
                .entry(respell.file)
                .or_default()
                .push(soopy::TextEdit {
                    range: soopy::ActionSpan {
                        source,
                        start,
                        end: start + respell.span.len as u64,
                    },
                    replacement: respell.text.into_bytes(),
                    producer: producer.clone(),
                });
        }
        let mut edits: Vec<soopy::SourceAction> = Vec::new();
        for (rel, file_edits) in by_file {
            let source = directory_source(&identity, &rel);
            edits.push(replace_action(
                source,
                content_id(&self.root, &rel)?,
                file_edits,
            ));
        }
        let mut stages = Vec::new();
        if !edits.is_empty() {
            stages.push(edits);
        }
        if self.dest_facts.is_none() {
            let (_, block) = self.import_block("");
            stages.push(vec![soopy::SourceAction::Create {
                path: directory_path(&self.rows.dest),
                bytes: match block.is_empty() {
                    true => self.moving_text.join("\n"),
                    false => format!("{block}\n{}", self.moving_text.join("\n")),
                }
                .into_bytes(),
            }]);
        }
        Ok(stages)
    }

    /// The parent `mod` a DEST this cleave creates needs, when the arm has one.
    fn new_file_decl(&self) -> Option<(String, sprefa_extract::Edit)> {
        match self.dest_facts {
            Some(_) => None,
            None => self
                .arm
                .declare_new_file(&self.cx, &self.rows.src, &self.rows.dest, self.declarer.as_deref())
                .map(|(file, mut edit)| {
                    edit.text = format!("{}{text}", self.cfg_prefix, text = edit.text);
                    (file, edit)
                }),
        }
    }

    /// An existing DEST whose module another crate now names must be public.
    fn publish_decl(&self) -> Option<(String, sprefa_extract::Edit)> {
        self.dest_facts.as_ref()?;
        let foreign = self
            .rows
            .callers
            .iter()
            .zip(&self.caller_modules)
            .filter(|(rel, _)| **rel != self.rows.dest)
            .any(|(rel, module)| {
                let spelling = as_written(
                    &self.cx,
                    &self.rows.dest,
                    module,
                    self.dest_spellings[&**rel].clone(),
                );
                !matches!(
                    spelling.split("::").next(),
                    Some("crate" | "self" | "super")
                )
            });
        match foreign {
            true => self.arm.publish_module(&self.cx, &self.rows.dest),
            false => None,
        }
    }

    /// Every path a stage reads or writes, so a verify rollback can restore it.
    fn touched(&self) -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        out.insert(self.rows.src.clone());
        if let Some((file, _)) = self.new_file_decl().or_else(|| self.publish_decl()) {
            out.insert(file);
        }
        if self.dest_facts.is_some() {
            out.insert(self.rows.dest.clone());
        }
        out.extend(self.rows.callers.iter().cloned());
        out.extend(self.qualified.iter().map(|(rel, _)| rel.clone()));
        out.extend(self.reexports.iter().map(|(rel, _, _)| rel.clone()));
        out.into_iter().collect()
    }

    /// The paths this run creates. A rollback deletes them, as it does a shim.
    fn created(&self) -> Vec<String> {
        match self.dest_facts.is_none() {
            true => vec![self.rows.dest.clone()],
            false => Vec::new(),
        }
    }
}

/// One module spelling per binding: `use a::b::Name;` rows carry the whole
/// path as their module, `use a::b::{Name}` rows carry `a::b`.
fn module_key(name: &str, module: &str) -> String {
    module
        .strip_suffix(name)
        .and_then(|head| head.strip_suffix("::"))
        .unwrap_or(module)
        .to_string()
}

/// A caller that named SRC through its package's crate ident (a bin reaching
/// its own lib) keeps that ident where the arm spelled DEST with `crate::`.
fn as_written(cx: &MoveCx, dest: &str, module: &str, spelling: String) -> String {
    let head = module.split("::").next().unwrap_or_default();
    let named = sprefa_extract::edit::rust_rehome::cargo_package(cx, dest)
        .is_some_and(|package| package.2 == head);
    match (named, spelling.strip_prefix("crate")) {
        (true, Some(rest)) if rest.is_empty() || rest.starts_with("::") => format!("{head}{rest}"),
        _ => spelling,
    }
}

fn item_cfg_prefix(rel: &str, text: &str, decl: &Decl) -> Result<String, String> {
    if !rel.ends_with(".rs") {
        return Ok(String::new());
    }
    let out = dispatch(
        rel,
        text.as_bytes(),
        FamilyMask {
            cst: true,
            ..FamilyMask::NONE
        },
    )
    .ok_or_else(|| format!("no CST fact arm owns {rel}"))?;
    let mut items = Vec::new();
    let mut attrs = Vec::new();
    let mut identifiers = Vec::new();
    flatten_each(&out, None, &mut |fact: FlatFact| -> Result<(), ()> {
        if let FlatFact::Node {
            family: sprefa_extract::FamilyTag::Cst,
            kind,
            name,
            span,
            ..
        } = fact
        {
            match kind.as_str() {
                "attribute_item" => attrs.push(span),
                "identifier" => identifiers.push((span, name)),
                _ if kind.ends_with("_item") && name.as_deref() == Some(decl.name.as_str()) => {
                    items.push(span)
                }
                _ => {}
            }
        }
        Ok(())
    })
    .map_err(|_| format!("flatten cfg attributes in {rel}"))?;
    let Some(item_start) = items
        .iter()
        .filter(|span| decl.span.start <= span.start && span.end <= decl.span.end())
        .map(|span| span.start)
        .min()
    else {
        return Ok(String::new());
    };
    attrs.sort_by_key(|span| span.start);
    let mut prefix = String::new();
    for attr in attrs {
        if attr.start < decl.span.start || attr.end > item_start {
            continue;
        }
        let mut names = identifiers
            .iter()
            .filter(|(span, _)| attr.start <= span.start && span.end <= attr.end)
            .collect::<Vec<_>>();
        names.sort_by_key(|(span, _)| span.start);
        let gated = match names.first().and_then(|(_, name)| name.as_deref()) {
            Some("cfg") => true,
            Some("cfg_attr") => names
                .iter()
                .skip(1)
                .any(|(_, name)| name.as_deref() == Some("cfg")),
            _ => false,
        };
        if gated {
            if let Some(source) = text.get(attr.start as usize..attr.end as usize) {
                prefix.push_str(source);
                prefix.push('\n');
            }
        }
    }
    Ok(prefix)
}

pub(super) fn rust_children(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn rust_route_index(
    cx: &MoveCx,
) -> Result<hafley_scm::read::lang::rust_modules::RustModuleIndex, String> {
    use hafley_scm::read::lang::rust_modules::{rust_module_facts_from_tree, RustModuleIndex};
    let mut corpus = Vec::new();
    let mut outputs = Vec::new();
    let mut modules = Vec::new();
    for rel in cx.files() {
        if !rel.ends_with(".rs") && !rel.ends_with("Cargo.toml") {
            continue;
        }
        let Some(text) = cx.text(rel) else { continue };
        let blob = soopy::ContentId::blake3(text.as_bytes());
        corpus.push((rel.clone(), blob.clone()));
        if !rel.ends_with(".rs") {
            continue;
        }
        let parsed = hafley_scm::lang::rust::RustFastFile::extract(rel, text.as_bytes())
            .ok_or_else(|| format!("parse Rust module route in {rel}"))?;
        modules.push((
            rel.clone(),
            rust_module_facts_from_tree(parsed.tree(), text.as_bytes()),
        ));
        if let Some(output) = dispatch(
            rel,
            text.as_bytes(),
            FamilyMask {
                cst: true,
                call: true,
                ..FamilyMask::NONE
            },
        ) {
            outputs.push((blob, output));
        }
    }
    let pairs: Vec<_> = outputs
        .iter()
        .map(|(blob, output)| (blob.clone(), output.as_ref()))
        .collect();
    let defs = hafley_scm::read::types::build_def_index(&pairs);
    Ok(RustModuleIndex::build(modules, &corpus, &defs))
}

/// Where `export` goes in a declaration: its start, plus each later line that
/// opens another TS overload of `name`. Latest first, so earlier offsets hold.
fn statement_starts(text: &str, span: Span, name: &str) -> Vec<u32> {
    let body = text.get(span.start as usize..span.end() as usize).unwrap_or("");
    let mut starts = vec![span.start];
    for line in body.split_inclusive('\n').skip(1) {
        let head = line.trim_start();
        let at = line.as_ptr() as usize - body.as_ptr() as usize + (line.len() - head.len());
        let head = head.strip_prefix("export ").unwrap_or(head);
        let head = head.strip_prefix("async ").unwrap_or(head);
        if head
            .strip_prefix("function ")
            .and_then(|rest| rest.strip_prefix(name))
            .is_some_and(|rest| rest.starts_with(['(', '<']))
        {
            starts.push(span.start + at as u32);
        }
    }
    starts.reverse();
    starts
}

/// `edit` applied to `text`, which is how a block built from nothing grows.
/// `from`'s module spelling for `to`; an error when the resolver places either in no module.
fn spell(arm: &dyn Cleave, cx: &MoveCx, from: &str, to: &str) -> Result<String, String> {
    arm.spell_module(cx, from, to).ok_or_else(|| {
        format!("{from} cannot name {to}: the {} module resolver places one of them in no module", arm.name())
    })
}

fn apply(text: &str, edit: &sprefa_extract::Edit) -> String {
    let mut out = text[..edit.span.start as usize].to_string();
    out.push_str(&edit.text);
    out.push_str(&text[edit.span.end() as usize..]);
    out
}

/// The message a path no arm owns produces.
fn out_of_scope(rel: &str) -> String {
    format!("cleave has no arm for {rel}; {SCOPE}")
}

/// Line-aligned cuts merged, then widened over the blank lines they orphan.
/// Widening can make two cuts meet, so both run to a fixpoint.
fn absorb(text: &str, cuts: Vec<Span>) -> Vec<Span> {
    let bytes = text.as_bytes();
    let mut merged = merge(cuts);
    loop {
        let before: Vec<(u32, u32)> = merged.iter().map(|cut| (cut.start, cut.len)).collect();
        for cut in &mut merged {
            widen(bytes, cut);
        }
        merged = merge(merged);
        if before
            == merged
                .iter()
                .map(|cut| (cut.start, cut.len))
                .collect::<Vec<_>>()
        {
            return merged;
        }
    }
}

/// One cut widened over the blank lines it orphans: forward always, and
/// backward when it runs to the end of the file.
fn widen(bytes: &[u8], cut: &mut Span) {
    let mut end = cut.end() as usize;
    while end < bytes.len() && bytes[end] == b'\n' {
        end += 1;
    }
    let mut start = cut.start as usize;
    if end == bytes.len() {
        while start > 0 && bytes[start - 1] == b'\n' && (start < 2 || bytes[start - 2] == b'\n') {
            start -= 1;
        }
    }
    cut.start = start as u32;
    cut.len = end as u32 - start as u32;
}

/// Overlapping and touching spans folded into one, in offset order.
fn merge(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_by_key(|span| span.start);
    let mut out: Vec<Span> = Vec::new();
    for span in spans {
        match out.last_mut() {
            Some(last) if span.start <= last.end() => {
                last.len = last.end().max(span.end()) - last.start;
            }
            _ => out.push(span),
        }
    }
    out
}

// ── the cross-file read ─────────────────────────────────────────────────────

/// One resolve pass over the corpus, read as `resolved_import` rows: which
/// bound name reaches which file, and who imports `SRC#ITEM`.
struct Imports {
    /// `(importer, bound name) -> (file, declared name, through a re-export)`.
    names: Vec<(String, String, String, String, bool)>,
    /// Resolved call sites `(caller file, site span, callee file, callee name)`.
    calls: Vec<(String, Span, String, String)>,
    rust_routes: hafley_scm::read::lang::rust_modules::RustModuleIndex,
    /// Target -> importers over every resolved module specifier: the barrels a
    /// cleave must re-read are the importers of SRC.
    importers: BTreeMap<String, BTreeSet<String>>,
    ts_resolver: Option<sprefa_extract::lang::ts_resolve::TsResolver>,
}

impl Imports {
    /// The TS importers of `src`, the only files a TS row re-reads.
    fn barrels(&self, src: &str) -> Vec<String> {
        self.importers.get(src).into_iter().flatten()
            .filter(|importer| cleave_for(importer).is_some_and(|arm| arm.name() == "ts"))
            .inspect(|_| drop(tracing::trace_span!("cleave.ts.barrel").entered()))
            .cloned()
            .collect()
    }

    fn route_reaches(&self, from: &str, module: &str, bound: &str, target: &str) -> bool {
        use hafley_scm::read::lang::rust_modules::ModuleCallTarget;
        let asked = module.rsplit("::").next().unwrap_or(bound);
        let qualifier = module_key(asked, module);
        let segments: Vec<String> = qualifier.split("::").map(str::to_string).collect();
        match self.rust_routes.module_call(from, &segments, asked) {
            ModuleCallTarget::Target(blob, _) => self.rust_routes.blob_of(target) == Some(&blob),
            _ => false,
        }
    }

    fn read(cx: &MoveCx, root: &Path) -> Result<Self, String> {
        let paths: Vec<PathBuf> = cx
            .files()
            .iter()
            .filter(|rel| cleave_for(rel).is_some())
            .map(|rel| cx.abs(rel))
            .collect();
        let request = ResolveRequest {
            paths: &paths,
            arms: ResolveArms {
                call: true,
                ..ResolveArms::default()
            },
            scip: ScipMode::Off,
            project_root: Some(root),
            scip_records: ScipRecords::default(),
            occurrence_text: false,
            rust_checker: None,
            ts_checker: None,
            go_checker: None,
            witness: false,
        };
        let facts =
            resolve_project(&request).map_err(|error| format!("resolve {root:?}: {error}"))?;
        let mut names = Vec::new();
        let mut calls = Vec::new();
        let mut importers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for fact in &facts {
            if let FlatFact::ResolvedImportRow { src_path, target_path, target_name: None, kind, .. } = fact {
                if kind == "module" {
                    if let (Some(importer), Some(target)) = (rel_of(root, src_path), rel_of(root, target_path)) {
                        importers.entry(target).or_default().insert(importer);
                    }
                }
                continue;
            }
            if let FlatFact::ResolvedEdge {
                caller_path,
                callee_path,
                callee_name: Some(callee),
                caller_site_start,
                caller_site_end,
                ..
            } = fact
            {
                if let (Some(caller), Some(target)) =
                    (rel_of(root, caller_path), rel_of(root, callee_path))
                {
                    calls.push((
                        caller,
                        span_of(*caller_site_start, *caller_site_end),
                        target,
                        callee.clone(),
                    ));
                }
                continue;
            }
            let FlatFact::ResolvedImportRow {
                src_path,
                name,
                target_path,
                target_name: Some(declared),
                kind,
                ..
            } = fact
            else {
                continue;
            };
            let (Some(importer), Some(target)) =
                (rel_of(root, src_path), rel_of(root, target_path))
            else {
                continue;
            };
            let relayed = kind == "indirect" || kind == "star";
            names.push((importer, name.clone(), target, declared.clone(), relayed));
        }
        let rust_routes = rust_route_index(cx)?;
        let ts_resolver = if paths.iter().any(|path| cleave_for(&path.to_string_lossy()).is_some_and(|arm| arm.name() == "ts")) {
            Some(sprefa_extract::lang::ts_resolve::TsResolver::new(root)?)
        } else {
            None
        };
        Ok(Self {
            names,
            calls,
            rust_routes,
            importers,
            ts_resolver,
        })
    }

    /// One landed row folded in: the item's importers now reach DEST, DEST
    /// binds what travelled, and every site span follows its file's edits.
    #[allow(clippy::type_complexity)]
    fn land(&mut self, plan: &Plan, edits: &BTreeMap<String, (String, Vec<(Span, u32)>)>) {
        let (src, dest, item) = (&plan.rows.src, &plan.rows.dest, &plan.rows.item);
        let mut added = Vec::new();
        for row in &plan.rows.travelling {
            if let Some((_, _, target, declared, _)) = self
                .names
                .iter()
                .find(|(from, bound, ..)| from == src && *bound == row.name)
            {
                added.push((
                    dest.clone(),
                    row.name.clone(),
                    target.clone(),
                    declared.clone(),
                    false,
                ));
            }
        }
        for row in self.names.iter_mut() {
            if row.2 == *src && row.3 == *item {
                row.2 = dest.clone();
            }
        }
        self.names.extend(added);
        let relayed = self.importers.get(src).cloned().unwrap_or_default();
        let dest_importers = self.importers.entry(dest.clone()).or_default();
        dest_importers.insert(src.clone());
        dest_importers.extend(relayed);
        self.names
            .push((src.clone(), item.clone(), dest.clone(), item.clone(), false));
        self.calls.retain_mut(|(caller, span, target, callee)| {
            if *target == *src && callee == item {
                *target = dest.clone();
            }
            let Some((_, shifts)) = edits.get(caller.as_str()) else {
                return true;
            };
            let mut delta: i64 = 0;
            for (edited, new_len) in shifts {
                if edited.end() <= span.start && !(edited.len == 0 && edited.start == span.start) {
                    delta += *new_len as i64 - edited.len as i64;
                } else if edited.start < span.end()
                    && span.start < edited.end().max(edited.start + 1)
                {
                    return false;
                }
            }
            span.start = (span.start as i64 + delta) as u32;
            true
        });
    }

    /// Whether `name` in `importer` can be a trait: its corpus declaration says
    /// `trait`, or it reaches no corpus file and nothing rules it out.
    fn maybe_trait(&self, cx: &MoveCx, importer: &str, name: &str) -> bool {
        let Some((_, _, target, declared, _)) = self
            .names
            .iter()
            .find(|(from, bound, ..)| from == importer && bound == name)
        else {
            return IMPORTED_TRAITS.contains(&name) || name.ends_with("Ext");
        };
        cx.text(target).is_some_and(|text| {
            text.match_indices(&format!("trait {declared}"))
                .any(|(at, found)| {
                    !text[at + found.len()..]
                        .starts_with(|ch: char| ch.is_alphanumeric() || ch == '_')
                })
        })
    }

    /// Call sites outside SRC that name `src#item` through a path
    /// (`crate::lang::item(..)`), which no import row carries.
    fn qualified(&self, cx: &MoveCx, src: &str, item: &str) -> Vec<(String, Span)> {
        let mut out: Vec<(String, Span)> = self
            .calls
            .iter()
            .filter(|(caller, _, target, callee)| target == src && callee == item && caller != src)
            .filter(|(caller, span, _, _)| {
                cx.text(caller)
                    .and_then(|text| {
                        text.get(span.start as usize..span.end() as usize)
                            .map(str::to_string)
                    })
                    .is_some_and(|written| written.contains("::") && written.ends_with(item))
            })
            .map(|(caller, span, _, _)| (caller.clone(), *span))
            .collect();
        out.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.start.cmp(&right.1.start)));
        out.dedup();
        out
    }

    /// The corpus file the specifier binding `name` in `importer` reaches. A
    /// package import reaches nothing, which is what makes it a package.
    fn target(&self, importer: &str, name: &str) -> Option<&str> {
        self.names
            .iter()
            .find(|(from, bound, _, _, _)| from == importer && bound == name)
            .map(|(_, _, target, _, _)| target.as_str())
    }

    /// Every file importing `src#item` directly, in path order. A re-exported
    /// binding follows its re-export, which this plan respells.
    fn callers(&self, src: &str, item: &str) -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for (importer, _, target, declared, relayed) in &self.names {
            if target == src && declared == item && importer != src && !relayed {
                out.insert(importer.clone());
            }
        }
        out.into_iter().collect()
    }
}

/// A resolve echoes the path spellings it was given, which are absolute here.
fn rel_of(root: &Path, path: &str) -> Option<String> {
    Path::new(path)
        .strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

/// The file supplying a Rust child module's `use super::*` binding.
fn glob_parent(cx: &MoveCx, src: &str, module: &str) -> Option<String> {
    if module != "super" || !src.ends_with(".rs") {
        return None;
    }
    let dir = Path::new(src).parent()?;
    ["lib.rs", "mod.rs"]
        .iter()
        .map(|stem| dir.join(stem).to_string_lossy().replace('\\', "/"))
        .find(|path| path != src && cx.contains(path))
}

// ── the file read ───────────────────────────────────────────────────────────

/// One import specifier the file writes, as a fact row carries it.
#[derive(Clone)]
struct SpecifierRow {
    name: String,
    module: String,
    span: Span,
    glob: bool,
    kind: String,
    imported: Option<String>,
    type_only: bool,
}

/// One top-level declaration, line aligned so a cut takes whole lines.
#[derive(Clone)]
struct Decl {
    name: String,
    span: Span,
    exported: bool,
    type_only: bool,
}

#[derive(Clone)]
struct UnsupportedMacroItem {
    name: String,
    macro_name: String,
}

/// One corpus file's rows beside its bytes. Every method here is set
/// arithmetic over fact rows; none of it reads syntax.
struct FileFacts {
    text: String,
    is_ts: bool,
    specifiers: Vec<SpecifierRow>,
    import_statements: Vec<Span>,
    import_end: u32,
    mutable_bindings: Vec<Span>,
    /// The callee, its span, and whether it was reached through a receiver.
    sites: Vec<(String, Span, bool)>,
    decls: Vec<Decl>,
    /// Names the file needs from outside the item that owns them.
    free: Vec<(String, Span)>,
    /// Rust `impl` blocks by self type; they travel with that type.
    impls: Vec<(String, Span)>,
    /// Static-like names inside macros whose item expansion cleave does not handle.
    unsupported_macros: Vec<UnsupportedMacroItem>,
}

impl FileFacts {
    /// `whole`: also read the scope rows only SRC needs (declarations, their
    /// exported-ness, and the free names each one carries).
    fn open(cx: &MoveCx, rel: &str, whole: bool) -> Result<Self, String> {
        let text = cx
            .text(rel)
            .ok_or_else(|| format!("read {rel}, or it is not UTF-8"))?;
        let mask = FamilyMask {
            cst: true,
            call: true,
            ..FamilyMask::NONE
        };
        let out = dispatch(rel, text.as_bytes(), mask)
            .ok_or_else(|| format!("no fact arm owns {rel}"))?;
        let mut specifiers = Vec::new();
        let mut import_statements = Vec::new();
        let mut import_end = 0;
        let mut mutable_bindings = Vec::new();
        let mut sites = Vec::new();
        flatten_each(&out, None, &mut |fact: FlatFact| -> Result<(), ()> {
            match fact {
                FlatFact::Node {
                    family: sprefa_extract::FamilyTag::Cst,
                    kind,
                    span,
                    ..
                } if kind == "import_statement" => {
                    import_end = import_end.max(line_end(&text, span.end));
                    import_statements.push(span_of(span.start, span.end));
                }
                FlatFact::Node {
                    family: sprefa_extract::FamilyTag::Cst,
                    kind,
                    span,
                    ..
                } if kind == "export_statement" => {
                    import_statements.push(span_of(span.start, span.end));
                }
                FlatFact::Node {
                    family: sprefa_extract::FamilyTag::Cst,
                    kind,
                    span,
                    ..
                } if matches!(kind.as_str(), "lexical_declaration" | "variable_declaration") => {
                    if matches!(text[span.start as usize..span.end as usize].split_whitespace().next(), Some("let" | "var")) {
                        mutable_bindings.push(span_of(span.start, span.end));
                    }
                }
                FlatFact::Specifier {
                    span,
                    name,
                    module: Some(module),
                    kind,
                    imported,
                    type_only,
                    ..
                } => specifiers.push(SpecifierRow {
                    name,
                    module,
                    span: span_of(span.start, span.end),
                    glob: kind == "namespace"
                        && text.get(span.start as usize..span.end as usize) == Some("*"),
                    kind,
                    imported,
                    type_only,
                }),
                FlatFact::Site {
                    span,
                    callee,
                    callee_path,
                    ..
                } => sites.push((callee, span_of(span.start, span.end), callee_path.is_some())),
                _ => {}
            }
            Ok(())
        })
        .map_err(|_| format!("flatten {rel}"))?;
        let (decls, free, impls, unsupported_macros) = match whole {
            false => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            true => {
                let (decls, free, impls) = scope_rows(cx, rel, &text)?;
                let unsupported_macros = match rel.ends_with(".rs") {
                    true => macro_item_rows(&text)?,
                    false => Vec::new(),
                };
                (decls, free, impls, unsupported_macros)
            }
        };
        let mut free = free;
        let mut decls = decls;
        let serde_paths = serde_paths(&text);
        for (name, _) in &serde_paths {
            if decls.iter().any(|decl| decl.name == *name) {
                continue;
            }
            if let Some(decl) = inline_mod(&text, name) {
                decls.push(decl);
            }
        }
        decls.sort_by_key(|decl| decl.span.start);
        free.extend(serde_paths);
        Ok(Self {
            text,
            is_ts: matches!(
                rel.rsplit('.').next(),
                Some("ts" | "tsx" | "mts" | "cts" | "js" | "mjs")
            ),
            specifiers,
            import_statements,
            import_end,
            mutable_bindings,
            sites,
            decls,
            free,
            impls,
            unsupported_macros,
        })
    }

    /// The names the file binds per module, in byte order.
    fn modules(&self) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for row in self.specifiers.iter().filter(|row| {
            !row.glob
                && !matches!(
                    row.kind.as_str(),
                    "reexport" | "side_effect" | "dynamic_import" | "require"
                )
        }) {
            let module = module_key(&row.name, &row.module);
            match out.iter_mut().find(|(held, _)| *held == module) {
                Some((_, names)) => names.push(row.name.clone()),
                None => out.push((module, vec![row.name.clone()])),
            }
        }
        out
    }

    /// The end of the file's import region: one past the last import it writes.
    fn import_region(&self, text: &str) -> u32 {
        if self.is_ts {
            return self.import_end;
        }
        self.specifiers
            .iter()
            .map(|row| line_end(text, row.span.end()))
            .max()
            .unwrap_or(0)
            .max(self.import_end)
    }

    /// Whether the import holding `span` hands its names out (`pub use`,
    /// `export ... from`): a re-export is the file's API, never an orphan.
    fn reexports(&self, span: Span) -> bool {
        let before = &self.text[..span.start as usize];
        let start = before.rfind([';', '}']).map_or(0, |at| at + 1);
        let statement = before[start..]
            .lines()
            .map(str::trim_start)
            .find(|line| !line.is_empty() && !line.starts_with("//") && !line.starts_with("#["))
            .unwrap_or_default();
        statement.starts_with("pub ")
            || statement.starts_with("pub(")
            || statement.starts_with("export ")
    }

    /// The file's bytes under `span`, empty when the span is off the end.
    fn slice(&self, span: Span) -> &str {
        self.text
            .get(span.start as usize..span.end() as usize)
            .unwrap_or_default()
    }

    /// A trait import (`maybe_trait`: declared `trait`, or a package name) may be what a
    /// remaining method call resolves through; no name row shows that, so it stays.
    fn method_scope(&self, name: &str, maybe_trait: bool, moving: &[Span]) -> bool {
        maybe_trait
            && name.starts_with(|ch: char| ch.is_uppercase())
            && self.sites.iter().any(|(_, span, _)| {
                let dotted = self.text[..span.start as usize].trim_end().ends_with('.');
                dotted && !moving.iter().any(|scope| inside(*span, *scope))
            })
    }

    /// Free occurrences of `name` inside any of `spans`.
    fn refs_in(&self, name: &str, spans: &[Span]) -> usize {
        self.free
            .iter()
            .filter(|(used, span)| used == name && spans.iter().any(|scope| inside(*span, *scope)))
            .count()
    }

    /// Free occurrences of `name` outside every one of `spans`.
    fn refs_outside(&self, name: &str, spans: &[Span]) -> usize {
        self.free
            .iter()
            .filter(|(used, span)| used == name && !spans.iter().any(|scope| inside(*span, *scope)))
            .count()
    }

    /// Names the moving set calls that SRC binds and the plan carries nowhere.
    /// A name SRC does not bind is ambient — the language answers it in DEST
    /// the same way — and a call through a receiver is a member access.
    fn ungraded(&self, spans: &[Span], carried: &BTreeSet<&str>) -> Vec<String> {
        let bound: BTreeSet<&str> = self
            .specifiers
            .iter()
            .map(|row| row.name.as_str())
            .chain(self.decls.iter().map(|decl| decl.name.as_str()))
            .chain(self.unsupported_macros.iter().map(|row| row.name.as_str()))
            .collect();
        let mut out: BTreeSet<String> = BTreeSet::new();
        for (name, span) in &self.free {
            if self.unsupported_macros.iter().any(|row| row.name == *name)
                && spans.iter().any(|scope| inside(*span, *scope))
            {
                out.insert(name.clone());
            }
        }
        for (callee, span, through_receiver) in &self.sites {
            if *through_receiver || !spans.iter().any(|scope| inside(*span, *scope)) {
                continue;
            }
            if !bound.contains(callee.as_str()) || carried.contains(callee.as_str()) {
                continue;
            }
            if self.refs_in(callee, spans) == 0 {
                continue;
            }
            out.insert(callee.clone());
        }
        out.into_iter().collect()
    }

    /// Every private helper the item reaches, with the fixpoint pass count.
    /// Only a `moved` helper widens the set a later pass reads.
    fn drag_fixpoint(&self, item: &Decl, drag: bool) -> (Vec<CleaveDrag>, u32) {
        let mut moving = vec![item.span];
        let mut claimed: Vec<CleaveDrag> = Vec::new();
        let mut iterations = 1u32;
        loop {
            let found = self.drag_candidates(item, &moving, &claimed);
            if found.is_empty() {
                return (claimed, iterations);
            }
            let mut widened = false;
            for decl in found {
                let action = self.drag_action(&decl, &moving, drag);
                if action == "moved" {
                    moving.push(decl.span);
                    widened = true;
                }
                claimed.push(CleaveDrag {
                    name: decl.name,
                    span: decl.span,
                    iteration: iterations,
                    action,
                });
            }
            if !widened || self.drag_candidates(item, &moving, &claimed).is_empty() {
                return (claimed, iterations);
            }
            iterations += 1;
        }
    }

    /// Where one helper goes, answered once and never revised. A helper nothing
    /// left in SRC references travels under `--drag`; a shared one exports.
    fn drag_action(&self, decl: &Decl, moving: &[Span], drag: bool) -> &'static str {
        let mut scope = moving.to_vec();
        scope.push(decl.span);
        match drag && !decl.exported && self.refs_outside(&decl.name, &scope) == 0 {
            true => "moved",
            false => "exported",
        }
    }

    /// Private declarations the moving set references and no pass has claimed.
    fn drag_candidates(&self, item: &Decl, moving: &[Span], claimed: &[CleaveDrag]) -> Vec<Decl> {
        self.decls
            .iter()
            .filter(|decl| decl.name != item.name)
            .filter(|decl| !claimed.iter().any(|row| row.name == decl.name))
            .filter(|decl| self.refs_in(&decl.name, moving) > 0)
            .cloned()
            .collect()
    }
}

/// Names generated by item macros that cleave cannot move as ordinary items.
fn macro_item_rows(text: &str) -> Result<Vec<UnsupportedMacroItem>, String> {
    let file = hafley_scm::lang::rust::parse_rust_file(text).map_err(|error| format!("parse Rust macro items: {error}"))?;
    let mut unsupported = Vec::new();
    for item in file.items {
        let syn::Item::Macro(item) = item else {
            continue;
        };
        if let Some(ident) = &item.ident {
            if item
                .mac
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "macro_rules")
            {
                unsupported.push(UnsupportedMacroItem {
                    name: ident.to_string(),
                    macro_name: "macro_rules".to_string(),
                });
            }
            continue;
        }
        let Some(macro_name) = item
            .mac
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
        else {
            continue;
        };
        if macro_name == "thread_local" {
            if let Ok(body) = syn::parse2::<syn::File>(item.mac.tokens.clone()) {
                let statics: Vec<_> = body
                    .items
                    .into_iter()
                    .filter_map(|item| match item {
                        syn::Item::Static(item) => Some(item),
                        _ => None,
                    })
                    .collect();
                unsupported.extend(statics.into_iter().map(|item| UnsupportedMacroItem {
                    name: item.ident.to_string(),
                    macro_name: macro_name.clone(),
                }));
                continue;
            }
        }
        unsupported.extend(macro_static_names(item.mac.tokens).into_iter().map(|name| {
            UnsupportedMacroItem {
                name,
                macro_name: macro_name.clone(),
            }
        }));
    }
    Ok(unsupported)
}

fn macro_static_names(tokens: proc_macro2::TokenStream) -> Vec<String> {
    let tokens: Vec<_> = tokens.into_iter().collect();
    let mut names = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(token, proc_macro2::TokenTree::Ident(ident) if ident == "static") {
            continue;
        }
        let mut name = index + 1;
        if matches!(tokens.get(name), Some(proc_macro2::TokenTree::Ident(ident)) if ident == "ref" || ident == "mut")
        {
            name += 1;
        }
        if let Some(proc_macro2::TokenTree::Ident(ident)) = tokens.get(name) {
            names.push(ident.to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}

/// The type a Rust `impl` block is for: `impl<T> Trait<U> for Name<T>` and
/// `impl Name` both answer `Name`.
fn impl_self(text: &str) -> Option<String> {
    let head = text.split('{').next()?.trim().strip_prefix("impl")?;
    let head = head.split(" where ").next()?.trim();
    let mut depth = 0i32;
    let mut generics_end = 0;
    if head.starts_with('<') {
        for (at, ch) in head.char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        generics_end = at + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    let head = head[generics_end..].trim();
    let target = head.rsplit_once(" for ").map_or(head, |(_, ty)| ty).trim();
    let target = target
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim();
    let name: String = target
        .rsplit("::")
        .next()?
        .chars()
        .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Root children a declaration carries when it moves: the doc comments,
/// attributes and decorators written directly above it.
const LEADING_TRIVIA: [&str; 5] = [
    "line_comment",
    "block_comment",
    "attribute_item",
    "comment",
    "decorator",
];

/// Where a declaration's cut starts: the first trivia root child directly
/// above the root child holding it, with no blank line between any two.
fn leading_trivia_start(text: &str, children: &[(u32, u32, String)], start: u32, end: u32) -> u32 {
    let Some(at) = children
        .iter()
        .position(|(from, to, _)| *from <= start && end <= *to)
    else {
        return start;
    };
    let mut first = children[at].0;
    for (from, to, kind) in children[..at].iter().rev() {
        let gap = text.get(*to as usize..first as usize).unwrap_or("x");
        if !LEADING_TRIVIA.contains(&kind.as_str())
            || gap.matches('\n').count() > 1
            || !gap.trim().is_empty()
        {
            break;
        }
        first = *from;
    }
    first.min(start)
}

/// The declared name inside a `scm` symbol spelling.
fn declared(symbol: &str) -> String {
    symbol
        .rsplit_once('/')
        .map_or(symbol, |(_, tail)| tail)
        .trim_end_matches("().")
        .to_string()
}

fn span_of(start: u32, end: u32) -> Span {
    Span {
        start,
        len: end - start,
    }
}

/// `span` widened to whole lines, the newline closing its last included.
fn line_span(text: &str, span: Span) -> Span {
    let bytes = text.as_bytes();
    let mut start = span.start as usize;
    while start > 0 && bytes[start - 1] != b'\n' {
        start -= 1;
    }
    Span {
        start: start as u32,
        len: line_end(text, span.end()) - start as u32,
    }
}

/// One past the newline that closes the line `at` sits on.
fn line_end(text: &str, at: u32) -> u32 {
    let bytes = text.as_bytes();
    let mut end = at as usize;
    while end < bytes.len() && bytes[end - 1] != b'\n' {
        end += 1;
    }
    end as u32
}

/// Whether `inner` sits inside `outer`, endpoints included.
fn inside(inner: Span, outer: Span) -> bool {
    inner.start >= outer.start && inner.end() <= outer.end()
}

// ── the request ─────────────────────────────────────────────────────────────

/// `<SRC>#<ITEM>` split at the last `#`, so a path holding one still parses.
fn split_target(target: &str) -> Result<(PathBuf, String), String> {
    let (src, item) = target
        .rsplit_once('#')
        .ok_or_else(|| format!("cleave target must be SRC#ITEM, got {target}"))?;
    if src.is_empty() || item.is_empty() {
        return Err(format!("cleave target must be SRC#ITEM, got {target}"));
    }
    Ok((PathBuf::from(src), item.to_string()))
}

/// The corpus root: as asked, else the git root holding SRC.
fn plan_root(requested: Option<&PathBuf>, src: &Path) -> Result<PathBuf, String> {
    let root = match requested {
        Some(root) => {
            let root = super::source_move::absolute(root)?;
            if !root.is_dir() {
                return Err(format!("--root is not a directory: {}", root.display()));
            }
            root
        }
        None => {
            let src = anchor_file(src)?;
            let parent = src.parent().unwrap_or(&src).to_path_buf();
            soopy::discover(&parent)
                .map_err(|error| format!("discover root for {}: {error}", src.display()))?
                .root
        }
    };
    root.canonicalize()
        .map_err(|error| format!("canonicalize root {}: {error}", root.display()))
}

/// SRC as `anchor_file` finds it, or a file an earlier batch row created.
fn anchor_file_in(cx: &MoveCx, path: &Path) -> Result<PathBuf, String> {
    let unborn = super::source_move::canonical_unborn(&super::source_move::absolute(path)?);
    match cx
        .rel(&unborn)
        .is_some_and(|rel| cx.overlaid().contains_key(&rel))
    {
        true => Ok(unborn),
        false => anchor_file(path),
    }
}

fn anchor_file(path: &Path) -> Result<PathBuf, String> {
    let path = super::source_move::absolute(path)?;
    if !path.is_file() {
        return Err(format!("cleave source is not a file: {}", path.display()));
    }
    path.canonicalize()
        .map_err(|error| format!("canonicalize {}: {error}", path.display()))
}

/// One package as a cleave judges it: directory, name, the ident a path spells
/// it with, and the dependency keys its manifest declares.
type PackageView = (String, String, String, BTreeSet<String>);

fn package_view(cx: &MoveCx, language: &str, rel: &str) -> Option<PackageView> {
    match language {
        "rust" => sprefa_extract::edit::rust_rehome::cargo_package(cx, rel),
        _ => sprefa_extract::edit::ts_rehome::cross::package_deps(cx, rel)
            .map(|(name, deps)| (name.clone(), name.clone(), name, deps)),
    }
}

/// The manifest key a package specifier names: `serde` of `serde::de`, `@a/b` of
/// `@a/b/c`, `lodash` of `lodash/fp`.
fn package_key<'a>(language: &str, module: &'a str) -> &'a str {
    match language {
        "rust" => module.split("::").next().unwrap_or(module),
        _ => {
            let cut = match module.starts_with('@') {
                true => module.match_indices('/').nth(1).map(|(at, _)| at),
                false => module.find('/'),
            };
            &module[..cut.unwrap_or(module.len())]
        }
    }
}

fn depends_on(user: &PackageView, on: &PackageView) -> bool {
    user.3
        .iter()
        .any(|key| *key == on.1 || key.replace('-', "_") == on.2)
}

/// A cleave into another package never edits manifests: it stops, naming the
/// dependency the result would need, when the needed edge is missing or cycles.
#[allow(clippy::too_many_arguments)]
fn cross_package_stop(
    cx: &MoveCx,
    language: &str,
    src: &str,
    dest: &str,
    item: &str,
    callers: &[String],
    src_uses_item: bool,
    dest_imports: &[(String, Vec<String>)],
    travelling: &[CleaveSpecifier],
) -> Result<(), String> {
    let (Some(from), Some(to)) = (
        package_view(cx, language, src),
        package_view(cx, language, dest),
    ) else {
        return Ok(());
    };
    if from.0 == to.0 {
        return Ok(());
    }
    let mut users: Vec<(String, PackageView)> = callers
        .iter()
        .filter_map(|caller| Some((caller.clone(), package_view(cx, language, caller)?)))
        .collect();
    if src_uses_item {
        users.push((src.to_string(), from.clone()));
    }
    let back = dest_imports.iter().any(|(module, _)| {
        module == &from.2
            || module.starts_with(&format!("{}::", from.2))
            || module.starts_with(&format!("{}/", from.2))
    });
    for (user, package) in &users {
        if package.0 == to.0 {
            continue;
        }
        if back && package.0 == from.0 {
            return Err(format!(
                "dependency cycle: {} -> {} -> {} ({user} uses {item}, and {dest} would import from {})",
                from.1, to.1, from.1, from.1
            ));
        }
        if !depends_on(package, &to) {
            return Err(format!(
                "cleave across packages: {} must depend on {} ({user} uses {item}); add the dependency, or `ryi move` the whole file (move edits manifests)",
                package.1, to.1
            ));
        }
    }
    // A third-party import travelling with the item needs the same key in DEST's
    // manifest; std and the package's own ident need none.
    for row in travelling.iter().filter(|row| row.kind == "package") {
        let key = package_key(language, &row.dest_module);
        let builtin = match language {
            "rust" => matches!(key, "std" | "core" | "alloc" | "crate" | "self" | "super"),
            _ => key.starts_with("node:") || key.starts_with('.'),
        };
        if builtin
            || key == to.2
            || to
                .3
                .iter()
                .any(|dep| dep == key || dep.replace('-', "_") == key)
        {
            continue;
        }
        return Err(format!(
            "cleave across packages: {} must depend on {key} ({item} imports {} from {}); add the dependency",
            to.1, row.name, row.dest_module
        ));
    }
    if back && !depends_on(&to, &from) {
        return Err(format!(
            "cleave across packages: {} must depend on {} ({dest} would import from it); add the dependency, or `ryi move` the whole file",
            to.1, from.1
        ));
    }
    Ok(())
}

#[cfg(test)]
mod batch_row_tests {
    use super::*;

    #[test]
    fn equivalent_pub_insertions_form_one_valid_item() {
        let edit = || Respell {
            file: "src/_1_pattern.rs".to_string(),
            span: Span::anchor(0),
            text: "pub ".to_string(),
            receipt: None,
        };
        let edits = Plan::normalize_row_edits("src/_1_pattern.rs", vec![edit(), edit()]).unwrap();
        let mut text = "struct Pattern;".to_string();
        for edit in edits.iter().rev() {
            text.replace_range(
                edit.span.start as usize..edit.span.end() as usize,
                &edit.text,
            );
        }
        assert_eq!(text, "pub struct Pattern;");
        hafley_scm::lang::rust::parse_rust_file(&text).unwrap();
    }
}

#[cfg(test)]
mod barrel_growth_tests {
    use super::*;
    use hafley_observe::{assert_growth_sized, CountRecorder, Growth, SpanCounts};
    use tracing_subscriber::prelude::*;

    /// `files` TS files, the first `barrels` of them importing SRC.
    fn imports(files: usize, barrels: usize) -> Imports {
        let mut importers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for file in 0..files {
            let target = if file < barrels { "src/item.ts".to_string() } else { format!("src/other_{file}.ts") };
            importers.entry(target).or_default().insert(format!("src/file_{file}.ts"));
        }
        Imports {
            names: Vec::new(),
            calls: Vec::new(),
            rust_routes: Default::default(),
            importers,
            ts_resolver: None,
        }
    }

    fn barrel_reads(files: usize, barrels: usize, rows: usize) -> SpanCounts {
        let imports = imports(files, barrels);
        let (recorder, layer) = CountRecorder::new();
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
            for _ in 0..rows {
                assert_eq!(imports.barrels("src/item.ts").len(), barrels);
            }
        });
        recorder.counts()
    }

    #[test]
    fn barrel_reads_grow_with_rows_and_src_barrels_never_with_corpus_files() {
        let span = "cleave.ts.barrel";
        let (small, large) = (barrel_reads(20, 5, 4), barrel_reads(2000, 5, 4));
        assert_growth_sized(&small, &large, span, 20, 2000, Growth::Constant);
        let (small, large) = (barrel_reads(2000, 5, 4), barrel_reads(2000, 500, 4));
        assert_growth_sized(&small, &large, span, 5, 500, Growth::Linear);
        let (small, large) = (barrel_reads(2000, 5, 2), barrel_reads(2000, 5, 200));
        assert_growth_sized(&small, &large, span, 2, 200, Growth::Linear);
    }
}
