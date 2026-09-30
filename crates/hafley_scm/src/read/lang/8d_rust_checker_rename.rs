//! rust-analyzer's rename, as text edits in the parse plane's offsets.

use super::*;
use ra_ap_ide::{FilePosition, RenameConfig};
use ra_ap_syntax::ast::HasModuleItem;
use ra_ap_syntax::{match_ast, TextSize};
use ra_ap_vfs::VfsPath;

/// One replacement: `start..end` in the parse plane's offsets of `path`
/// (`OffsetMap`), the text there now, and the text rust-analyzer writes.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct RenameEdit {
    pub path: String,
    pub start: u32,
    pub end: u32,
    pub old: String,
    pub new: String,
}

pub enum RenameSeed<'a> {
    /// A parse-plane offset inside the declaration's name.
    At(u32),
    /// The item named this at the anchor file's root; it must be the only one.
    Name(&'a str),
}

#[derive(Debug)]
pub enum RenameFailure {
    Checker(CheckerError),
    NotFound,
    /// Parse-plane `(start, end)` of every candidate declaration.
    Ambiguous(Vec<(u32, u32)>),
    /// rust-analyzer's own refusal, verbatim.
    Refused(String),
    /// The rename moves or creates files (a module rename); ryi does not stage those.
    FileSystem(String),
}

impl From<CheckerError> for RenameFailure {
    fn from(error: CheckerError) -> Self {
        RenameFailure::Checker(error)
    }
}

/// Inverse of `OffsetMap::to_span_offset`: line start byte plus a character column.
fn to_byte_offset(text: &str, span_offset: u32) -> u32 {
    let line_start = text.as_bytes()[..(span_offset as usize).min(text.len())]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |newline| newline + 1);
    let column = span_offset as usize - line_start;
    text[line_start..]
        .char_indices()
        .nth(column)
        .map_or(text.len(), |(byte, _)| line_start + byte) as u32
}

fn item_name(node: &ra_ap_syntax::SyntaxNode) -> Option<ast::Name> {
    match_ast! {
        match node {
            ast::Fn(it) => it.name(),
            ast::Struct(it) => it.name(),
            ast::Enum(it) => it.name(),
            ast::Union(it) => it.name(),
            ast::Trait(it) => it.name(),
            ast::TypeAlias(it) => it.name(),
            ast::Const(it) => it.name(),
            ast::Static(it) => it.name(),
            ast::Module(it) => it.name(),
            ast::MacroRules(it) => it.name(),
            ast::Variant(it) => it.name(),
            ast::RecordField(it) => it.name(),
            _ => None,
        }
    }
}

/// Items at the file root named `name`; when there are none, every named item in the file.
fn declarations(file: &ast::SourceFile, name: &str) -> Vec<ast::Name> {
    let root: Vec<ast::Name> = file
        .items()
        .filter_map(|item| item_name(item.syntax()))
        .filter(|named| named.text() == name)
        .collect();
    if !root.is_empty() {
        return root;
    }
    file.syntax()
        .descendants()
        .filter_map(|node| item_name(&node))
        .filter(|named| named.text() == name)
        .collect()
}

/// Rename the symbol at `seed` in `anchor`. `overlays` (a batch's staged texts)
/// hold for this call only; the warm session gets the on-disk text back after it.
pub fn rename(
    root: &Path,
    files: &[(String, PathBuf)],
    overlays: &[(String, PathBuf, String)],
    anchor: &str,
    seed: RenameSeed<'_>,
    new_name: &str,
    budget: Duration,
) -> Result<Vec<RenameEdit>, RenameFailure> {
    let (workspace, _) =
        super::super::rust_checker_session::checker_workspace(root, super::super::rust_checker::Tier::Slow, files, budget)?;
    let mut workspace = workspace.lock().unwrap();
    let mut paths: HashMap<ra_ap_ide::FileId, String> = HashMap::new();
    let mut anchor_id = None;
    for (name, path) in files.iter().map(|(name, path)| (name, path.clone())) {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if let Some(id) = workspace
            .vfs
            .file_id(&VfsPath::new_real_path(path.to_string_lossy().into_owned()))
        {
            let id = ra_ap_ide::FileId::from_raw(id.0.index());
            if name == anchor {
                anchor_id = Some(id);
            }
            paths.insert(id, name.clone());
        }
    }
    let anchor_id = anchor_id.ok_or_else(|| {
        RenameFailure::Refused(format!("{anchor} is not in the Cargo workspace rust-analyzer loaded at {}", root.display()))
    })?;
    let set_texts = |workspace: &mut super::super::rust_checker_session::CheckerWorkspace,
                     texts: &[(ra_ap_ide::FileId, String)]| {
        let mut change = ra_ap_ide_db::ChangeWithProcMacros::default();
        for (id, text) in texts {
            change.change_file(*id, Some(text.clone()));
        }
        workspace.host.apply_change(change);
    };
    let staged: Vec<(ra_ap_ide::FileId, String, String)> = overlays
        .iter()
        .filter_map(|(name, _, text)| {
            let id = paths.iter().find(|(_, path)| *path == name).map(|(id, _)| *id)?;
            let disk = workspace.host.analysis().file_text(id).ok()?.to_string();
            Some((id, text.clone(), disk))
        })
        .collect();
    set_texts(
        &mut workspace,
        &staged.iter().map(|(id, text, _)| (*id, text.clone())).collect::<Vec<_>>(),
    );
    let result = rename_in(&workspace, &paths, anchor_id, seed, new_name);
    set_texts(
        &mut workspace,
        &staged.iter().map(|(id, _, disk)| (*id, disk.clone())).collect::<Vec<_>>(),
    );
    result
}

fn rename_in(
    workspace: &super::super::rust_checker_session::CheckerWorkspace,
    paths: &HashMap<ra_ap_ide::FileId, String>,
    anchor_id: ra_ap_ide::FileId,
    seed: RenameSeed<'_>,
    new_name: &str,
) -> Result<Vec<RenameEdit>, RenameFailure> {
    let analysis = workspace.host.analysis();
    let cancelled = |_| RenameFailure::Refused("rust-analyzer cancelled the query".to_string());
    let anchor_text = analysis.file_text(anchor_id).map_err(cancelled)?.to_string();
    let offset = match seed {
        RenameSeed::At(at) => {
            let byte = TextSize::from(to_byte_offset(&anchor_text, at));
            let file = analysis.parse(anchor_id).map_err(cancelled)?;
            let on_name = file
                .syntax()
                .token_at_offset(byte)
                .any(|token| token.parent().is_some_and(|parent| ast::Name::can_cast(parent.kind())));
            // `--at` is any offset inside the declaration; rust-analyzer needs one on its name.
            match on_name {
                true => u32::from(byte),
                false => file
                    .syntax()
                    .token_at_offset(byte)
                    .next()
                    .and_then(|token| token.parent_ancestors().find_map(|node| item_name(&node)))
                    .map(|name| u32::from(name.syntax().text_range().start()))
                    .ok_or(RenameFailure::NotFound)?,
            }
        }
        RenameSeed::Name(name) => {
            let file = analysis.parse(anchor_id).map_err(cancelled)?;
            let offsets = OffsetMap::new(&anchor_text);
            match declarations(&file, name).as_slice() {
                [] => return Err(RenameFailure::NotFound),
                [one] => u32::from(one.syntax().text_range().start()),
                many => {
                    return Err(RenameFailure::Ambiguous(
                        many.iter()
                            .map(|name| {
                                let range = name.syntax().text_range();
                                (
                                    offsets.to_span_offset(u32::from(range.start())),
                                    offsets.to_span_offset(u32::from(range.end())),
                                )
                            })
                            .collect(),
                    ))
                }
            }
        }
    };
    let config = RenameConfig {
        prefer_no_std: false,
        prefer_prelude: true,
        prefer_absolute: false,
        show_conflicts: true,
    };
    let change = analysis
        .rename(
            FilePosition {
                file_id: anchor_id,
                offset: TextSize::from(offset),
            },
            new_name,
            &config,
        )
        .map_err(cancelled)?
        .map_err(|error| RenameFailure::Refused(error.to_string()))?;
    if !change.file_system_edits.is_empty() {
        return Err(RenameFailure::FileSystem(format!("{:?}", change.file_system_edits)));
    }
    let mut edits = Vec::new();
    for (file_id, (text_edit, _)) in change.source_file_edits {
        let Some(path) = paths.get(&file_id) else {
            return Err(RenameFailure::Refused(format!(
                "rust-analyzer edits a file outside the corpus (file id {})",
                file_id.index()
            )));
        };
        let text = analysis.file_text(file_id).map_err(cancelled)?.to_string();
        let offsets = OffsetMap::new(&text);
        for indel in text_edit.iter() {
            let start = u32::from(indel.delete.start());
            let end = u32::from(indel.delete.end());
            edits.push(RenameEdit {
                path: path.clone(),
                start: offsets.to_span_offset(start),
                end: offsets.to_span_offset(end),
                old: text[start as usize..end as usize].to_string(),
                new: indel.insert.clone(),
            });
        }
    }
    edits.sort();
    Ok(edits)
}
