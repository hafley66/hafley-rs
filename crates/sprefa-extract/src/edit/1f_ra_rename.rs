//! The slow Rust rename is rust-analyzer's rename.

use std::time::Duration;

use crate::lang::rust_checker::{rename, RenameFailure, RenameSeed};

use crate::edit_seams::{RefRole, RenameAbstain, RenameStop, SymbolRef};
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::Span;

const ENGINE: &str = "rust-analyzer";

pub fn symbol_refs_and_abstains(
    cx: &RenameCx,
    request: &RenameRequest,
) -> Result<(Vec<SymbolRef>, Vec<RenameAbstain>), RenameStop> {
    let files: Vec<(String, std::path::PathBuf)> = cx
        .files()
        .iter()
        .filter(|rel| rel.ends_with(".rs"))
        .map(|rel| (rel.clone(), cx.abs(rel)))
        .collect();
    let overlays: Vec<(String, std::path::PathBuf, String)> = cx
        .overlaid()
        .iter()
        .filter(|(rel, _)| rel.ends_with(".rs"))
        .map(|(rel, text)| (rel.clone(), cx.abs(rel), text.clone()))
        .collect();
    let seed = match request.at {
        Some(at) => RenameSeed::At(at),
        None => RenameSeed::Name(&request.old),
    };
    let edits = rename(
        cx.root(),
        &files,
        &overlays,
        &request.anchor,
        seed,
        &request.new,
        Duration::from_secs(120),
    )
    .map_err(|failure| match failure {
        RenameFailure::NotFound => RenameStop::NotFound {
            anchor: request.anchor.clone(),
            old: request.old.clone(),
        },
        RenameFailure::Ambiguous(sites) => RenameStop::Ambiguous {
            anchor: request.anchor.clone(),
            old: request.old.clone(),
            sites: sites
                .into_iter()
                .map(|(start, end)| Span { start, len: end - start })
                .collect(),
        },
        RenameFailure::Refused(reason) | RenameFailure::FileSystem(reason) => RenameStop::Refused {
            anchor: request.anchor.clone(),
            engine: ENGINE,
            reason,
        },
        RenameFailure::Checker(error) => RenameStop::Refused {
            anchor: request.anchor.clone(),
            engine: ENGINE,
            reason: error.to_string(),
        },
    })?;
    let mut refs = Vec::with_capacity(edits.len());
    for edit in edits {
        let site = Span {
            start: edit.start,
            len: edit.end - edit.start,
        };
        cx.put_slow_edit(&edit.path, site, edit.new);
        refs.push(SymbolRef {
            file: edit.path,
            span: site,
            role: RefRole::Read,
            text: edit.old,
        });
    }
    Ok((refs, Vec::new()))
}
