//! Remove an emptied Rust source and the declarations supplied by the module engine.
use std::collections::{BTreeMap, BTreeSet};
use sprefa_extract::MoveCx;

pub fn remove(cx: &mut MoveCx, sources: &BTreeSet<String>) -> Result<BTreeSet<String>, String> {
    let mut deleted = BTreeSet::new();
    let mut declarations: BTreeMap<String, BTreeSet<(usize, usize)>> = BTreeMap::new();
    for source in sources.iter().filter(|source| source.ends_with(".rs")) {
        let text = cx.text(source).ok_or_else(|| format!("cleave source {source} has no text"))?;
        if !hafley_scm::lang::rust::parse_rust_file(&text).map_err(|error| error.to_string())?.items.is_empty() {
            continue;
        }
        let places = sprefa_extract::edit::rust_module_tree::places(cx, source)?;
        if places.is_empty() {
            return Err(format!("cannot locate the module declaration of emptied source {source}"));
        }
        for place in places {
            let (path, start, end) = place.decl.ok_or_else(|| format!("cannot delete emptied crate root {source}"))?;
            let parent = cx.rel(&path).ok_or_else(|| format!("declaration {} is outside the cleave root", path.display()))?;
            declarations.entry(parent).or_default().insert((start as usize, end as usize));
        }
        deleted.insert(source.clone());
    }
    for (parent, spans) in declarations {
        let mut text = cx.text(&parent).ok_or_else(|| format!("module parent {parent} has no text"))?;
        for (start, end) in spans.into_iter().rev() {
            let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
            let start = if text[line_start..start].trim().is_empty() { line_start } else { start };
            let end = if text[end..].starts_with("\n\n") { end + 2 } else if text[end..].starts_with('\n') { end + 1 } else { end };
            text.replace_range(start..end, "");
        }
        cx.overlay(&parent, text);
    }
    Ok(deleted)
}
