//! Persistent coordinator sessions through the `acpx` ACP client.

use std::io::{self, BufRead, Write};
use std::path::Path;

use anyhow::{Context, Result};

use boop::bus::Route;
use boop::config;
use boop_acp::channel::acpx;
use boop::harness::HarnessId;

use crate::cli::{mail_dir, write_route};

const DEFAULT_ROUTE: &str = "coordinator";
fn resolve_agent_and_model(preset: &str) -> Result<(String, Option<String>)> {
    if acpx::recognizes_agent(preset) {
        return Ok((preset.to_owned(), None));
    }
    let path = config::default_path()?;
    let row = config::resolve_preset(preset, &path)?;
    let agent = boop::lane::harness_for_preset(&row)?
        .context("model preset does not select an ACP agent")?;
    // acpx spells effort in brackets; the model string itself stays bare.
    let model = match row.effort.as_deref() {
        Some(effort) => format!("{}[{effort}]", row.model),
        None => row.model,
    };
    Ok((agent.as_str().to_owned(), Some(model)))
}

pub(crate) fn run_foreground(
    preset: &str,
    name: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let cwd = std::env::current_dir().context("read coordinator cwd")?;
    let (agent, model) = resolve_agent_and_model(preset)?;
    let name = name.unwrap_or(DEFAULT_ROUTE);
    let dir = mail_dir(mail_dir_arg)?;
    let routes = boop::bus::read_routes(&dir)?;
    let reusable = routes.get(name).is_some_and(|route| {
        route.kind == "coordinator"
            && route.mode.as_deref() == Some("acpx")
            && acpx::route_agent(route) == Some(agent.as_str())
            && route.cwd.as_deref() == Some(cwd.to_string_lossy().as_ref())
    });
    if !reusable {
        acpx::ensure(&agent, name, model.as_deref(), &cwd)?;
    }
    write_route(
        &dir,
        name,
        Route {
            kind: "coordinator".into(),
            harness: HarnessId::parse(&agent),
            tmux: std::env::var("TMUX_PANE").ok(),
            cwd: Some(cwd.display().to_string()),
            model,
            mode: Some("acpx".into()),
            session_id: Some(name.into()),
            source_path: Some(format!("acpx-agent={agent}")),
            parent: None,
            goal: Some("foreground ACP coordinator".into()),
            registered_at: Some(boop::bus::now_iso()),
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        },
    )?;

    println!("registered {name} -> {agent} ACPX session");
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let route = boop::bus::read_routes(&dir)?
            .remove(name)
            .context("coordinator route disappeared")?;
        let response = acpx::prompt(&route, &line, false)?;
        stdout.write_all(response.as_bytes())?;
        if !response.ends_with('\n') {
            stdout.write_all(b"\n")?;
        }
        stdout.flush()?;
    }
    Ok(())
}
