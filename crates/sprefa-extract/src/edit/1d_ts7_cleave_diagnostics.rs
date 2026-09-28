use std::collections::BTreeSet;
use std::path::Path;

use lsp_types::Uri;
use serde_json::json;

use super::ts7_cleave_facts::with_session;
use super::ts7_lsp_session::{file_uri, TsSession};

fn diagnostics(session: &mut TsSession, uri: &Uri) -> Result<Vec<String>, String> {
    let reply = session.lsp.request(
        "textDocument/diagnostic",
        &json!({
            "textDocument": {"uri": uri.as_str()},
        }),
    )?;
    if let Some(error) = reply.error {
        return Err(format!("diagnostic: {}", error.message));
    }
    let result = reply.result.ok_or("diagnostic returned no result")?;
    let rows = result["items"]
        .as_array()
        .ok_or("diagnostic returned no items")?;
    let mut errors: Vec<String> = rows
        .iter()
        .filter(|row| row["severity"].as_u64().unwrap_or(1) == 1)
        .map(|row| {
            format!(
                "{}: {}",
                row["code"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| row["code"].to_string()),
                row["message"].as_str().unwrap_or("unknown diagnostic")
            )
        })
        .collect();
    errors.sort();
    Ok(errors)
}

#[cfg(unix)]
fn fill_preview(root: &Path, preview: &Path) -> Result<(), String> {
    for entry in
        std::fs::read_dir(root).map_err(|error| format!("read {}: {error}", root.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_name() == ".git" {
            continue;
        }
        let source = entry.path();
        let dest = preview.join(entry.file_name());
        if dest.exists() {
            if source.is_dir() && dest.is_dir() {
                fill_preview(&source, &dest)?;
            }
        } else {
            std::os::unix::fs::symlink(&source, &dest)
                .map_err(|error| format!("preview link {}: {error}", dest.display()))?;
        }
    }
    Ok(())
}

pub fn check_preview(root: &Path, preview: &Path, paths: &BTreeSet<String>) -> Result<(), String> {
    let baseline = with_session(root, |session| {
        let mut baseline = Vec::new();
        for path in paths {
            let original = root.join(path);
            if !original.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&original)
                .map_err(|error| format!("read {path}: {error}"))?;
            let uri = file_uri(&original)?;
            session.sync_document(&uri, path, &text)?;
        }
        for path in paths {
            if !root.join(path).is_file() {
                continue;
            }
            let uri = file_uri(&root.join(path))?;
            baseline.extend(diagnostics(session, &uri)?);
        }
        Ok(baseline)
    })?;
    #[cfg(unix)]
    fill_preview(root, preview)?;
    let mut session = TsSession::open(preview)?;
    let mut after = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(preview.join(path))
            .map_err(|error| format!("read preview {path}: {error}"))?;
        let uri = file_uri(&preview.join(path))?;
        session.sync_document(&uri, path, &text)?;
    }
    for path in paths {
        let uri = file_uri(&preview.join(path))?;
        let rows = diagnostics(&mut session, &uri)?;
        after.extend(rows.into_iter().map(|error| (path.clone(), error)));
    }
    let baseline: BTreeSet<_> = baseline.into_iter().collect();
    after.retain(|row| !baseline.contains(&row.1));
    if let Some((path, error)) = after.first() {
        return Err(format!("cleave --slow diagnostic in {path}: {error}"));
    }
    Ok(())
}
