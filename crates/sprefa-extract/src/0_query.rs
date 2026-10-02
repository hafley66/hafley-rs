use crate::cli::QueryArgs;
use std::io::Write;
use std::path::Path;

use sprefa_extract::{content_id_of, query_tree_sitter_spans, RyiLang, TreeSitterQuery};

pub fn run(cli: QueryArgs) -> Result<(), String> {
    run_to(cli, Box::new(std::io::stdout()))
}

pub fn run_to(cli: QueryArgs, writer: Box<dyn Write + Send>) -> Result<(), String> {
    // A digest names a blob, so its path need not exist in the worktree.
    let paths = match &cli.digest {
        Some(_) if cli.inputs.paths.len() == 1 => vec![cli.inputs.paths[0].clone()],
        Some(_) => return Err("--digest names one blob; pass exactly one input".into()),
        None => crate::inputs::expand(&cli.inputs)?,
    };
    if paths.is_empty() {
        return Err("query: no inputs; pass files, directories, globs, - or --entry".into());
    }
    let mut output = crate::sqlite::Output::with_writer(cli.sqlite.as_deref(), writer, true)
        .map_err(|error| error.to_string())?;
    let mut compatible = std::collections::HashMap::<String, bool>::new();
    let mut skipped = std::collections::BTreeMap::<String, usize>::new();
    for path in &paths {
        let name = path.to_string_lossy();
        let language = match &cli.lang {
            Some(language) => language.clone(),
            None => RyiLang::from_path(&name)
                .map(|lang| lang.name().to_string())
                .ok_or_else(|| format!("{name}: no language for this extension; pass --lang"))?,
        };
        if !compatible.contains_key(&language) {
            let grammar = RyiLang::parse_name(&language)
                .ok_or_else(|| format!("unknown lang '{language}'"))?
                .tree_sitter_language();
            let supported = match hafley_scm::build(&grammar, &cli.query) {
                Ok(_) => true,
                Err(hafley_scm::QueryExtError::Parse(error))
                    if error.kind == tree_sitter::QueryErrorKind::NodeType => false,
                Err(error) => return Err(format!("query ({language}): {error:?}")),
            };
            compatible.insert(language.clone(), supported);
        }
        if !compatible[&language] {
            *skipped.entry(language).or_default() += 1;
            continue;
        }
        let bytes = source_bytes(path, cli.digest.as_deref())?;
        let request = TreeSitterQuery {
            language,
            query: cli.query.clone(),
        };
        let matches = query_tree_sitter_spans(&bytes, &request)?;
        if let Some(database) = &mut output.database {
            database
                .source(&name, content_id_of(&bytes).to_string())
                .map_err(|error| error.to_string())?;
            for found in &matches {
                for capture in &found.captures {
                    let row = serde_json::json!({
                        "record": "capture",
                        "query": cli.query,
                        "capture": capture.label,
                        "text": capture.text,
                        "start": capture.start,
                        "end": capture.end,
                        "match_start": found.start,
                        "match_end": found.end,
                    });
                    database.insert(row).map_err(|error| error.to_string())?;
                }
            }
            continue;
        }
        for found in matches {
            let mut row = std::collections::BTreeMap::<String, serde_json::Value>::new();
            row.insert("path".into(), name.as_ref().into());
            for capture in found.captures {
                row.insert(capture.label, capture.text.into());
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
    if !skipped.is_empty() {
        crate::ops::print_diagnostic(format_args!(
            "query: skipped {} files whose grammar lacks a query node type ({})",
            skipped.values().sum::<usize>(),
            skipped.iter().map(|(language, count)| format!("{language}: {count}"))
                .collect::<Vec<_>>().join(", "),
        ));
    }
    output.finish().map_err(|error| error.to_string())
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
