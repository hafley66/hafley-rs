use crate::cli::QueryArgs;
use std::path::{Path, PathBuf};

use sprefa_extract::{content_id_of, query_tree_sitter_spans, RyiLang, TreeSitterQuery};

pub fn run(cli: QueryArgs) -> Result<(), String> {
    // A digest names a blob, so its path need not exist in the worktree.
    let paths = match &cli.digest {
        Some(_) if cli.inputs.paths.len() == 1 => vec![PathBuf::from(&cli.inputs.paths[0])],
        Some(_) => return Err("--digest names one blob; pass exactly one input".into()),
        None => crate::inputs::expand(&cli.inputs)?,
    };
    if paths.is_empty() {
        return Err("query: no inputs; pass files, directories, globs, - or --entry".into());
    }
    let mut output =
        crate::sqlite::Output::new(cli.sqlite.as_deref()).map_err(|error| error.to_string())?;
    for path in &paths {
        let name = path.to_string_lossy();
        let language = match &cli.lang {
            Some(language) => language.clone(),
            None => RyiLang::from_path(&name)
                .map(|lang| lang.name().to_string())
                .ok_or_else(|| format!("{name}: no language for this extension; pass --lang"))?,
        };
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
                .line(&serde_json::to_string(&row).map_err(|error| format!("query output: {error}"))?)
                .map_err(|error| error.to_string())?;
        }
    }
    output.finish().map_err(|error| error.to_string())
}

fn source_bytes(path: &Path, digest: Option<&str>) -> Result<Vec<u8>, String> {
    match digest {
        Some(oid) => cat_blob(path, oid),
        None => std::fs::read(path)
            .map_err(|error| format!("query input '{}': {error}", path.display())),
    }
}

fn cat_blob(path: &Path, oid: &str) -> Result<Vec<u8>, String> {
    let repository = soopy::discover(path.parent().unwrap_or(path))
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
