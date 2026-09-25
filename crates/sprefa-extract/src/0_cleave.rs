//! `ryi cleave <SRC>#<ITEM> <DEST>`: one item leaves SRC and lands in DEST,
//! carrying the specifiers it needs and respelling every importer. The plan is
//! fact rows only; the `Cleave` roster spells the three edits they cannot.
//! @comment-ok: module header, the seam list every bin arm opens with

use crate::cli::CleaveArgs;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use sprefa_extract::move_stage::{
    content_id, print_previews, run_verify_command, stage_and_commit, state_root, Mirror,
    VerifyJournal,
};
use sprefa_extract::{
    directory_path, directory_source, dispatch, flatten_each, cleave_for, normalize,
    replace_action, resolve_project, scm_facts, FamilyMask, FlatFact, MoveCx, Cleave,
    ResolveArms, ResolveRequest, Respell, ScipMode, ScipRecords, Span,
};
use sprefa_extract::edit_seams::CleavePlan;
use sprefa_extract::edit_seams::CleaveSpecifier;
use sprefa_extract::edit_seams::CleaveDrag;

const PRODUCER: &str = "extract-cleave";

const SCOPE: &str = "not supported: cross-language cleave, moving a type with its impl blocks";

pub fn run(cli: CleaveArgs) -> Result<(), String> {
    if cli.verify.is_some() && !cli.commit {
        return Err(
            "--verify needs --commit".to_string(),
        );
    }
    let plan = Plan::build(&cli)?;
    let state = state_root(cli.state.as_deref())?;

    println!("root {}", plan.root.display());
    println!(
        "plan {}#{} -> {}",
        plan.rows.src, plan.rows.item, plan.rows.dest
    );
    if !plan.rows.unresolved.is_empty() {
        for name in &plan.rows.unresolved {
            println!("ungraded {name}");
        }
        println!(
            "next: ryi graph --uses {} {}",
            plan.rows.unresolved[0],
            plan.root.display()
        );
        return Ok(());
    }
    for row in &plan.rows.travelling {
        println!(
            "travel {} from {} as {} ({})",
            row.name, row.module, row.dest_module, row.kind
        );
    }
    for row in &plan.rows.orphans {
        println!("orphan {} from {}", row.name, row.module);
    }
    for row in &plan.rows.dragged {
        println!("drag {} {} pass {}", row.name, row.action, row.iteration);
    }
    println!("drag fixpoint {} passes", plan.rows.drag_iterations);
    for caller in &plan.rows.callers {
        println!("caller {caller}");
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
    match cli.commit {
        true => {
            let journal =
                VerifyJournal::capture(&plan.root, &[], &plan.created(), &plan.touched())?;
            for stage in &stages {
                let (id, previews) =
                    stage_and_commit(&plan.root, &state, stage, soopy::Durability::Durable)?;
                print_previews(&previews, "");
                println!("stage {id} committed");
            }
            verify_after_commit(&plan, &state, cli.verify.as_deref(), &journal)?;
        }
        false => {
            for (id, previews) in dry_previews {
                print_previews(&previews, "");
                println!("stage {id} dry run, tree untouched");
            }
        }
    }
    if cli.text_refs {
        report_text_refs(&plan);
    }
    if cli.json {
        println!("{}", plan_json(&plan.rows));
    }
    Ok(())
}

/// Keep-if-pass: a non-zero or timed-out checker walks every touched path back
/// to its pre-run bytes, deletes the DEST this run created, and exits 3.
fn verify_after_commit(
    plan: &Plan,
    state: &Path,
    command: Option<&str>,
    journal: &VerifyJournal,
) -> Result<(), String> {
    let Some(command) = command else {
        return Ok(());
    };
    match run_verify_command(&plan.root, command)? {
        Some(0) => println!("verify ok"),
        code => {
            let reason = code.map_or_else(|| "timeout".to_string(), |rc| rc.to_string());
            let count = journal.restore(&plan.root, state, &[])?;
            println!("verify failed (rc={reason}): rolled back {count} files");
            super::exit(3);
        }
    }
    Ok(())
}

/// Lines naming the item in files no arm owns and no plan edit covers.
/// Rewriting text carriers is out of scope for `move` and for this verb.
fn report_text_refs(plan: &Plan) {
    let edited: BTreeSet<String> = plan.touched().into_iter().collect();
    for rel in plan.cx.files() {
        if cleave_for(rel).is_some() || edited.contains(rel) {
            continue;
        }
        let Some(text) = plan.cx.text(rel) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if line.contains(&plan.rows.item) {
                println!(
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
    /// Call sites naming the item through a module path, as (file, span).
    qualified: Vec<(String, Span)>,
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
            syn::parse_file(&text).map_err(|error| {
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
        let (src, item) = split_target(&cli.target)?;
        let root = plan_root(cli.root.as_ref(), &src)?;
        let cx = MoveCx::open(&root)?;
        let src = within_root(&root, &anchor_file(&src)?)?;
        let dest = within_root(&root, &canonical_unborn(&absolute(&cli.dest)?))?;
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

        let source = FileFacts::open(&cx, &src, true)?;
        let item_decl = source
            .decls
            .iter()
            .find(|decl| decl.name == item)
            .ok_or_else(|| format!("{src} declares no {item}"))?
            .clone();

        let imports = Imports::read(&cx, &root)?;
        let (mut dragged, drag_iterations) = source.drag_fixpoint(&item_decl, cli.drag);
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
        let carried: BTreeSet<(String, String)> = dest_facts
            .iter()
            .flat_map(|facts| facts.specifiers.iter())
            .map(|row| (row.name.clone(), module_key(&row.name, &row.module)))
            .collect();

        let mut travelling = Vec::new();
        let mut orphans = Vec::new();
        let mut glob_unresolved = BTreeSet::new();
        for row in source.specifiers.iter().filter(|row| !row.glob) {
            let dest_module = match imports.target(&src, &row.name) {
                Some(target) => arm
                    .respell_relative(&cx, &src, &dest, &row.module)
                    .unwrap_or_else(|| arm.spell_module(&cx, &dest, target)),
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
            if source.refs_in(&row.name, &moving) > 0 && !dest_bound.contains(row.name.as_str()) {
                travelling.push(plan_row.clone());
            }
            if source.refs_outside(&row.name, &moving) == 0 && !source.reexports(row.span) {
                orphans.push(plan_row);
            }
        }
        for glob in source.specifiers.iter().filter(|row| row.glob) {
            let Some(parent) = glob_parent(&cx, &src, &glob.module) else {
                continue;
            };
            let provider = FileFacts::open(&cx, &parent, true)?;
            for row in provider.specifiers.iter().filter(|row| !row.glob) {
                if source.refs_in(&row.name, &moving) == 0
                    || travelling.iter().any(|held| held.name == row.name)
                {
                    continue;
                }
                let target = imports.target(&parent, &row.name);
                let dest_module = target
                    .map(|path| arm.spell_module(&cx, &dest, path))
                    .unwrap_or_else(|| row.module.clone());
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
                let dest_module = arm.spell_module(&cx, &dest, &parent);
                let kind = if carried.contains(&(decl.name.clone(), module_key(&decl.name, &dest_module))) {
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
                dest_facts: None,
                moving_text: Vec::new(),
                dest_imports: Vec::new(),
                callers: Vec::new(),
                caller_modules: Vec::new(),
                qualified: Vec::new(),
            });
        }

        let src_module = arm.spell_module(&cx, &dest, &src);
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
                    .filter(|row| !carried.contains(&(row.name.clone(), module_key(&row.name, &src_module))))
                    .map(|row| (row.name.clone(), src_module.clone())),
            );
        for (name, module) in wanted {
            match dest_imports.iter_mut().find(|(held, _)| *held == module) {
                Some((_, names)) => names.push(name),
                None => {
                    let mut names: Vec<String> = dest_facts
                        .iter()
                        .flat_map(|facts| facts.specifiers.iter())
                        .filter(|row| row.module == module)
                        .map(|row| row.name.clone())
                        .collect();
                    names.push(name);
                    dest_imports.push((module, names));
                }
            }
        }

        let callers = imports.callers(&src, &item);
        let mut views = Vec::with_capacity(callers.len());
        let mut caller_modules = Vec::with_capacity(callers.len());
        for caller in &callers {
            let facts = FileFacts::open(&cx, caller, false)?;
            caller_modules.push(
                facts
                    .specifiers
                    .iter()
                    .find(|row| row.name == item)
                    .map_or_else(String::new, |row| row.module.clone()),
            );
            views.push(facts);
        }
        moving.sort_by_key(|span| span.start);
        let mut moving_text: Vec<String> = moving
            .iter()
            .map(|span| source.slice(*span).to_string())
            .collect();
        if !item_decl.exported
            && (source.refs_outside(&item, &moving) > 0 || !callers.is_empty())
        {
            if let Some(first) = moving_text.first_mut() {
                if let Some(edit) = arm.edit_export(first, Span::anchor(0), true) {
                    *first = apply(first, &edit);
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
            dest_facts,
            moving_text,
            dest_imports,
            callers: views,
            caller_modules,
            qualified,
        })
    }

    /// Every byte this cleave rewrites, as one `Respell` per span, in (file,
    /// offset) order. Nothing here touches the tree; the stages do.
    fn respells(&self) -> Vec<Respell> {
        let mut out = self.source_respells();
        out.extend(self.dest_respells());
        out.extend(self.caller_respells());
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
            let Some(written) = self
                .cx
                .text(rel)
                .and_then(|text| text.get(span.start as usize..span.end() as usize).map(str::to_string))
            else {
                continue;
            };
            let module = written.rsplit_once("::").map_or("", |(module, _)| module);
            let spelling = as_written(
                &self.cx,
                &self.rows.dest,
                module,
                self.arm.spell_module(&self.cx, rel, &self.rows.dest),
            );
            out.push(Respell {
                receipt: Some(format!("path {rel}: {written} -> {spelling}::{}", self.rows.item)),
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
        for row in self.rows.dragged.iter().filter(|row| row.action == "exported") {
            let Some(edit) = self.arm.edit_export(&self.source.text, row.span, true) else {
                continue;
            };
            edits.push(Respell {
                file: self.rows.src.clone(),
                span: edit.span,
                text: edit.text,
                receipt: Some(format!("export {} stays in {}", row.name, self.rows.src)),
            });
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
            let module = self.arm.spell_module(&self.cx, &self.rows.src, &self.rows.dest);
            if let Some(edit) = self.arm.edit_import(
                &self.source.text,
                std::slice::from_ref(&self.rows.item),
                &module,
            ) {
                edits.push(Respell {
                    file: self.rows.src.clone(),
                    span: edit.span,
                    text: edit.text,
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
                edit.span.len == 0 && cut.span.start <= edit.span.start && edit.span.start < cut.span.end()
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
            if let Some(edit) = self.arm.edit_import(&block, names, module) {
                block = apply(&block, &edit);
            }
        }
        // DEST that imported the item now declares it: that import goes.
        for (module, names) in self.dest_facts.iter().flat_map(|facts| facts.modules()) {
            if !names.contains(&self.rows.item) {
                continue;
            }
            let kept: Vec<String> = names.into_iter().filter(|name| *name != self.rows.item).collect();
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
            let kept: Vec<String> = facts
                .specifiers
                .iter()
                .filter(|row| row.module == *module && row.name != self.rows.item)
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
                self.arm.spell_module(&self.cx, rel, &self.rows.dest),
            );
            let mut landing: Vec<String> = facts
                .specifiers
                .iter()
                .filter(|row| row.module == spelling)
                .map(|row| row.name.clone())
                .collect();
            landing.push(self.rows.item.clone());
            if let Some(edit) =
                self.arm
                    .edit_import_like(&facts.text, &landing, &spelling, &self.rows.item, module)
            {
                out.push(Respell {
                    file: rel.clone(),
                    span: edit.span,
                    text: edit.text,
                    receipt: Some(format!("caller {rel}: {} -> {spelling}", self.rows.item)),
                });
            }
        }
        out
    }

    /// One soopy stage of Replace actions, plus a Create stage when DEST is new.
    fn stages(&self) -> Result<Vec<Vec<soopy::SourceAction>>, String> {
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
                bytes: format!("{block}\n{}", self.moving_text.join("\n")).into_bytes(),
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
                .declare_new_file(&self.cx, &self.rows.src, &self.rows.dest),
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
                    self.arm.spell_module(&self.cx, rel, &self.rows.dest),
                );
                !matches!(spelling.split("::").next(), Some("crate" | "self" | "super"))
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

/// `edit` applied to `text`, which is how a block built from nothing grows.
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
        if before == merged.iter().map(|cut| (cut.start, cut.len)).collect::<Vec<_>>() {
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
}

impl Imports {
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
        for fact in &facts {
            if let FlatFact::ResolvedEdge {
                caller_path,
                callee_path,
                callee_name: Some(callee),
                caller_site_start,
                caller_site_end,
                ..
            } = fact
            {
                if let (Some(caller), Some(target)) = (rel_of(root, caller_path), rel_of(root, callee_path)) {
                    calls.push((caller, span_of(*caller_site_start, *caller_site_end), target, callee.clone()));
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
            let (Some(importer), Some(target)) = (rel_of(root, src_path), rel_of(root, target_path))
            else {
                continue;
            };
            let relayed = kind == "indirect" || kind == "star";
            names.push((importer, name.clone(), target, declared.clone(), relayed));
        }
        Ok(Self { names, calls })
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
                    .and_then(|text| text.get(span.start as usize..span.end() as usize).map(str::to_string))
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
struct SpecifierRow {
    name: String,
    module: String,
    span: Span,
    glob: bool,
}

/// One top-level declaration, line aligned so a cut takes whole lines.
#[derive(Clone)]
struct Decl {
    name: String,
    span: Span,
    exported: bool,
}

/// One corpus file's rows beside its bytes. Every method here is set
/// arithmetic over fact rows; none of it reads syntax.
struct FileFacts {
    text: String,
    specifiers: Vec<SpecifierRow>,
    /// The callee, its span, and whether it was reached through a receiver.
    sites: Vec<(String, Span, bool)>,
    decls: Vec<Decl>,
    /// Names the file needs from outside the item that owns them.
    free: Vec<(String, Span)>,
    /// Rust `impl` blocks by self type; they travel with that type.
    impls: Vec<(String, Span)>,
}

impl FileFacts {
    /// `whole`: also read the scope rows only SRC needs (declarations, their
    /// exported-ness, and the free names each one carries).
    fn open(cx: &MoveCx, rel: &str, whole: bool) -> Result<Self, String> {
        let text = cx
            .text(rel)
            .ok_or_else(|| format!("read {rel}, or it is not UTF-8"))?;
        let mask = FamilyMask {
            call: true,
            ..FamilyMask::NONE
        };
        let out = dispatch(rel, text.as_bytes(), mask)
            .ok_or_else(|| format!("no fact arm owns {rel}"))?;
        let mut specifiers = Vec::new();
        let mut sites = Vec::new();
        flatten_each(&out, None, &mut |fact: FlatFact| -> Result<(), ()> {
            match fact {
                FlatFact::Specifier {
                    span,
                    name,
                    module: Some(module),
                    kind,
                    ..
                } => specifiers.push(SpecifierRow {
                    name,
                    module,
                    span: span_of(span.start, span.end),
                    glob: kind == "namespace"
                        && text.get(span.start as usize..span.end as usize) == Some("*"),
                }),
                FlatFact::Site {
                    span,
                    callee,
                    callee_path,
                    ..
                } => sites.push((
                    callee,
                    span_of(span.start, span.end),
                    callee_path.is_some(),
                )),
                _ => {}
            }
            Ok(())
        })
        .map_err(|_| format!("flatten {rel}"))?;
        let (decls, free, impls) = match whole {
            false => (Vec::new(), Vec::new(), Vec::new()),
            true => scope_rows(cx, rel, &text)?,
        };
        Ok(Self {
            text,
            specifiers,
            sites,
            decls,
            free,
            impls,
        })
    }

    /// The names the file binds per module, in byte order.
    fn modules(&self) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for row in self.specifiers.iter().filter(|row| !row.glob) {
            match out.iter_mut().find(|(held, _)| *held == row.module) {
                Some((_, names)) => names.push(row.name.clone()),
                None => out.push((row.module.clone(), vec![row.name.clone()])),
            }
        }
        out
    }

    /// The end of the file's import region: one past the last import it writes.
    fn import_region(&self, text: &str) -> u32 {
        self.specifiers
            .iter()
            .map(|row| line_end(text, row.span.end()))
            .max()
            .unwrap_or(0)
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
        statement.starts_with("pub ") || statement.starts_with("pub(") || statement.starts_with("export ")
    }

    /// The file's bytes under `span`, empty when the span is off the end.
    fn slice(&self, span: Span) -> &str {
        self.text
            .get(span.start as usize..span.end() as usize)
            .unwrap_or_default()
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
            .collect();
        let mut out: BTreeSet<String> = BTreeSet::new();
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

/// The scope rows one file contributes: its top-level declarations, line
/// aligned, every free name each carries, and each Rust `impl` by self type.
#[allow(clippy::type_complexity)]
fn scope_rows(
    cx: &MoveCx,
    rel: &str,
    text: &str,
) -> Result<(Vec<Decl>, Vec<(String, Span)>, Vec<(String, Span)>), String> {
    let path = cx.abs(rel);
    let facts =
        scm_facts(&[path]).map_err(|error| format!("scope rows for {rel}: {error}"))?;
    let (top_level, root_children) = root_item_spans(rel, text)?;
    let mut decls: Vec<Decl> = Vec::new();
    let mut free = Vec::new();
    let file = Span {
        start: 0,
        len: text.len() as u32,
    };
    for fact in &facts {
        match fact {
            FlatFact::OccurrenceRow {
                role,
                exported,
                decl_start,
                decl_end,
                symbol,
                ..
            } if role == "def" => {
                let start = leading_trivia_start(text, &root_children, *decl_start, *decl_end);
                let span = line_span(text, span_of(start, *decl_end));
                if span == file || !top_level.contains(&(*decl_start, *decl_end)) {
                    continue;
                }
                let name = declared(symbol);
                if decls.iter().any(|held| held.name == name) {
                    continue;
                }
                decls.push(Decl {
                    name,
                    span,
                    exported: *exported,
                });
            }
            FlatFact::FreeNameRow {
                name, start, end, ..
            } => free.push((name.clone(), span_of(*start, *end))),
            _ => {}
        }
    }
    decls.sort_by_key(|decl| decl.span.start);
    let impls = root_children
        .iter()
        .filter(|(_, _, kind)| kind == "impl_item")
        .filter_map(|(start, end, _)| {
            let self_ty = impl_self(text.get(*start as usize..*end as usize)?)?;
            let from = leading_trivia_start(text, &root_children, *start, *end);
            Some((self_ty, line_span(text, span_of(from, *end))))
        })
        .collect();
    Ok((decls, free, impls))
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
    let target = target.trim_start_matches('&').trim_start_matches("mut ").trim();
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

/// Declaration spans at the CST root, including declarations wrapped by a TS
/// `export_statement`, beside every root child and its kind in byte order.
#[allow(clippy::type_complexity)]
fn root_item_spans(
    rel: &str,
    text: &str,
) -> Result<(BTreeSet<(u32, u32)>, Vec<(u32, u32, String)>), String> {
    let mask = FamilyMask {
        cst: true,
        ..FamilyMask::NONE
    };
    let out = dispatch(rel, text.as_bytes(), mask)
        .ok_or_else(|| format!("no CST fact arm owns {rel}"))?;
    let mut exports = BTreeSet::new();
    let mut variable_lists = BTreeSet::new();
    let mut children = Vec::new();
    let mut kinds: BTreeMap<(u32, u32), String> = BTreeMap::new();
    flatten_each(&out, None, &mut |fact: FlatFact| -> Result<(), ()> {
        if let FlatFact::Node {
            family: sprefa_extract::FamilyTag::Cst,
            kind,
            span,
            ..
        } = &fact
        {
            kinds
                .entry((span.start, span.end))
                .or_insert_with(|| kind.clone());
        }
        match fact {
            FlatFact::Node {
                family: sprefa_extract::FamilyTag::Cst,
                kind,
                span,
                ..
            } if kind == "export_statement" => {
                exports.insert((span.start, span.end));
            }
            FlatFact::Node {
                family: sprefa_extract::FamilyTag::Cst,
                kind,
                span,
                ..
            } if matches!(kind.as_str(), "lexical_declaration" | "variable_declaration") => {
                variable_lists.insert((span.start, span.end));
            }
            FlatFact::Edge {
                family: sprefa_extract::FamilyTag::Cst,
                kind,
                from,
                to,
                ..
            } if kind == "child" => children.push(((from.start, from.end), (to.start, to.end))),
            _ => {}
        }
        Ok(())
    })
    .map_err(|_| format!("flatten CST {rel}"))?;
    let root = (0, text.len() as u32);
    let mut root_children: Vec<(u32, u32, String)> = children
        .iter()
        .filter(|(from, _)| *from == root)
        .map(|(_, to)| (to.0, to.1, kinds.get(to).cloned().unwrap_or_default()))
        .collect();
    root_children.sort();
    root_children.dedup();
    let direct: BTreeSet<(u32, u32)> = children
        .iter()
        .filter(|(from, _)| *from == root)
        .map(|(_, to)| *to)
        .collect();
    let mut items = direct.clone();
    items.extend(children.iter().filter_map(|(from, to)| {
        (direct.contains(from) && exports.contains(from)).then_some(*to)
    }));
    let visible_lists: BTreeSet<(u32, u32)> = items
        .iter()
        .filter(|span| variable_lists.contains(span))
        .copied()
        .collect();
    items.extend(children.into_iter().filter_map(|(from, to)| {
        visible_lists.contains(&from).then_some(to)
    }));
    Ok((items, root_children))
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
            let root = absolute(root)?;
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

fn anchor_file(path: &Path) -> Result<PathBuf, String> {
    let path = absolute(path)?;
    if !path.is_file() {
        return Err(format!("cleave source is not a file: {}", path.display()));
    }
    path.canonicalize()
        .map_err(|error| format!("canonicalize {}: {error}", path.display()))
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(normalize(path));
    }
    let cwd = std::env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    Ok(normalize(&cwd.join(path)))
}

/// DEST need not exist yet, so only its deepest existing ancestor canonicalizes;
/// the tail is re-appended so root-relative stripping still holds.
fn canonical_unborn(path: &Path) -> PathBuf {
    let path = normalize(path);
    let mut tail = Vec::new();
    let mut probe = path.as_path();
    loop {
        if let Ok(real) = probe.canonicalize() {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        let (Some(parent), Some(name)) = (probe.parent(), probe.file_name()) else {
            return path;
        };
        tail.push(name.to_os_string());
        probe = parent;
    }
}

fn within_root(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| format!("{} is outside root {}", path.display(), root.display()))
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
        module == &from.2 || module.starts_with(&format!("{}::", from.2)) || module.starts_with(&format!("{}/", from.2))
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
        if builtin || key == to.2 || to.3.iter().any(|dep| dep == key || dep.replace('-', "_") == key) {
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
