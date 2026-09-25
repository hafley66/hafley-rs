//! `--entry`: the files an entry reaches over resolved imports, one shot.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::read::project::{fill_indexes, import_facts, read_inputs_with_modules, Planes, ProjectError};
use crate::read::seams::{FileSet, IndexBag, ManifestMap, ProjectCx, ProjectDigest};
use crate::read::shape::ContentId;
use crate::read::source::RyiOutput;
use crate::read::wire::FlatFact;

/// A cycle-free import chain longer than this is past any real module tree.
pub const REACH_DEPTH_CAP: u32 = 32;

/// Every `universe` file an `entry` reaches over `resolved_import` targets in
/// `depth` hops; a go package-directory target reaches the files inside it.
pub fn reach_files(
    _root: &Path,
    universe: &[PathBuf],
    entry: &[PathBuf],
    depth: Option<u32>,
) -> Result<Vec<PathBuf>, ProjectError> {
    let inputs = read_inputs_with_modules(universe, Planes::Resolve { flow: false })?;
    let pairs: Vec<(ContentId, &RyiOutput)> = inputs
        .iter()
        .map(|input| (input.blob.clone(), input.output.as_ref()))
        .collect();
    let corpus: Vec<(String, ContentId)> = inputs
        .iter()
        .map(|input| (input.path.clone(), input.blob.clone()))
        .collect();
    let cx = ProjectCx {
        files: &FileSet,
        manifests: &ManifestMap,
        reader: None,
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
        witness: false,
    };
    fill_indexes(&cx, &inputs, &pairs, &corpus);

    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let by_canonical: HashMap<PathBuf, usize> = universe
        .iter()
        .enumerate()
        .map(|(index, path)| (canonical(path), index))
        .collect();
    let index_of = |spelled: &str| by_canonical.get(&canonical(Path::new(spelled))).copied();
    let mut next: HashMap<usize, Vec<usize>> = HashMap::new();
    for input in &inputs {
        let Some(from) = index_of(&input.path) else { continue };
        crate::read::types::set_own(Some(input.blob.clone()));
        for fact in import_facts(input, &cx) {
            let FlatFact::ResolvedImportRow { target_path, .. } = fact else { continue };
            let target = Path::new(&target_path);
            if let Some(to) = index_of(&target_path) {
                next.entry(from).or_default().push(to);
            } else if target.is_dir() {
                let dir = canonical(target);
                next.entry(from).or_default().extend(
                    universe
                        .iter()
                        .enumerate()
                        .filter(|(_, path)| canonical(path).parent() == Some(dir.as_path()))
                        .map(|(index, _)| index),
                );
            }
        }
        crate::read::types::set_own(None);
    }

    let mut reached = vec![false; universe.len()];
    let mut frontier = Vec::new();
    for path in entry {
        let Some(index) = by_canonical.get(&canonical(path)).copied() else {
            return Err(ProjectError::Read(
                path.clone(),
                std::io::Error::new(std::io::ErrorKind::NotFound, "entry is not in the input set"),
            ));
        };
        if !reached[index] {
            reached[index] = true;
            frontier.push(index);
        }
    }
    let limit = depth.unwrap_or(REACH_DEPTH_CAP).min(REACH_DEPTH_CAP);
    let mut hop = 0;
    while !frontier.is_empty() && hop < limit {
        let mut following = Vec::new();
        for from in frontier {
            for &to in next.get(&from).into_iter().flatten() {
                if !reached[to] {
                    reached[to] = true;
                    following.push(to);
                }
            }
        }
        frontier = following;
        hop += 1;
    }
    Ok(universe
        .iter()
        .zip(reached)
        .filter_map(|(path, hit)| hit.then(|| path.clone()))
        .collect())
}
