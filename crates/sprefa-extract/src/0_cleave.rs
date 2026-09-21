//! `ryi cleave <SRC>#<ITEM> <DEST>`: one item leaves SRC and lands in DEST,
//! carrying the specifiers it needs, dropping the ones nothing left in SRC
//! references, and respelling every importer. TypeScript only; the corpus read,
//! the soopy stages and the verify rollback are `move`'s, reused as they are.
//! @comment-ok: module header, the seam list every bin arm opens with

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use clap::Parser;
use sprefa_extract::move_stage::{
    content_id, print_previews, run_verify_command, stage_and_commit, state_root, Mirror,
    VerifyJournal,
};
use sprefa_extract::types::{CleaveDrag, CleavePlan, CleaveSpecifier};
use sprefa_extract::{
    directory_path, directory_source, dirname, dispatch, flatten_each, normalize,
    relative_between, replace_action, resolve_project, FamilyMask, FamilyTag, FlatFact, MoveCx,
    ResolveArms, ResolveRequest, Respell, ScipMode, ScipRecords, Span,
};

const PRODUCER: &str = "extract-cleave";

/// The out-of-scope list the help text states, so a caller reads it before the
/// run rather than after.
const SCOPE: &str = "Out of scope, each its own issue: Rust cleave (the `mod` relocation and \
                     visibility widening), cross-language cleave, moving a type together with \
                     its `impl` blocks, and an item whose free names carry a `-` grade (the \
                     names and the `ryi graph --uses` command that answers them print, exit 0).";

/// The extensions the TS arm owns. A cleave stays inside one of them.
const TS_EXTENSIONS: [&str; 6] = ["ts", "tsx", "mts", "cts", "js", "mjs"];

/// cst kinds that carry a top-level declaration's name.
const DECL_KINDS: [&str; 8] = [
    "function_declaration",
    "generator_function_declaration",
    "class_declaration",
    "abstract_class_declaration",
    "interface_declaration",
    "type_alias_declaration",
    "enum_declaration",
    "variable_declarator",
];

/// cst kinds that bind a name inside a declaration.
const BINDER_KINDS: [&str; 3] = [
    "required_parameter",
    "optional_parameter",
    "variable_declarator",
];

/// cst kinds that are an identifier occurrence. `property_identifier` is not
/// one: `a.join` names a member, never the `join` an import bound.
const USE_KINDS: [&str; 3] = [
    "identifier",
    "type_identifier",
    "shorthand_property_identifier",
];

/// Globals no import carries. A free name among them is graded, not missing.
const BUILTINS: [&str; 30] = [
    "Array", "BigInt", "Boolean", "Date", "Error", "Infinity", "JSON", "Map", "Math", "NaN",
    "Number", "Object", "Promise", "Proxy", "Reflect", "RegExp", "Set", "String", "Symbol",
    "WeakMap", "WeakSet", "console", "globalThis", "module", "process", "require", "undefined",
    "this", "super", "arguments",
];

#[derive(Parser)]
#[command(
    name = "ryi cleave",
    about = "move one item out of a file into another, with the imports it needs",
    after_help = SCOPE
)]
pub struct CleaveCli {
    /// `<SRC>#<ITEM>`: the file the item is declared in and its name.
    target: String,
    /// The file it lands in. Created when it does not exist.
    dest: PathBuf,
    /// Corpus root. Defaults to the git root holding SRC.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Soopy state root. Must sit outside the corpus root.
    #[arg(long)]
    state: Option<PathBuf>,
    /// Also pull the same-file private helpers the item references, to a
    /// fixpoint whose pass count is the plan's `drag_iterations`.
    #[arg(long)]
    drag: bool,
    /// Apply the plan to the real tree instead of dry running it.
    #[arg(long)]
    commit: bool,
    /// Run this shell command in the root after `--commit`; a non-zero or
    /// timed-out run rolls every touched file back to its pre-run state.
    #[arg(long = "verify")]
    verify: Option<String>,
    /// Report the SRC spellings this cleave leaves behind in plain text.
    #[arg(long = "text-refs")]
    text_refs: bool,
    /// Close the output with one JSON line carrying the whole plan.
    #[arg(long)]
    json: bool,
}

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    let cli = CleaveCli::try_parse_from(args).map_err(|error| error.to_string())?;
    if cli.verify.is_some() && !cli.commit {
        return Err(
            "--verify runs the command only after --commit; a dry run never runs it".to_string(),
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
        // The disclosure doctrine: an unanswerable state prints what it has
        // plus the command that answers it, and exits 0.
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
        println!("drag {} pass {}", row.name, row.iteration);
    }
    println!("drag fixpoint {} passes", plan.rows.drag_iterations);
    for caller in &plan.rows.callers {
        println!("caller {caller}");
    }

    let stages = plan.stages()?;
    match cli.commit {
        true => {
            let journal = VerifyJournal::capture(
                &plan.root,
                &[],
                &plan.created(),
                &plan.touched(),
            )?;
            for stage in &stages {
                let (id, previews) =
                    stage_and_commit(&plan.root, &state, stage, soopy::Durability::Durable)?;
                print_previews(&previews, "");
                println!("stage {id} committed");
            }
            verify_after_commit(&plan, &state, cli.verify.as_deref(), &journal)?;
        }
        false => {
            let mirror = Mirror::build(&plan.root, &stages)?;
            for stage in &stages {
                let (id, previews) =
                    stage_and_commit(mirror.root(), &state, stage, soopy::Durability::DryRun)?;
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
            std::process::exit(3);
        }
    }
    Ok(())
}

/// Lines naming the item in files no plan edit covers. Rewriting text carriers
/// is out of scope for `move` and for this verb; the scan only names them.
fn report_text_refs(plan: &Plan) {
    let edited: BTreeSet<String> = plan.touched().into_iter().collect();
    for rel in plan.cx.files() {
        if is_ts(rel) || edited.contains(rel) {
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
    rows: CleavePlan,
    source: FileView,
    /// None when DEST does not exist yet and this run creates it.
    dest_view: Option<FileView>,
    /// The item text, and each dragged helper's, in SRC byte order.
    moving_text: Vec<String>,
    callers: Vec<FileView>,
}

impl Plan {
    fn build(cli: &CleaveCli) -> Result<Self, String> {
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
        if !is_ts(&src) || !is_ts(&dest) {
            return Err(format!(
                "cleave is the TypeScript arm; {src} -> {dest} is not two {} files",
                TS_EXTENSIONS.join("/")
            ));
        }

        let source = FileView::open(&cx, &src)?;
        let item_decl = source
            .decls
            .iter()
            .find(|decl| decl.name == item)
            .ok_or_else(|| format!("{src} declares no {item}"))?
            .clone();

        // The free names the item reaches that no specifier, no SRC
        // declaration and no local binding answers.
        let unresolved = source.ungraded(&[item_decl.span]);
        if !unresolved.is_empty() {
            return Ok(Plan {
                root,
                cx,
                rows: CleavePlan {
                    src,
                    dest,
                    item,
                    item_span: item_decl.span,
                    drag_iterations: 1,
                    unresolved,
                    ..CleavePlan::default()
                },
                source,
                dest_view: None,
                moving_text: Vec::new(),
                callers: Vec::new(),
            });
        }

        let (dragged, drag_iterations) = source.drag_fixpoint(&item_decl, cli.drag);
        let mut moving: Vec<Span> = vec![item_decl.span];
        moving.extend(dragged.iter().map(|row| row.span));

        let dest_view = match cx.contains(&dest) {
            true => Some(FileView::open(&cx, &dest)?),
            false => None,
        };
        let dest_dir = dirname(&dest).to_string();
        let src_dir = dirname(&src).to_string();
        let carried: BTreeSet<(String, String)> = dest_view
            .iter()
            .flat_map(|view| view.imports.iter())
            .flat_map(|statement| {
                let module = statement.module.clone();
                statement
                    .names
                    .iter()
                    .map(move |(name, _)| (name.clone(), module.clone()))
            })
            .collect();

        let mut travelling = Vec::new();
        let mut orphans = Vec::new();
        for statement in &source.imports {
            for (name, _) in &statement.names {
                let dest_module = match statement.module.starts_with('.') {
                    // A relative specifier is re-aimed at DEST's directory; a
                    // package path anchors to the root and travels as written.
                    true => respell_relative(&src_dir, &dest_dir, &statement.module),
                    false => statement.module.clone(),
                };
                let kind = match (
                    carried.contains(&(name.clone(), dest_module.clone())),
                    statement.module.starts_with('.'),
                ) {
                    (true, _) => "carried",
                    (false, true) => "relative",
                    (false, false) => "package",
                };
                let row = CleaveSpecifier {
                    name: name.clone(),
                    module: statement.module.clone(),
                    dest_module,
                    span: statement.span,
                    kind,
                };
                if source.refs_in(name, &moving) > 0 {
                    travelling.push(row.clone());
                }
                if source.refs_outside(name, &moving) == 0 {
                    orphans.push(row);
                }
            }
        }

        let callers = callers_of(&cx, &root, &src, &item)?;
        let mut views = Vec::with_capacity(callers.len());
        for caller in &callers {
            views.push(FileView::open(&cx, caller)?);
        }
        moving.sort_by_key(|span| span.start);
        let moving_text = moving
            .iter()
            .map(|span| source.slice(*span).to_string())
            .collect();
        Ok(Plan {
            root,
            cx,
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
            dest_view,
            moving_text,
            callers: views,
        })
    }

    /// Every byte this cleave rewrites, as one `Respell` per span, in (file,
    /// offset) order. Nothing here touches the tree; the stages do.
    fn respells(&self) -> Vec<Respell> {
        let mut out = self.source_respells();
        out.extend(self.dest_respells());
        out.extend(self.caller_respells());
        out.sort_by(|left, right| {
            left.file
                .cmp(&right.file)
                .then(left.span.start.cmp(&right.span.start))
        });
        out
    }

    /// SRC loses the moving spans and every specifier nothing left references.
    fn source_respells(&self) -> Vec<Respell> {
        let mut cuts: Vec<Span> = Vec::new();
        cuts.push(self.rows.item_span);
        cuts.extend(self.rows.dragged.iter().map(|row| row.span));
        let orphaned: BTreeSet<&str> = self
            .rows
            .orphans
            .iter()
            .map(|row| row.name.as_str())
            .collect();
        let mut trims: Vec<Respell> = Vec::new();
        for statement in &self.source.imports {
            let kept: Vec<&(String, Span)> = statement
                .names
                .iter()
                .filter(|(name, _)| !orphaned.contains(name.as_str()))
                .collect();
            if kept.len() == statement.names.len() {
                continue;
            }
            if kept.is_empty() {
                cuts.push(statement.span);
                continue;
            }
            for (index, (name, span)) in statement.names.iter().enumerate() {
                if !orphaned.contains(name.as_str()) {
                    continue;
                }
                trims.push(Respell {
                    file: self.rows.src.clone(),
                    span: separator_cut(statement, index, *span),
                    text: String::new(),
                    receipt: None,
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
        out.extend(trims);
        out
    }

    /// DEST gains the travelling specifiers it does not already carry, then the
    /// moving text. A DEST this run creates gets both as its whole content.
    fn dest_respells(&self) -> Vec<Respell> {
        let Some(view) = self.dest_view.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let imports = self.import_block();
        if !imports.is_empty() {
            let at = view
                .imports
                .iter()
                .map(|statement| statement.span.end())
                .max()
                .unwrap_or(0);
            out.push(Respell {
                file: self.rows.dest.clone(),
                span: Span::anchor(at),
                text: imports,
                receipt: None,
            });
        }
        out.push(Respell {
            file: self.rows.dest.clone(),
            span: Span::anchor(view.text.len() as u32),
            text: format!("\n{}", self.moving_text.join("\n")),
            receipt: None,
        });
        out
    }

    /// The import lines DEST gains, one per module, in SRC order.
    fn import_block(&self) -> String {
        let mut per_module: Vec<(String, Vec<String>)> = Vec::new();
        for row in &self.rows.travelling {
            if row.kind == "carried" {
                continue;
            }
            match per_module
                .iter_mut()
                .find(|(module, _)| *module == row.dest_module)
            {
                Some((_, names)) => names.push(row.name.clone()),
                None => per_module.push((row.dest_module.clone(), vec![row.name.clone()])),
            }
        }
        per_module
            .into_iter()
            .map(|(module, names)| format!("import {{ {} }} from \"{module}\";\n", names.join(", ")))
            .collect()
    }

    /// Every importer of `SRC#ITEM` re-aimed at DEST. A specifier binding other
    /// names is split; one binding only the item has its module rewritten.
    fn caller_respells(&self) -> Vec<Respell> {
        let mut out = Vec::new();
        for (rel, view) in self.rows.callers.iter().zip(&self.callers) {
            let spelling = spell_relative(dirname(rel), &self.rows.dest);
            for statement in &view.imports {
                if !aims_at(rel, &statement.module, &self.rows.src) {
                    continue;
                }
                let Some(index) = statement
                    .names
                    .iter()
                    .position(|(name, _)| *name == self.rows.item)
                else {
                    continue;
                };
                let quote = view.slice(statement.module_span).chars().next().unwrap_or('"');
                if statement.names.len() == 1 {
                    out.push(Respell {
                        file: rel.clone(),
                        span: statement.module_span,
                        text: format!("{quote}{spelling}{quote}"),
                        receipt: Some(format!("caller {rel}: {} -> {spelling}", statement.module)),
                    });
                    continue;
                }
                let (_, span) = statement.names[index];
                out.push(Respell {
                    file: rel.clone(),
                    span: separator_cut(statement, index, span),
                    text: String::new(),
                    receipt: None,
                });
                out.push(Respell {
                    file: rel.clone(),
                    span: Span::anchor(statement.span.end()),
                    text: format!(
                        "import {{ {} }} from {quote}{spelling}{quote};\n",
                        self.rows.item
                    ),
                    receipt: Some(format!("caller {rel}: {} split", self.rows.item)),
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
        if self.dest_view.is_none() {
            stages.push(vec![soopy::SourceAction::Create {
                path: directory_path(&self.rows.dest),
                bytes: format!("{}\n{}", self.import_block(), self.moving_text.join("\n"))
                    .into_bytes(),
            }]);
        }
        Ok(stages)
    }

    /// Every path a stage reads or writes, so a verify rollback can restore it.
    fn touched(&self) -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        out.insert(self.rows.src.clone());
        if self.dest_view.is_some() {
            out.insert(self.rows.dest.clone());
        }
        out.extend(self.rows.callers.iter().cloned());
        out.into_iter().collect()
    }

    /// The paths this run creates. A rollback deletes them, as it does a shim.
    fn created(&self) -> Vec<String> {
        match self.dest_view.is_none() {
            true => vec![self.rows.dest.clone()],
            false => Vec::new(),
        }
    }
}

/// The bytes a named import loses when one of its names goes: the name plus the
/// separator that joined it to the one after, or to the one before when last.
fn separator_cut(statement: &ImportStatement, index: usize, span: Span) -> Span {
    if let Some((_, next)) = statement.names.get(index + 1) {
        return Span {
            start: span.start,
            len: next.start - span.start,
        };
    }
    match index.checked_sub(1).and_then(|at| statement.names.get(at)) {
        Some((_, previous)) => Span {
            start: previous.end(),
            len: span.end() - previous.end(),
        },
        None => span,
    }
}

/// Whether `module`, as `caller` writes it, names `src`.
fn aims_at(caller: &str, module: &str, src: &str) -> bool {
    if !module.starts_with('.') {
        return false;
    }
    let target = sprefa_extract::join_rel(dirname(caller), module);
    let src = drop_extension(src);
    target == src || target == format!("{src}/index")
}

/// Line-aligned cuts merged, then widened over the blank lines they orphan:
/// forward always, and backward when the cut now runs to the end of the file.
/// Widening can make two cuts meet, so the merge runs again after it.
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

/// Every file importing `src#item`, in path order. `resolved_import` carries
/// the importer and the declaration it reached, so one resolve answers it.
fn callers_of(cx: &MoveCx, root: &Path, src: &str, item: &str) -> Result<Vec<String>, String> {
    let paths: Vec<PathBuf> = cx
        .files()
        .iter()
        .filter(|rel| is_ts(rel))
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
    let facts = resolve_project(&request).map_err(|error| format!("resolve {root:?}: {error}"))?;
    let mut callers = BTreeSet::new();
    for fact in &facts {
        let FlatFact::ResolvedImportRow {
            src_path,
            target_path,
            target_name,
            ..
        } = fact
        else {
            continue;
        };
        if target_name.as_deref() != Some(item) {
            continue;
        }
        let Some(target) = rel_of(root, target_path) else {
            continue;
        };
        if target != src {
            continue;
        }
        if let Some(importer) = rel_of(root, src_path) {
            if importer != src {
                callers.insert(importer);
            }
        }
    }
    Ok(callers.into_iter().collect())
}

/// A resolve echoes the path spellings it was given, which are absolute here.
fn rel_of(root: &Path, path: &str) -> Option<String> {
    Path::new(path)
        .strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

// ── the corpus read ─────────────────────────────────────────────────────────

/// One TS file read through the cst plane: its text, its top-level statements,
/// and every identifier occurrence outside an import statement.
struct FileView {
    text: String,
    imports: Vec<ImportStatement>,
    decls: Vec<Decl>,
    /// Identifier occurrences that are neither an import binding nor a
    /// declaring name, in byte order.
    uses: Vec<(String, Span)>,
    /// Names a parameter or a local declarator binds, by binding span.
    bindings: Vec<(String, Span)>,
    /// Call sites the file writes, by span, with the callee as written and
    /// whether it was reached through a receiver path.
    sites: Vec<(String, Span, bool)>,
}

/// One import statement, line aligned so a deletion takes the whole line.
struct ImportStatement {
    span: Span,
    module: String,
    /// The module string literal, quotes included.
    module_span: Span,
    /// Each bound name and the specifier span that binds it.
    names: Vec<(String, Span)>,
}

/// One top-level declaration, line aligned for the same reason.
#[derive(Clone)]
struct Decl {
    name: String,
    span: Span,
    exported: bool,
}

impl FileView {
    fn open(cx: &MoveCx, rel: &str) -> Result<Self, String> {
        let bytes = cx.read(rel).ok_or_else(|| format!("read {rel}"))?;
        let text = String::from_utf8(bytes.clone())
            .map_err(|error| format!("{rel} is not UTF-8: {error}"))?;
        let mask = FamilyMask {
            cst: true,
            call: true,
            ..FamilyMask::NONE
        };
        let out = dispatch(rel, &bytes, mask).ok_or_else(|| format!("no Source owns {rel}"))?;
        let mut facts = Vec::new();
        flatten_each(&out, None, &mut |fact: FlatFact| -> Result<(), ()> {
            facts.push(fact);
            Ok(())
        })
        .map_err(|_| format!("flatten {rel}"))?;
        Ok(Self::from_facts(text, &facts))
    }

    fn from_facts(text: String, facts: &[FlatFact]) -> Self {
        let mut nodes: Vec<(&str, Option<&str>, Span)> = Vec::new();
        let mut program = Span::empty();
        let mut children: Vec<Span> = Vec::new();
        let mut specifiers: Vec<(String, Option<String>, Span)> = Vec::new();
        let mut sites: Vec<(String, Span, bool)> = Vec::new();
        for fact in facts {
            match fact {
                FlatFact::Node {
                    family: FamilyTag::Cst,
                    span,
                    kind,
                    name,
                    ..
                } => {
                    let span = Span {
                        start: span.start,
                        len: span.end - span.start,
                    };
                    if kind == "program" {
                        program = span;
                    }
                    nodes.push((kind.as_str(), name.as_deref(), span));
                }
                FlatFact::Edge {
                    family: FamilyTag::Cst,
                    kind,
                    from,
                    to,
                    ..
                } if kind == "child" && from.start == program.start && from.end == program.end() => {
                    children.push(Span {
                        start: to.start,
                        len: to.end - to.start,
                    });
                }
                FlatFact::Specifier {
                    span,
                    name,
                    module,
                    ..
                } => specifiers.push((
                    name.clone(),
                    module.clone(),
                    Span {
                        start: span.start,
                        len: span.end - span.start,
                    },
                )),
                FlatFact::Site {
                    span,
                    callee,
                    callee_path,
                    ..
                } => sites.push((
                    callee.clone(),
                    Span {
                        start: span.start,
                        len: span.end - span.start,
                    },
                    callee_path.is_some(),
                )),
                _ => {}
            }
        }
        children.sort_by_key(|span| span.start);

        let mut imports = Vec::new();
        let mut decls = Vec::new();
        for child in &children {
            let child = line_span(&text, *child);
            let kinds: Vec<&(&str, Option<&str>, Span)> = nodes
                .iter()
                .filter(|(_, _, span)| inside(*span, child))
                .collect();
            let is_import = kinds
                .iter()
                .any(|(kind, _, span)| *kind == "import_statement" && span.start == child.start
                    || *kind == "import_statement" && inside(*span, child));
            if is_import {
                let module_span = kinds
                    .iter()
                    .find(|(kind, _, _)| *kind == "string")
                    .map(|(_, _, span)| *span)
                    .unwrap_or(child);
                let module = text
                    .get(module_span.start as usize..module_span.end() as usize)
                    .map(bare)
                    .unwrap_or_default()
                    .to_string();
                let names = specifiers
                    .iter()
                    .filter(|(_, _, span)| inside(*span, child))
                    .map(|(name, _, span)| (name.clone(), *span))
                    .collect();
                imports.push(ImportStatement {
                    span: child,
                    module,
                    module_span,
                    names,
                });
                continue;
            }
            let Some((_, Some(name), _)) = kinds
                .iter()
                .find(|(kind, name, _)| DECL_KINDS.contains(kind) && name.is_some())
            else {
                continue;
            };
            decls.push(Decl {
                name: (*name).to_string(),
                span: child,
                exported: kinds
                    .iter()
                    .any(|(kind, _, span)| *kind == "export_statement" && span.start == child.start),
            });
        }

        // A binder's bound name is the leftmost identifier inside it; a
        // declaration's own name is the leftmost identifier carrying it.
        let identifiers: Vec<(&str, Span)> = nodes
            .iter()
            .filter(|(kind, name, _)| USE_KINDS.contains(kind) && name.is_some())
            .map(|(_, name, span)| (name.unwrap(), *span))
            .collect();
        let leftmost = |scope: Span| -> Option<(&str, Span)> {
            identifiers
                .iter()
                .filter(|(_, span)| inside(*span, scope))
                .min_by_key(|(_, span)| span.start)
                .copied()
        };
        let mut bindings: Vec<(String, Span)> = Vec::new();
        for (kind, _, span) in &nodes {
            if !BINDER_KINDS.contains(kind) {
                continue;
            }
            if let Some((name, at)) = leftmost(*span) {
                bindings.push((name.to_string(), at));
            }
        }
        let mut declaring: BTreeSet<u32> = BTreeSet::new();
        for decl in &decls {
            if let Some((name, at)) = leftmost(decl.span) {
                if name == decl.name {
                    declaring.insert(at.start);
                }
            }
        }
        let uses = identifiers
            .iter()
            .filter(|(_, span)| !imports.iter().any(|row| inside(*span, row.span)))
            .filter(|(_, span)| !declaring.contains(&span.start))
            .map(|(name, span)| ((*name).to_string(), *span))
            .collect();

        FileView {
            text,
            imports,
            decls,
            uses,
            bindings,
            sites,
        }
    }

    /// The file's bytes under `span`, empty when the span is off the end.
    fn slice(&self, span: Span) -> &str {
        self.text
            .get(span.start as usize..span.end() as usize)
            .unwrap_or_default()
    }

    /// Occurrences of `name` inside any of `spans`.
    fn refs_in(&self, name: &str, spans: &[Span]) -> usize {
        self.uses
            .iter()
            .filter(|(used, span)| used == name && spans.iter().any(|scope| inside(*span, *scope)))
            .count()
    }

    /// Occurrences of `name` outside every one of `spans`.
    fn refs_outside(&self, name: &str, spans: &[Span]) -> usize {
        self.uses
            .iter()
            .filter(|(used, span)| used == name && !spans.iter().any(|scope| inside(*span, *scope)))
            .count()
    }

    /// Names bound by a parameter or a local declarator inside `spans`.
    fn locals_in(&self, spans: &[Span]) -> BTreeSet<&str> {
        self.bindings
            .iter()
            .filter(|(_, span)| spans.iter().any(|scope| inside(*span, *scope)))
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Free names inside `spans` that no specifier, declaration, local binding
    /// or builtin answers. A call through a receiver is a member access.
    fn ungraded(&self, spans: &[Span]) -> Vec<String> {
        let imported: BTreeSet<&str> = self
            .imports
            .iter()
            .flat_map(|row| row.names.iter())
            .map(|(name, _)| name.as_str())
            .collect();
        let declared: BTreeSet<&str> = self.decls.iter().map(|decl| decl.name.as_str()).collect();
        let locals = self.locals_in(spans);
        let mut out: BTreeSet<String> = BTreeSet::new();
        for (callee, span, through_receiver) in &self.sites {
            if *through_receiver || !spans.iter().any(|scope| inside(*span, *scope)) {
                continue;
            }
            if imported.contains(callee.as_str())
                || declared.contains(callee.as_str())
                || locals.contains(callee.as_str())
                || BUILTINS.contains(&callee.as_str())
            {
                continue;
            }
            out.insert(callee.clone());
        }
        out.into_iter().collect()
    }

    /// The private helpers the item drags along, with the fixpoint pass count.
    /// Pass 1 reads the item; each later pass reads the pass before it.
    fn drag_fixpoint(&self, item: &Decl, drag: bool) -> (Vec<CleaveDrag>, u32) {
        let mut moving = vec![item.span];
        let mut claimed: Vec<CleaveDrag> = Vec::new();
        if !drag {
            return (claimed, 1);
        }
        let mut iterations = 1u32;
        loop {
            let found = self.drag_candidates(item, &moving);
            if found.is_empty() {
                return (claimed, iterations);
            }
            for decl in found {
                moving.push(decl.span);
                claimed.push(CleaveDrag {
                    name: decl.name,
                    span: decl.span,
                    iteration: iterations,
                });
            }
            // The pass that claims nothing new does not count: a run that drags
            // nothing reports one pass, not two.
            let next = self.drag_candidates(item, &moving);
            if next.is_empty() {
                return (claimed, iterations);
            }
            iterations += 1;
        }
    }

    /// Declarations the moving set references that SRC no longer needs: not
    /// exported, not the item, and with no reference left outside the set.
    fn drag_candidates(&self, item: &Decl, moving: &[Span]) -> Vec<Decl> {
        self.decls
            .iter()
            .filter(|decl| !decl.exported && decl.name != item.name)
            .filter(|decl| !moving.iter().any(|span| span.start == decl.span.start))
            .filter(|decl| self.refs_in(&decl.name, moving) > 0)
            .filter(|decl| {
                let mut scope = moving.to_vec();
                scope.push(decl.span);
                self.refs_outside(&decl.name, &scope) == 0
            })
            .cloned()
            .collect()
    }
}

/// `span` widened to whole lines: back to the start of its first line, forward
/// through the newline closing its last.
fn line_span(text: &str, span: Span) -> Span {
    let bytes = text.as_bytes();
    let mut start = span.start as usize;
    while start > 0 && bytes[start - 1] != b'\n' {
        start -= 1;
    }
    let mut end = span.end() as usize;
    while end < bytes.len() && bytes[end - 1] != b'\n' {
        end += 1;
    }
    Span {
        start: start as u32,
        len: (end - start) as u32,
    }
}

/// Whether `inner` sits inside `outer`, endpoints included.
fn inside(inner: Span, outer: Span) -> bool {
    inner.start >= outer.start && inner.end() <= outer.end()
}

/// A string literal without its quotes.
fn bare(literal: &str) -> &str {
    let bytes = literal.as_bytes();
    let quoted = bytes.len() >= 2
        && matches!(bytes[0], b'\'' | b'"' | b'`')
        && bytes[bytes.len() - 1] == bytes[0];
    match quoted {
        true => &literal[1..literal.len() - 1],
        false => literal,
    }
}

/// A relative specifier SRC writes, re-aimed at DEST's directory.
fn respell_relative(src_dir: &str, dest_dir: &str, module: &str) -> String {
    let target = sprefa_extract::join_rel(src_dir, module);
    spell_relative(dest_dir, &target)
}

/// `from_dir` -> `target` as TypeScript spells it: extensionless, `./` led when
/// it does not climb.
fn spell_relative(from_dir: &str, target: &str) -> String {
    let target = drop_extension(target);
    let relative = relative_between(from_dir, &target);
    match relative.is_empty() {
        true => ".".to_string(),
        false if relative.starts_with("..") => relative,
        false => format!("./{relative}"),
    }
}

/// A TS path without its extension; anything else unchanged.
fn drop_extension(rel: &str) -> String {
    let Some((head, extension)) = rel.rsplit_once('.') else {
        return rel.to_string();
    };
    match TS_EXTENSIONS.contains(&extension) {
        true => head.to_string(),
        false => rel.to_string(),
    }
}

fn is_ts(rel: &str) -> bool {
    rel.rsplit_once('.')
        .is_some_and(|(_, extension)| TS_EXTENSIONS.contains(&extension))
}

// ── the request ─────────────────────────────────────────────────────────────

/// `<SRC>#<ITEM>` split at the last `#`, so a path holding one still parses.
fn split_target(target: &str) -> Result<(PathBuf, String), String> {
    let (src, item) = target
        .rsplit_once('#')
        .ok_or_else(|| format!("a cleave target is `<SRC>#<ITEM>`, not {target}"))?;
    if src.is_empty() || item.is_empty() {
        return Err(format!("a cleave target is `<SRC>#<ITEM>`, not {target}"));
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
