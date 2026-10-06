//! Owned Names answers, retained by the workspace until its VFS changes.
use super::modules::ModulePlace;
use super::names::{Abstain, DefPlace};
use std::collections::HashMap;
use std::path::PathBuf;

pub(super) type Definitions = Result<Vec<DefPlace>, Abstain>;

#[derive(Default)]
pub(crate) struct NamesCache {
    pub all_places: Option<Vec<ModulePlace>>,
    pub paths: HashMap<(PathBuf, Vec<String>, Option<u32>), Definitions>,
    pub written: HashMap<(PathBuf, u32, String), Definitions>,
    pub dependencies: HashMap<PathBuf, Vec<(String, ModulePlace)>>,
}
