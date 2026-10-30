use crate::cli::QueryArgs;

#[path = "0a_query_split.rs"]
mod split;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use sprefa_extract::{content_id_of, extract_pool, query_tree_sitter_spans_with, RyiLang};

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

pub fn run(cli: QueryArgs) -> Result<()> {
    run_to(cli, Box::new(std::io::stdout()), None)
}

/// `cancel` is the daemon request's flag: set, the run stops and exits 3.
pub fn run_to(
    cli: QueryArgs,
    writer: Box<dyn Write + Send>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<()> {
    // A digest names a blob, so its path need not exist in the worktree.
    let paths = match &cli.digest {
        Some(_) if cli.inputs.paths.len() == 1 => vec![cli.inputs.paths[0].clone()],
        Some(_) => return Err("--digest names one blob; pass exactly one input".into()),
        None => tracing::debug_span!(target: "sprefa_extract::query", "query.expand")
            .in_scope(|| crate::inputs::expand(&cli.inputs))?,
    };
    if paths.is_empty() {
        return Err("query: no inputs; pass files, directories, globs, - or --entry".into());
    }
    // Every `--query` text, then every `--scm` file; `label` names a pattern in rows.
    let mut patterns: Vec<Pattern> = cli
        .query
        .iter()
        .map(|text| Pattern { label: text.clone(), text: text.clone() })
        .collect();
    for file in &cli.scm {
        let text = std::fs::read_to_string(sprefa_extract::io_path(file))
            .map_err(|error| format!("--scm '{}': {error}", file.display()))?;
        patterns.push(Pattern { label: file.to_string_lossy().into_owned(), text });
    }
    match (&cli.scmpp, patterns.is_empty()) {
        (Some(file), true) => {
            let text = std::fs::read_to_string(sprefa_extract::io_path(file))
                .map_err(|error| format!("--scmpp '{}': {error}", file.display()))?;
            return run_scmpp(&cli, &text, &paths, writer, cancel, false);
        }
        (Some(_), false) => return Err("query: pass --query/--scm or --scmpp, not both".into()),
        (None, true) => return Err("query: pass --query TEXT, --scm FILE or --scmpp FILE".into()),
        (None, false) => {}
    }
    // One pattern keeps the historical row shape; several name their source per row.
    let labelled = patterns.len() > 1;
    // 3+ unanchored siblings: the scm++ split answers the same rows without tree-sitter's
    // combinatorial walk; `--sqlite` keeps the capture-row store, so it stays on the flat engine.
    if cli.store.sqlite.is_none() {
        let first = paths.first().map(|path| language_for(cli.lang.as_deref(), &path.to_string_lossy()));
        if let Some(Ok(language)) = first {
            if let Some(grammar) = RyiLang::parse_name(&language) {
                let grammar = grammar.tree_sitter_language();
                if let Some(pattern) = patterns.iter().find(|pattern| split::splits(&pattern.text, &grammar)) {
                    if labelled {
                        return Err(format!(
                            "query: {} needs the scm++ split (3+ unanchored siblings); run it alone",
                            pattern.label
                        )
                        .into());
                    }
                    return run_scmpp(&cli, &pattern.text, &paths, writer, cancel, true);
                }
            }
        }
    }
    let mut output = crate::sqlite::Output::with_writer(cli.store.sqlite.as_deref(), writer, true)
        .map_err(|error| error.to_string())?;
    // Per language, every pattern that grammar accepts merges into one query:
    // one cursor walk per file answers them all. `owners` maps a merged
    // pattern index back to its source: (first merged index, pattern).
    let mut compiled = std::collections::HashMap::<String, Option<Merged>>::new();
    let mut skipped = std::collections::BTreeMap::<String, usize>::new();
    let mut jobs = Vec::<(&std::path::PathBuf, String)>::with_capacity(paths.len());
    // A file with no language ends the stream where it sits: the files before
    // it still answer, then the error.
    let mut late_error = None;
    for path in &paths {
        let name = path.to_string_lossy();
        let language = match language_for(cli.lang.as_deref(), &name) {
            Ok(language) => language,
            Err(error) => {
                late_error = Some(error);
                break;
            }
        };
        if !compiled.contains_key(&language) {
            let _compile =
                tracing::debug_span!(target: "sprefa_extract::query", "query.compile", language).entered();
            let grammar = RyiLang::parse_name(&language)
                .ok_or_else(|| format!("unknown lang '{language}'"))?
                .tree_sitter_language();
            // All patterns at once first; only a failure compiles them one by one,
            // to drop the ones this grammar lacks node types for and to name a bad one.
            let entry = match Merged::build(&grammar, &patterns, (0..patterns.len()).collect()) {
                Ok(merged) => Some(merged),
                Err(_) => {
                    let mut kept = Vec::new();
                    for (index, pattern) in patterns.iter().enumerate() {
                        match hafley_scm::build(&grammar, &pattern.text) {
                            Ok(_) => kept.push(index),
                            Err(hafley_scm::QueryExtError::Parse(error))
                                if error.kind == tree_sitter::QueryErrorKind::NodeType => {}
                            Err(error) if labelled => {
                                return Err(format!("query ({language}) {}: {error}", pattern.label).into())
                            }
                            Err(error) => return Err(format!("query ({language}): {error}").into()),
                        }
                    }
                    match kept.is_empty() {
                        true => None,
                        false => Some(
                            Merged::build(&grammar, &patterns, kept)
                                .map_err(|error| format!("query ({language}): merged patterns: {error}"))?,
                        ),
                    }
                }
            };
            compiled.insert(language.clone(), entry);
        }
        if compiled[&language].is_none() {
            *skipped.entry(language).or_default() += 1;
            continue;
        }
        jobs.push((path, language));
    }
    // Workers pull file indexes in order from one counter, so a slow file never
    // holds a batch; each worker reads, parses, matches and renders its rows.
    // This thread only restores input order and writes. The request root is
    // thread-local, so each worker reads through the caller's root.
    let io_root = sprefa_extract::request_io_root();
    let stdout = output.database.is_none();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let work = |parser: &mut tree_sitter::Parser, index: usize| -> Result<Done, String> {
        let (path, language) = &jobs[index];
        let _file = tracing::debug_span!(target: "sprefa_extract::query", "query.file", index).entered();
        let merged = compiled[language].as_ref().expect("compiled language");
        let read = || source_bytes(path, cli.digest.as_deref());
        let bytes = tracing::debug_span!(target: "sprefa_extract::query", "query.read").in_scope(|| match &io_root {
            Some(root) => sprefa_extract::with_io_root(root.clone(), read),
            None => read(),
        })?;
        let mut matches = query_tree_sitter_spans_with(&bytes, &merged.grammar, &merged.query, parser)?;
        // Rows group by source pattern in argument order, document order within.
        let owner = |found: &sprefa_extract::TreeSitterSpannedMatch| merged.owner[found.pattern as usize];
        matches.sort_by_key(owner);
        let rows = matches.into_iter().map(|found| (owner(&found), found)).collect::<Vec<_>>();
        let _render = tracing::debug_span!(target: "sprefa_extract::query", "query.render").entered();
        let lines = match stdout {
            true => {
                let name = path.to_string_lossy();
                let mut block = Vec::new();
                for (pattern, found) in &rows {
                    render_row(&mut block, &name, labelled.then_some(patterns[*pattern].label.as_str()), found)
                        .map_err(|error| format!("query output: {error}"))?;
                }
                Rendered::Lines(block)
            }
            false => Rendered::Rows(rows),
        };
        Ok(Done { content_id: content_id_of(&bytes).to_string(), rendered: lines })
    };
    let (sender, receiver) = std::sync::mpsc::sync_channel::<(usize, Result<Done, String>)>(256);
    std::thread::scope(|scope| -> Result<()> {
        scope.spawn(|| {
            let sender = sender;
            extract_pool().broadcast(|_| {
                let mut parser = tree_sitter::Parser::new();
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= jobs.len() || stop.load(Ordering::Relaxed) {
                        break;
                    }
                    if sender.send((index, work(&mut parser, index))).is_err() {
                        break;
                    }
                }
            });
        });
        let written = (|| -> Result<()> {
            let mut pending = std::collections::BTreeMap::new();
            let mut want = 0;
            loop {
                let received = tracing::debug_span!(target: "sprefa_extract::query", "query.wait")
                    .in_scope(|| receiver.recv());
                let Ok((index, result)) = received else { break };
                if cancel.as_ref().is_some_and(|cancel| cancel.load(Ordering::Acquire)) {
                    return Err(crate::RyiExit::new(3, "query: cancelled").into());
                }
                pending.insert(index, result);
                while let Some(result) = pending.remove(&want) {
                    want += 1;
                    let _write = tracing::debug_span!(target: "sprefa_extract::query", "query.write").entered();
                    let name = jobs[want - 1].0.to_string_lossy();
                    let done = result?;
                    match done.rendered {
                        Rendered::Lines(block) => {
                            if !block.is_empty() {
                                output.stdout_block(&block).map_err(|error| error.to_string())?;
                            }
                        }
                        Rendered::Rows(rows) => {
                            let database = output.database.as_mut().expect("sqlite output");
                            database
                                .source(&name, done.content_id)
                                .map_err(|error| error.to_string())?;
                            for (pattern, found) in &rows {
                                for capture in &found.captures {
                                    // Typed and `record`-first: the binder batches rows
                                    // across files instead of one insert per file.
                                    database
                                        .bind_row(&CaptureRow {
                                            record: "capture",
                                            query: &patterns[*pattern].text,
                                            capture: &capture.label,
                                            text: &capture.text,
                                            start: capture.start,
                                            end: capture.end,
                                            match_start: found.start,
                                            match_end: found.end,
                                        })
                                        .map_err(|error| error.to_string())?;
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        })();
        // An early return stops the workers: they see `stop` or a closed channel.
        stop.store(true, Ordering::Relaxed);
        drop(receiver);
        written
    })?;
    if let Some(error) = late_error {
        return Err(error.into());
    }
    if !skipped.is_empty() {
        crate::ops::print_diagnostic(format_args!(
            "query: skipped {} files whose grammar lacks a query node type ({})",
            skipped.values().sum::<usize>(),
            skipped.iter().map(|(language, count)| format!("{language}: {count}"))
                .collect::<Vec<_>>().join(", "),
        ));
    }
    output.finish()
}

/// The kept patterns of one language, merged into one query; `owner[p]` is
/// the source pattern of merged pattern `p`.
struct Merged {
    grammar: tree_sitter::Language,
    query: hafley_scm::QueryExt,
    owner: Vec<usize>,
}

impl Merged {
    /// One query from `kept` patterns' texts; a merged pattern's start byte
    /// names the source text it came from.
    fn build(
        grammar: &tree_sitter::Language,
        patterns: &[Pattern],
        kept: Vec<usize>,
    ) -> Result<Self, hafley_scm::QueryExtError> {
        let mut text = String::new();
        let mut starts = Vec::with_capacity(kept.len());
        for index in kept {
            starts.push((text.len(), index));
            text.push_str(&patterns[index].text);
            text.push('\n');
        }
        let query = hafley_scm::build(grammar, &text)?;
        let owner = (0..query.user.pattern_count())
            .map(|pattern| {
                let byte = query.user.start_byte_for_pattern(pattern);
                starts[starts.partition_point(|(start, _)| *start <= byte) - 1].1
            })
            .collect();
        Ok(Self { grammar: grammar.clone(), query, owner })
    }
}

/// One file's answer: stdout lines rendered by the worker, or capture rows
/// tagged with their source pattern for the SQLite store.
struct Done {
    content_id: String,
    rendered: Rendered,
}

enum Rendered {
    /// The file's JSONL rows, each ending in `\n`.
    Lines(Vec<u8>),
    Rows(Vec<(usize, sprefa_extract::TreeSitterSpannedMatch)>),
}

/// One `--query` row appended to `block` with its newline: path, the source
/// label when several patterns ran, each capture's text, and the match's
/// lines, keys sorted. `line`/`end_line` win over a capture of that name, a
/// capture wins over `path`/`query`.
fn render_row(
    block: &mut Vec<u8>,
    name: &str,
    label: Option<&str>,
    found: &sprefa_extract::TreeSitterSpannedMatch,
) -> serde_json::Result<()> {
    enum Cell<'a> {
        Text(&'a str),
        Texts(Vec<(u32, &'a str)>),
        Number(u32),
    }
    let mut row = std::collections::BTreeMap::<&str, Cell>::new();
    row.insert("path", Cell::Text(name));
    if let Some(label) = label {
        row.insert("query", Cell::Text(label));
    }
    // A capture with one node is its text; a capture bound to several nodes
    // (`*`, `+`, or a repeated name) is an array of their texts in document order.
    let mut captures = std::collections::BTreeMap::<&str, Vec<(u32, &str)>>::new();
    for capture in &found.captures {
        captures.entry(capture.label.as_str()).or_default().push((capture.start, capture.text.as_str()));
    }
    for (label, mut texts) in captures {
        let cell = match texts.len() {
            1 => Cell::Text(texts[0].1),
            _ => {
                texts.sort_by_key(|(start, _)| *start);
                Cell::Texts(texts)
            }
        };
        row.insert(label, cell);
    }
    row.insert("line", Cell::Number(found.line));
    row.insert("end_line", Cell::Number(found.end_line));
    block.push(b'{');
    for (index, (key, cell)) in row.iter().enumerate() {
        if index > 0 {
            block.push(b',');
        }
        serde_json::to_writer(&mut *block, key)?;
        block.push(b':');
        match cell {
            Cell::Text(text) => serde_json::to_writer(&mut *block, text)?,
            Cell::Number(number) => serde_json::to_writer(&mut *block, number)?,
            Cell::Texts(texts) => {
                block.push(b'[');
                for (position, (_, text)) in texts.iter().enumerate() {
                    if position > 0 {
                        block.push(b',');
                    }
                    serde_json::to_writer(&mut *block, text)?;
                }
                block.push(b']');
            }
        }
    }
    block.extend_from_slice(b"}\n");
    Ok(())
}

/// One query source: `label` names it in rows (the `--scm` path, or the
/// query text itself), `text` is what compiles.
struct Pattern {
    label: String,
    text: String,
}

/// `--lang` names a grammar family: `typescript` takes each file's own grammar from its
/// extension, so a `.tsx` file parses with tsx and its JSX is no error node.
fn language_for(lang: Option<&str>, name: &str) -> Result<String, String> {
    let by_path = RyiLang::from_path(name);
    match (lang, by_path) {
        (None, Some(found)) => Ok(found.name().to_string()),
        (None, None) => Err(format!("{name}: no language for this extension; pass --lang")),
        (Some(lang), Some(found @ (RyiLang::TypeScript | RyiLang::Tsx)))
            if RyiLang::parse_name(lang) == Some(RyiLang::TypeScript) =>
        {
            Ok(found.name().to_string())
        }
        (Some(lang), _) => Ok(lang.to_string()),
    }
}

#[derive(serde::Serialize)]
struct CaptureRow<'a> {
    record: &'static str,
    query: &'a str,
    capture: &'a str,
    text: &'a str,
    start: u32,
    end: u32,
    match_start: u32,
    match_end: u32,
}

fn source_bytes(path: &Path, digest: Option<&str>) -> Result<Vec<u8>, String> {
    match digest {
        Some(oid) => cat_blob(path, oid),
        None => std::fs::read(sprefa_extract::io_path(path))
            .map_err(|error| format!("query input '{}': {error}", path.display())),
    }
}

fn cat_blob(path: &Path, oid: &str) -> Result<Vec<u8>, String> {
    let repository = soopy::discover(sprefa_extract::io_path(path.parent().unwrap_or(path)))
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    let mut batch = soopy::GitBatch::open(&repository.root)
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    let bytes = batch
        .read(&soopy::ObjectId(oid.into()))
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    Ok(bytes.to_vec())
}

fn one_line_text(text: String) -> String {
    text.lines()
        .next()
        .unwrap_or("invalid query command")
        .to_string()
}

/// `--scmpp`: flat patterns per language, capture and CST rows per file, then one SQL statement.
/// Without `--sqlite` the rows live in `:memory:` for the run and print to stdout;
/// with it they land in `scmpp_row` and stdout stays empty.
/// `plain`: rows print in the `--query` shape (`split::plain_row`).
fn run_scmpp(
    cli: &QueryArgs,
    text: &str,
    paths: &[std::path::PathBuf],
    writer: Box<dyn Write + Send>,
    cancel: Option<Arc<AtomicBool>>,
    plain: bool,
) -> Result<()> {
    let mut lines = std::collections::HashMap::<String, Vec<u32>>::new();
    // Each file's root node range: a capture of the whole file keeps its text for the SQL, not for stdout.
    let mut roots = std::collections::HashMap::<String, (u64, u64)>::new();
    let mut output = crate::sqlite::Output::with_writer(cli.store.sqlite.as_deref(), writer, true)
        .map_err(|error| error.to_string())?;
    if output.database.is_none() {
        output.database = Some(crate::sqlite::Database::memory().map_err(|error| error.to_string())?);
    }
    let handle = output.database.as_ref().expect("scm++ database").connection().get_interrupt_handle();
    crate::deadline::within_or_cancel(Some(handle), cli.timeout, cancel, "query", |deadline| {
        let mut compiled = std::collections::HashMap::<String, Option<hafley_scm::scmpp::Compiled>>::new();
        let mut skipped = std::collections::BTreeMap::<String, usize>::new();
        let mut store = crate::scmpp::open(output.database.as_mut().expect("scm++ database"))
            .map_err(|error| format!("scm++ store: {error}"))?;
        let mut used = std::collections::BTreeSet::<String>::new();
        for path in paths {
            if deadline.expired() {
                return Err("scm++ passed its deadline".into());
            }
            let name = path.to_string_lossy();
            let language = language_for(cli.lang.as_deref(), &name)?;
            let grammar = RyiLang::parse_name(&language)
                .ok_or_else(|| format!("unknown lang '{language}'"))?
                .tree_sitter_language();
            if !compiled.contains_key(&language) {
                let entry = match hafley_scm::scmpp::compile(&grammar, text) {
                    Ok(found) => Some(found),
                    Err(hafley_scm::scmpp::ScmppError::Query { error, .. })
                        if error.kind == tree_sitter::QueryErrorKind::NodeType => None,
                    Err(error) => return Err(format!("query ({language}): {error}").into()),
                };
                if let Some(found) = &entry {
                    for warning in &found.warnings {
                        crate::ops::print_diagnostic(format_args!("query ({language}): {warning}"));
                    }
                    let database = output.database.as_mut().expect("scm++ database");
                    crate::scmpp::check_sql(database, found).map_err(|error| format!("query ({language}): {error}"))?;
                }
                compiled.insert(language.clone(), entry);
            }
            let Some(found) = &compiled[&language] else {
                *skipped.entry(language).or_default() += 1;
                continue;
            };
            used.insert(language.clone());
            let bytes = source_bytes(path, cli.digest.as_deref())?;
            if plain {
                lines.insert(name.to_string(), split::line_starts(&bytes));
            }
            let tree = hafley_scm::cst::parse(&grammar, &bytes)
                .ok_or_else(|| format!("{name}: tree-sitter returned no tree"))?;
            let database = output.database.as_mut().expect("scm++ database");
            let stop = || deadline.expired();
            let root = tree.root_node();
            roots.insert(name.to_string(), (root.start_byte() as u64, root.end_byte() as u64));
            let trim = cli.store.sqlite.is_none();
            crate::scmpp::write_file(database, &mut store, found, &name, &bytes, &tree, &stop, trim)?;
        }
        if !skipped.is_empty() {
            crate::ops::print_diagnostic(format_args!(
                "query: skipped {} files whose grammar lacks a query node type ({})",
                skipped.values().sum::<usize>(),
                skipped.iter().map(|(language, count)| format!("{language}: {count}"))
                    .collect::<Vec<_>>().join(", "),
            ));
        }
        // One SQL statement runs over every file's rows; abstain when the languages compiled different SQL.
        let sql_of = |language: &String| &compiled[language].as_ref().expect("compiled language").sql;
        if let Some(first) = used.first() {
            let differ = used.iter().filter(|language| sql_of(language) != sql_of(first)).collect::<Vec<_>>();
            if !differ.is_empty() {
                return Err(format!(
                    "query: scm++ compiled different SQL for languages {first} and {}; run one language per --lang",
                    differ.iter().map(|language| language.as_str()).collect::<Vec<_>>().join(", "),
                ).into());
            }
        }
        if deadline.expired() {
            return Err("scm++ passed its deadline".into());
        }
        if let Some(language) = used.first() {
            let found = compiled[language].as_ref().expect("compiled language");
            let database = output.database.as_mut().expect("scm++ database");
            let rows = crate::scmpp::run_sql(database, found).map_err(|error| format!("scm++ SQL: {error}"))?;
            // --sqlite keeps the rows in `scmpp_row` and prints none, like every other --sqlite verb.
            if cli.store.sqlite.is_none() {
                for mut row in rows {
                    let root = roots.get(row["path"].as_str().unwrap_or_default()).copied();
                    let whole: Vec<String> = row
                        .keys()
                        .filter_map(|key| key.strip_suffix("__text"))
                        .filter(|label| {
                            let at = |suffix: &str| row.get(&format!("{label}__{suffix}")).and_then(|value| value.as_u64());
                            root.is_some_and(|(start, end)| at("start") == Some(start) && at("end") == Some(end))
                        })
                        .map(|label| format!("{label}__text"))
                        .collect();
                    for key in whole {
                        row.shift_remove(&key);
                    }
                    let line = match plain {
                        true => serde_json::to_string(&split::plain_row(&row, &lines[row["path"].as_str().unwrap_or_default()])),
                        false => serde_json::to_string(&row),
                    }
                    .map_err(|error| format!("query output: {error}"))?;
                    output.stdout_line(&line).map_err(|error| error.to_string())?;
                }
            }
        }
        output.finish()
    })
}
