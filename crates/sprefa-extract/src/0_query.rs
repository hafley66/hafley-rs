use crate::cli::QueryArgs;
use std::io::Write;
use std::path::Path;

use rayon::prelude::*;
use sprefa_extract::{content_id_of, extract_pool, query_tree_sitter_spans_with, RyiLang};

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

pub fn run(cli: QueryArgs) -> Result<()> {
    run_to(cli, Box::new(std::io::stdout()))
}

pub fn run_to(cli: QueryArgs, writer: Box<dyn Write + Send>) -> Result<()> {
    // A digest names a blob, so its path need not exist in the worktree.
    let paths = match &cli.digest {
        Some(_) if cli.inputs.paths.len() == 1 => vec![cli.inputs.paths[0].clone()],
        Some(_) => return Err("--digest names one blob; pass exactly one input".into()),
        None => crate::inputs::expand(&cli.inputs)?,
    };
    if paths.is_empty() {
        return Err("query: no inputs; pass files, directories, globs, - or --entry".into());
    }
    let query = match (&cli.query, &cli.scmpp) {
        (Some(query), None) => query.clone(),
        (None, Some(file)) => return run_scmpp(&cli, file, &paths, writer),
        (Some(_), Some(_)) => return Err("query: pass --query or --scmpp, not both".into()),
        (None, None) => return Err("query: pass --query TEXT or --scmpp FILE".into()),
    };
    let mut output = crate::sqlite::Output::with_writer(cli.store.sqlite.as_deref(), writer, true)
        .map_err(|error| error.to_string())?;
    // One compiled query per language, built once; `None` marks a grammar that
    // lacks one of the query's node types.
    let mut compiled =
        std::collections::HashMap::<String, Option<(tree_sitter::Language, hafley_scm::QueryExt)>>::new();
    let mut skipped = std::collections::BTreeMap::<String, usize>::new();
    let mut jobs = Vec::<(&std::path::PathBuf, String)>::with_capacity(paths.len());
    // A file with no language ends the stream where it sits: the files before
    // it still answer, then the error.
    let mut late_error = None;
    for path in &paths {
        let name = path.to_string_lossy();
        let language = match &cli.lang {
            Some(language) => language.clone(),
            None => match RyiLang::from_path(&name) {
                Some(lang) => lang.name().to_string(),
                None => {
                    late_error = Some(format!("{name}: no language for this extension; pass --lang"));
                    break;
                }
            },
        };
        if !compiled.contains_key(&language) {
            let grammar = RyiLang::parse_name(&language)
                .ok_or_else(|| format!("unknown lang '{language}'"))?
                .tree_sitter_language();
            let entry = match hafley_scm::build(&grammar, &query) {
                Ok(built) => Some((grammar, built)),
                Err(hafley_scm::QueryExtError::Parse(error))
                    if error.kind == tree_sitter::QueryErrorKind::NodeType => None,
                Err(error) => return Err(format!("query ({language}): {error}").into()),
            };
            compiled.insert(language.clone(), entry);
        }
        if compiled[&language].is_none() {
            *skipped.entry(language).or_default() += 1;
            continue;
        }
        jobs.push((path, language));
    }
    // Read, parse and match fan out on the extraction pool, one parser per
    // worker; rows are written in input order, a chunk at a time. The request
    // root is thread-local, so each worker reads through the caller's root.
    let io_root = sprefa_extract::request_io_root();
    for chunk in jobs.chunks(1024) {
        let results: Vec<Result<(String, Vec<sprefa_extract::TreeSitterSpannedMatch>), String>> =
            extract_pool().install(|| {
                chunk
                    .par_iter()
                    .map_init(tree_sitter::Parser::new, |parser, (path, language)| {
                        let (grammar, built) = compiled[language].as_ref().expect("compiled language");
                        let read = || source_bytes(path, cli.digest.as_deref());
                        let bytes = match &io_root {
                            Some(root) => sprefa_extract::with_io_root(root.clone(), read),
                            None => read(),
                        }?;
                        let matches = query_tree_sitter_spans_with(&bytes, grammar, built, parser)?;
                        Ok((content_id_of(&bytes).to_string(), matches))
                    })
                    .collect()
            });
        for ((path, _), result) in chunk.iter().zip(results) {
            let name = path.to_string_lossy();
            let (content_id, matches) = result?;
            if let Some(database) = &mut output.database {
                database
                    .source(&name, content_id)
                    .map_err(|error| error.to_string())?;
                for found in &matches {
                    for capture in &found.captures {
                        // Typed and `record`-first: the binder batches rows
                        // across files instead of one insert per file.
                        database
                            .bind_row(&CaptureRow {
                                record: "capture",
                                query: &query,
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
                continue;
            }
            for found in matches {
                let mut row = std::collections::BTreeMap::<String, serde_json::Value>::new();
                row.insert("path".into(), name.as_ref().into());
                // A capture with one node is its text; a capture bound to several nodes
                // (`*`, `+`, or a repeated name) is an array of their texts in document order.
                let mut nodes = std::collections::BTreeMap::<String, Vec<(u32, String)>>::new();
                for capture in found.captures {
                    nodes.entry(capture.label).or_default().push((capture.start, capture.text));
                }
                for (label, mut texts) in nodes {
                    let value = if texts.len() == 1 {
                        texts.pop().expect("one node").1.into()
                    } else {
                        texts.sort_by_key(|(start, _)| *start);
                        texts.into_iter().map(|(_, text)| text).collect::<Vec<_>>().into()
                    };
                    row.insert(label, value);
                }
                row.insert("line".into(), found.line.into());
                row.insert("end_line".into(), found.end_line.into());
                output
                    .line(
                        &serde_json::to_string(&row)
                            .map_err(|error| format!("query output: {error}"))?,
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
    }
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
fn run_scmpp(
    cli: &QueryArgs,
    file: &Path,
    paths: &[std::path::PathBuf],
    writer: Box<dyn Write + Send>,
) -> Result<()> {
    let text = std::fs::read_to_string(sprefa_extract::io_path(file))
        .map_err(|error| format!("--scmpp '{}': {error}", file.display()))?;
    let mut output = crate::sqlite::Output::with_writer(cli.store.sqlite.as_deref(), writer, true)
        .map_err(|error| error.to_string())?;
    if output.database.is_none() {
        output.database = Some(crate::sqlite::Database::memory().map_err(|error| error.to_string())?);
    }
    let handle = output.database.as_ref().expect("scm++ database").connection().get_interrupt_handle();
    crate::deadline::within(Some(handle), cli.timeout, "query", |deadline| {
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
            let language = match &cli.lang {
                Some(language) => language.clone(),
                None => RyiLang::from_path(&name)
                    .map(|lang| lang.name().to_string())
                    .ok_or_else(|| format!("{name}: no language for this extension; pass --lang"))?,
            };
            let grammar = RyiLang::parse_name(&language)
                .ok_or_else(|| format!("unknown lang '{language}'"))?
                .tree_sitter_language();
            if !compiled.contains_key(&language) {
                let entry = match hafley_scm::scmpp::compile(&grammar, &text) {
                    Ok(found) => Some(found),
                    Err(hafley_scm::scmpp::ScmppError::Query { error, .. })
                        if error.kind == tree_sitter::QueryErrorKind::NodeType => None,
                    Err(error) => return Err(format!("query ({language}): {error}").into()),
                };
                if let Some(found) = &entry {
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
            let tree = hafley_scm::cst::parse(&grammar, &bytes)
                .ok_or_else(|| format!("{name}: tree-sitter returned no tree"))?;
            let database = output.database.as_mut().expect("scm++ database");
            crate::scmpp::write_file(database, &mut store, found, &name, &bytes, &tree)?;
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
                for row in rows {
                    let line = serde_json::to_string(&row).map_err(|error| format!("query output: {error}"))?;
                    output.stdout_line(&line).map_err(|error| error.to_string())?;
                }
            }
        }
        output.finish()
    })
}
