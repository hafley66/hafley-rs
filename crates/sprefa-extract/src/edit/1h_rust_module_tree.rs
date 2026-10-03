//! Cleave's Rust module answers: rust-analyzer's def maps over the run's texts.

use crate::move_cx::MoveCx;

#[cfg(feature = "rust-checker")]
pub use hafley_scm::read::lang::rust_checker::ModulePlace;

#[cfg(feature = "rust-checker")]
fn tree(cx: &MoveCx) -> Result<&hafley_scm::read::lang::rust_checker::RustModuleTree, String> {
    cx.rust_modules
        .get_or_init(|| {
            hafley_scm::read::lang::rust_checker::module_tree(cx.root(), std::time::Duration::from_secs(120))
                .map_err(|error| format!("rust-analyzer cannot load the Cargo workspace at {}: {error}", cx.root().display()))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Every module `rel` is once the host holds `cx`'s planned and overlaid texts.
#[cfg(feature = "rust-checker")]
pub fn places(cx: &MoveCx, rel: &str) -> Result<Vec<ModulePlace>, String> {
    let tree = tree(cx)?;
    let texts: Vec<(std::path::PathBuf, String)> = cx
        .resolver_texts()
        .into_iter()
        .filter(|(path, _)| path.ends_with(".rs"))
        .map(|(path, text)| (cx.abs(path), text.to_string()))
        .collect();
    tree.sync(&texts).map_err(|error| error.to_string())?;
    Ok(tree.places(&cx.abs(rel)))
}

/// The name `from`'s crate reaches `to`'s crate by.
#[cfg(feature = "rust-checker")]
pub fn extern_name(cx: &MoveCx, from: &ModulePlace, to: &ModulePlace) -> Result<String, String> {
    Ok(tree(cx)?.extern_name(from, to))
}

#[cfg(not(feature = "rust-checker"))]
#[derive(Clone, Debug)]
pub struct ModulePlace {
    pub crate_root: std::path::PathBuf,
    pub crate_name: String,
    pub path: Vec<String>,
    pub decl: Option<(std::path::PathBuf, u32, u32)>,
}

#[cfg(not(feature = "rust-checker"))]
pub fn places(_cx: &MoveCx, _rel: &str) -> Result<Vec<ModulePlace>, String> {
    Err("Rust module paths need --features rust-checker".to_string())
}

#[cfg(not(feature = "rust-checker"))]
pub fn extern_name(_cx: &MoveCx, _from: &ModulePlace, to: &ModulePlace) -> Result<String, String> {
    Ok(to.crate_name.clone())
}
