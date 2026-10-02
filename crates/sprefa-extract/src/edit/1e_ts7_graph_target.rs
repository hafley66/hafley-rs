//! Targeted TypeScript LSP references for graph queries.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::ts7_cleave_facts::with_session;
use super::ts7_lsp_session::file_uri;
use super::ts7_symbol_seed::{byte_at_lsp_position, declaration_spans};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TargetReference {
    pub source_path: String,
    pub site_start: u32,
    pub site_end: u32,
    pub target_path: String,
    pub target_start: u32,
    pub target_end: u32,
}

fn position(text: &str, offset: usize) -> Result<Value, String> {
    let before = text.get(..offset).ok_or("position splits a character")?;
    Ok(json!({
        "line": before.bytes().filter(|byte| *byte == b'\n').count(),
        "character": before.rsplit('\n').next().unwrap_or(before).encode_utf16().count(),
    }))
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn references(
    root: &Path,
    files: &[(String, PathBuf)],
    seeds: &[(String, String)],
    source_paths: &BTreeSet<String>,
) -> Result<Vec<TargetReference>, String> {
    let _checker_span = crate::trace::tracked(tracing::info_span!("typescript.checker")).entered();
    if seeds.is_empty() {
        return Ok(Vec::new());
    }
    let supplied: HashMap<PathBuf, &str> = files
        .iter()
        .map(|(name, path)| (canonical(path), name.as_str()))
        .collect();
    with_session(root, |session| {
        let mut found = BTreeSet::new();
        for (name, path) in files {
            if !source_paths.contains(name) || !(name.ends_with(".ts") || name.ends_with(".tsx")) {
                continue;
            }
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("read {}: {error}", path.display()))?;
            let uri = file_uri(&canonical(path))?;
            session.sync_document(&uri, name, &text)?;
        }
        for (target_path, name) in seeds {
            let path = canonical(&PathBuf::from(target_path));
            if !supplied.contains_key(&path) {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("read {}: {error}", path.display()))?;
            let uri = file_uri(&path)?;
            session.sync_document(&uri, target_path, &text)?;
            let declarations = declaration_spans(&mut session.lsp, &uri, &text, name)?;
            tracing::debug!(
                target_path,
                name,
                declarations = declarations.len(),
                "TypeScript target declarations"
            );
            for declaration in declarations {
                let reply = session.lsp.request(
                    "textDocument/references",
                    &json!({
                        "textDocument": {"uri": uri.as_str()},
                        "position": position(&text, declaration.start as usize)?,
                        "context": {"includeDeclaration": false},
                    }),
                )?;
                if let Some(error) = reply.error {
                    return Err(format!("references {name}: {}", error.message));
                }
                let rows = reply.result.unwrap_or(Value::Null);
                tracing::debug!(
                    target_path,
                    name,
                    references = rows.as_array().map_or(0, Vec::len),
                    "TypeScript target references"
                );
                for row in rows.as_array().into_iter().flatten() {
                    let Some(source) = row["uri"]
                        .as_str()
                        .and_then(|uri| url::Url::parse(uri).ok())
                        .and_then(|uri| uri.to_file_path().ok())
                    else {
                        continue;
                    };
                    let source = canonical(&source);
                    let Some(source_path) = supplied.get(&source) else {
                        continue;
                    };
                    let source_text = std::fs::read_to_string(&source)
                        .map_err(|error| format!("read {}: {error}", source.display()))?;
                    let start = serde_json::from_value(row["range"]["start"].clone())
                        .map_err(|error| format!("reference position: {error}"))?;
                    let end = serde_json::from_value(row["range"]["end"].clone())
                        .map_err(|error| format!("reference position: {error}"))?;
                    found.insert(TargetReference {
                        source_path: (*source_path).to_string(),
                        site_start: byte_at_lsp_position(&source_text, start)? as u32,
                        site_end: byte_at_lsp_position(&source_text, end)? as u32,
                        target_path: target_path.clone(),
                        target_start: declaration.start,
                        target_end: declaration.end(),
                    });
                }
            }
        }
        Ok(found.into_iter().collect())
    })
}
