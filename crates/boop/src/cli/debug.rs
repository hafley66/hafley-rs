use std::path::Path;

use anyhow::Result;
use tracing::warn;

use boop::{config, lane};

use crate::cli::db::open_ro_store;
use crate::cli::{line, now_ms};
use crate::{ConfigCmd, HostCmd};

/// `boop debug`: the WARN/ERROR window, trail plus store.
pub(crate) fn run_debug(since: &str, lane: Option<&str>, json: bool) -> Result<()> {
    let window = boop::debug::parse_window(since)?;
    let since_ms = now_ms().saturating_sub(window.as_millis() as u64);
    let root = boop::trail::lanes_root()?;
    let mut alerts = boop::debug::trail_alerts(&root, since_ms, lane);
    match open_ro_store().and_then(|store| boop::debug::store_alerts(&store, since_ms, lane)) {
        Ok(rows) => alerts.extend(rows),
        Err(error) => warn!(error = %error, "trace error events unreadable"),
    }
    alerts.sort_by(|left, right| {
        left.lane
            .cmp(&right.lane)
            .then(left.at_ms.cmp(&right.at_ms))
    });
    match json {
        true => line(&serde_json::to_string_pretty(&boop::debug::as_json(
            &alerts,
        ))?),
        false => line(&boop::debug::report(&alerts, window)),
    }
    Ok(())
}

pub(crate) fn run_host(cmd: HostCmd) -> Result<()> {
    match cmd {
        HostCmd::Chat => {
            let response =
                match serde_json::from_reader::<_, boop::host::ChatRequest>(std::io::stdin()) {
                    Ok(request) => boop::host::run_chat(request),
                    Err(error) => boop::host::ChatResponse::Failed {
                        outcome: "failed",
                        detail: format!("read host chat JSON: {error}"),
                    },
                };
            println!("{}", serde_json::to_string(&response)?);
            Ok(())
        }
    }
}

/// An opencode route handed to `codex exec -m` is a broken invocation, so a
/// default preset whose model routes elsewhere goes unused.
pub(crate) fn default_preset_for_harness(
    config: &config::Config,
    config_path: &Path,
    harness_id: &str,
) -> Result<Option<String>> {
    let Some(preset) = config.default_model_preset.as_deref() else {
        return Ok(None);
    };
    let model = config::resolve_model(preset, config_path)?;
    match lane::harness_for_model(&model)? {
        Some(owner) if owner == harness_id => Ok(Some(preset.to_owned())),
        _ => Ok(None),
    }
}

/// `boop config path` prints the resolved config path; `boop config show`
/// prints the loaded config as pretty JSON, including the defaults a missing
/// file produces.
pub(crate) fn run_config(cmd: ConfigCmd) -> Result<()> {
    match cmd {
        ConfigCmd::Path => line(&config::default_path()?.display().to_string()),
        ConfigCmd::Show => line(&config::show(&config::default_path()?)?),
        ConfigCmd::Presets => line(&presets_table()?),
    }
    Ok(())
}

/// Each preset resolved to model, variant, and the harness the model spelling
/// names, with the `default-model-preset` row marked.
pub(crate) fn presets_table() -> Result<String> {
    let path = config::default_path()?;
    let config = config::load(&path)?;
    let mut rows: Vec<[String; 5]> = vec![[
        "PRESET".into(),
        "MODEL".into(),
        "VARIANT".into(),
        "HARNESS".into(),
        "DEFAULT".into(),
    ]];
    for name in config.model_presets.keys() {
        let preset = config::resolve_preset(name, &path)?;
        let harness = lane::harness_for_model(&preset.model)?
            .map(|harness| harness.into_owned())
            .unwrap_or_else(|| "?".to_owned());
        let default = if config.default_model_preset.as_deref() == Some(name) {
            "*"
        } else {
            ""
        };
        rows.push([
            name.clone(),
            preset.model,
            preset.variant.unwrap_or_default(),
            harness,
            default.to_owned(),
        ]);
    }
    if rows.len() == 1 {
        return Ok(format!("no model presets in {}", path.display()));
    }
    let mut widths = [0usize; 5];
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.len());
        }
    }
    let table = rows
        .iter()
        .map(|row| {
            row.iter()
                .zip(widths)
                .map(|(cell, width)| format!("{cell:<width$}"))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(table)
}
