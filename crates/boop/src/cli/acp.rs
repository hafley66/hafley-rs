//! `boop acp host` and `boop acp attach`: the resident session host and the
//! stdio shim an unmodified ACP client spawns in place of an agent binary.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use boop::bus;

use super::{line, mail_dir};

/// The adapter argv for one harness id, out of the vendored registry snapshot.
/// The compiled roster is the fallback for a harness the registry has dropped.
fn adapter_for(harness: &str) -> Result<Vec<String>> {
    if let Some(argv) = boop_acp::agents::adapter(harness)? {
        return Ok(argv);
    }
    let row: &[&str] = match harness {
        "claude" => boop_acp::channel::acp::CLAUDE_ADAPTER,
        "codex" => boop_acp::channel::acp::CODEX_ADAPTER,
        "kimi" => boop_acp::channel::acp::KIMI_ADAPTER,
        "opencode" => boop_acp::channel::acp::OPENCODE_ADAPTER,
        other => anyhow::bail!("harness `{other}` has no acp adapter in the roster"),
    };
    Ok(row.iter().map(|part| (*part).to_owned()).collect())
}

/// Print the vendored registry snapshot, one row per agent.
pub(crate) fn run_acp_agents(refresh_from: Option<&str>, out: Option<&Path>) -> Result<()> {
    let Some(source) = refresh_from else {
        for row in boop_acp::agents::snapshot()? {
            line(&format!(
                "{}\t{}\t{}\t{}",
                row.id,
                row.version,
                row.distribution,
                row.argv.join(" ")
            ));
        }
        return Ok(());
    };
    let json = read_source(source)?;
    let rows = boop_acp::agents::parse(&json)?;
    line(&format!("{} agents from {source}", rows.len()));
    let Some(out) = out else {
        line("pass --out <PATH> to write it; nothing was written");
        return Ok(());
    };
    std::fs::write(out, &json)
        .with_context(|| format!("write the acp agent registry to {}", out.display()))?;
    line(&format!("wrote {}", out.display()));
    Ok(())
}

/// Read a refresh source. A URL goes through `curl`, which is the OS's HTTP
/// client; boop links none of its own for one explicit verb.
fn read_source(source: &str) -> Result<String> {
    let source = match source {
        "upstream" => boop_acp::agents::SOURCE_URL,
        named => named,
    };
    if !source.starts_with("http://") && !source.starts_with("https://") {
        return std::fs::read_to_string(source)
            .with_context(|| format!("read the acp agent registry from {source}"));
    }
    let fetched = std::process::Command::new("curl")
        .args(["-fsSL", source])
        .output()
        .with_context(|| format!("run curl against {source}"))?;
    anyhow::ensure!(
        fetched.status.success(),
        "curl failed on {source}: {}",
        String::from_utf8_lossy(&fetched.stderr)
    );
    String::from_utf8(fetched.stdout).context("the acp agent registry is not utf-8")
}

/// Own one route's ACP session for as long as this process lives.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_acp_host(
    route: &str,
    harness: Option<&str>,
    model: Option<&str>,
    cwd: Option<&Path>,
    resume: Option<&str>,
    poll_ms: Option<u64>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let registered = bus::read_routes(&dir)?.get(route).cloned();
    let harness = harness
        .map(str::to_owned)
        .or_else(|| registered.as_ref().and_then(|row| row.harness.clone()))
        .with_context(|| format!("route `{route}` names no harness; pass --harness"))?;
    let cwd: PathBuf = cwd
        .map(Path::to_path_buf)
        .or_else(|| {
            registered
                .as_ref()
                .and_then(|row| row.cwd.clone())
                .map(PathBuf::from)
        })
        .map(Ok)
        .unwrap_or_else(|| std::env::current_dir().context("read the current directory"))?;
    // A host that outlived its process resumes the id the last one pinned;
    // nothing new is persisted for restart to work.
    let resume = resume
        .map(str::to_owned)
        .or_else(|| registered.as_ref().and_then(|row| row.session_id.clone()));
    let spec = boop_acp::host::HostSpec {
        route: route.to_owned(),
        adapter: adapter_for(&harness)?,
        cwd,
        model: model
            .map(str::to_owned)
            .or_else(|| registered.as_ref().and_then(|row| row.model.clone())),
        resume,
        mail_dir: dir.clone(),
        poll: poll_ms
            .map(std::time::Duration::from_millis)
            .unwrap_or(boop_acp::host::POLL),
    };
    line(&format!(
        "acp host {route} on {}",
        boop_acp::host::socket_path(&dir, route).display()
    ));
    boop_acp::host::run(spec)
}

/// Pump this process's stdio onto the route's host socket.
pub(crate) fn run_acp_attach(route: &str, mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    boop_acp::host::attach(&dir, route)
}

/// Print whether each route has a host answering on its socket.
pub(crate) fn run_acp_list(mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    for (name, route) in bus::read_routes(&dir)? {
        let alive = boop_acp::host::route_host_alive(&dir, &name);
        line(&format!(
            "{name}\t{}\t{}\t{}",
            route.kind,
            match alive {
                true => "host-live",
                false => "no-host",
            },
            boop_acp::host::socket_path(&dir, &name).display()
        ));
    }
    Ok(())
}
