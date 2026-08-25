use std::path::Path;

use anyhow::Result;
use tracing::warn;

use boop::harness::HarnessId;
use boop::{config, lane};

use crate::cli::db::open_ro_store;
use crate::cli::{line, now_ms};
use crate::{ConfigCmd, HostCmd};

/// `boop debug <lane>`: what happened to one lane, in the order a reader asks
/// it. Five sections, each of which prints `none` rather than nothing:
///
/// | # | section | source |
/// |---|---|---|
/// | 1 | route | the registry row, its liveness, and the store's last turn |
/// | 2 | mail | the last 5 rows to or from the lane, with the rung each landed on |
/// | 3 | worktree | `git log -5 --oneline` and the `git status --short` count |
/// | 4 | transcript | the last 3 assistant turns and the last 3 tool calls |
/// | 5 | alerts | the WARN/ERROR window, trail plus store |
pub(crate) fn run_lane_debug(lane: &str, since: &str, mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = crate::cli::mail_dir(mail_dir_arg)?;
    let routes = boop::bus::read_routes(&dir).unwrap_or_default();
    let route = routes.get(lane);
    line(&format!("== 1 route {lane} =="));
    match route {
        None => line("none"),
        Some(route) => {
            let liveness = match crate::cli::job::route_liveness(&dir, lane) {
                crate::cli::job::RouteLiveness::Live => "live",
                crate::cli::job::RouteLiveness::Dead => "dead",
                crate::cli::job::RouteLiveness::Unknown => "unknown",
            };
            line(&format!(
                "kind      {}\nharness   {}\nmodel     {}\nsession   {}\ncwd       {}\nparent    {}\ntmux      {}\nliveness  {liveness}",
                route.kind,
                route.harness.map_or("-", HarnessId::as_str),
                route.model.as_deref().unwrap_or("-"),
                route.session_id.as_deref().unwrap_or("-"),
                route.cwd.as_deref().unwrap_or("-"),
                route.parent.as_deref().unwrap_or("-"),
                route.tmux.as_deref().unwrap_or("-"),
            ));
            match last_turn_ms(route.session_id.as_deref()) {
                Some(at_ms) => line(&format!(
                    "last turn {at_ms} ({}s idle)",
                    now_ms().saturating_sub(at_ms) / 1000
                )),
                None => line("last turn none"),
            }
        }
    }

    line(&format!("\n== 2 mail {lane} =="));
    let mut rows = crate::cli::mail::all_messages(&dir).unwrap_or_default();
    rows.retain(|row| row.from == lane || row.to == lane);
    let recent: Vec<_> = rows.iter().rev().take(5).collect();
    match recent.is_empty() {
        true => line("none"),
        false => {
            let store = open_ro_store().ok();
            for row in recent.into_iter().rev() {
                let landed = store
                    .as_ref()
                    .and_then(|store| store.delivery_rows(&row.id).ok())
                    .unwrap_or_default()
                    .last()
                    .map(|transition| format!("{} ({})", transition.outcome, transition.detail))
                    .unwrap_or_else(|| "no delivery transition".to_owned());
                line(&format!(
                    "{} {} -> {} [{}] {landed}",
                    row.id, row.from, row.to, row.kind,
                ));
            }
        }
    }

    line(&format!("\n== 3 worktree {lane} =="));
    match route.and_then(|route| route.worktree_dir.as_deref().or(route.cwd.as_deref())) {
        None => line("none"),
        Some(tree) => {
            let log = git(tree, &["log", "-5", "--oneline"]);
            line(match log.trim().is_empty() {
                true => "none",
                false => log.trim(),
            });
            let dirty = git(tree, &["status", "--short"]);
            line(&format!(
                "dirty {}",
                dirty.lines().filter(|line| !line.trim().is_empty()).count()
            ));
        }
    }

    line(&format!("\n== 4 transcript {lane} =="));
    match route.and_then(|route| route.session_id.as_deref()) {
        None => line("none"),
        Some(session) => {
            print_tail(session, "assistant", 3);
            print_tail(session, "tool", 3);
        }
    }

    line(&format!("\n== 5 alerts {lane} =="));
    run_debug(since, Some(lane), false)
}

/// The last `count` turns of one role, oldest first, or `none`.
fn print_tail(session: &str, role: &str, count: usize) {
    let rows = open_ro_store()
        .and_then(|store| {
            store.query_turns(&boop::ident::TurnQuery {
                session: Some(session.to_owned()),
                role: Some(role.to_owned()),
                ..boop::ident::TurnQuery::default()
            })
        })
        .unwrap_or_default();
    let tail: Vec<_> = rows.iter().rev().take(count).collect();
    match tail.is_empty() {
        true => line(&format!("{role} none")),
        false => {
            for row in tail.into_iter().rev() {
                let said = row["said"].as_str().unwrap_or_default();
                let head: String = said.chars().take(160).collect();
                line(&format!("{role} #{} {head}", row["turn"]));
            }
        }
    }
}

/// The newest `agent_turn` timestamp for one session.
fn last_turn_ms(session: Option<&str>) -> Option<u64> {
    let session = session?;
    let rows = open_ro_store()
        .and_then(|store| {
            store.query_turns(&boop::ident::TurnQuery {
                session: Some(session.to_owned()),
                ..boop::ident::TurnQuery::default()
            })
        })
        .ok()?;
    rows.iter().filter_map(|row| row["ts"].as_u64()).max()
}

/// One git read in the lane's worktree. A tree git cannot answer reads empty,
/// which every caller renders as `none`.
fn git(tree: &str, args: &[&str]) -> String {
    std::process::Command::new("git")
        .args(["-C", tree])
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

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
    let passes = boop::trail::sync_trail_path()
        .map(|path| boop::trail::read_sync_trail(&path))
        .unwrap_or_default();
    let sync = boop::debug::sync_report(&passes, since_ms);
    match json {
        true => line(&serde_json::to_string_pretty(&serde_json::json!({
            "alerts": boop::debug::as_json(&alerts),
            "sync": boop::debug::sync_json(&passes, since_ms),
        }))?),
        false => {
            line(&boop::debug::report(&alerts, window));
            line(&sync);
        }
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
    harness_id: HarnessId,
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

/// Each preset resolved to model, variant, executable, and the harness the
/// model spelling names, with the `default-model-preset` row marked.
pub(crate) fn presets_table() -> Result<String> {
    let path = config::default_path()?;
    let config = config::load(&path)?;
    let mut rows: Vec<[String; 6]> = vec![[
        "PRESET".into(),
        "MODEL".into(),
        "VARIANT".into(),
        "BIN".into(),
        "HARNESS".into(),
        "DEFAULT".into(),
    ]];
    for name in config.model_presets.keys() {
        let preset = config::resolve_preset(name, &path)?;
        let harness = lane::harness_for_model(&preset.model)?
            .map_or("?", HarnessId::as_str)
            .to_owned();
        let default = if config.default_model_preset.as_deref() == Some(name) {
            "*"
        } else {
            ""
        };
        rows.push([
            name.clone(),
            preset.model,
            preset.variant.unwrap_or_default(),
            preset.bin.unwrap_or_default(),
            harness,
            default.to_owned(),
        ]);
    }
    if rows.len() == 1 {
        return Ok(format!("no model presets in {}", path.display()));
    }
    let mut widths = [0usize; 6];
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Sabotage receipt: dropping the harness-fit guard makes this assert the
    /// codex arm, spelling `codex exec -m openrouter/...`, which cannot run.
    #[test]
    fn the_default_preset_reaches_only_its_own_harness() {
        let dir = std::env::temp_dir().join("boop-default-preset-fit");
        std::fs::create_dir_all(&dir).expect("create the probe directory");
        let path = dir.join("config.json");
        std::fs::write(
            &path,
            r#"{ "default-model-preset": "flash4",
                 "model-presets": { "flash4": "openrouter/deepseek/deepseek-v4-flash-0731" } }"#,
        )
        .expect("write the probe config");
        let config = config::load(&path).expect("load the probe config");
        assert_eq!(
            default_preset_for_harness(&config, &path, HarnessId::Opencode).unwrap(),
            Some("flash4".to_owned())
        );
        assert_eq!(
            default_preset_for_harness(&config, &path, HarnessId::Codex).unwrap(),
            None
        );
    }
}
