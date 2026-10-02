//! Validate placement before any lane-create side effect.
use anyhow::Result;
use std::path::Path;

pub(crate) fn validate_target(env: &[(String, String)]) -> Result<()> {
    let root = boop::trail::lane_target_root()?;
    anyhow::ensure!(root.is_absolute(), "BOOP_LANE_TARGET_ROOT must be absolute");
    for (key, value) in env {
        if key == "BOOP_LANE_TARGET_ROOT" {
            anyhow::ensure!(
                Path::new(value) == root,
                "lane create cannot override BOOP_LANE_TARGET_ROOT"
            );
        }
        if key == "CARGO_TARGET_DIR" {
            anyhow::ensure!(
                Path::new(value).is_absolute(),
                "CARGO_TARGET_DIR must be absolute"
            );
            anyhow::ensure!(!value.is_empty() && boop::target_root::under(&root, Path::new(value)), "CARGO_TARGET_DIR={} is outside lane target root {}; lane create refuses this target", value, root.display());
            anyhow::ensure!(
                !Path::new(value).starts_with(root.join("_shared"))
                    && !boop::target_root::under(&root.join("_shared"), Path::new(value))
                    && !std::fs::canonicalize(value)
                        .ok()
                        .zip(std::fs::canonicalize(root.join("_shared")).ok())
                        .is_some_and(|(target, shared)| target == shared),
                "_shared is reserved for boop-start"
            );
        }
    }
    Ok(())
}
