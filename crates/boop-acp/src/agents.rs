//! The ACP agent roster as data: a vendored snapshot of the upstream registry,
//! which pins versions boop's own four argv rows floated on.

use anyhow::{Context, Result};
use serde_json::Value;

/// The snapshot this build was compiled with. Refreshed by an explicit verb,
/// never fetched at run time.
const SNAPSHOT: &str = include_str!("../registry/acp-agents.json");

/// Where a refresh reads from.
pub const SOURCE_URL: &str = "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

/// Which registry row each boop harness id names. The two spellings differ
/// because boop's ids predate the registry.
const HARNESS_AGENTS: &[(&str, &str)] = &[
    ("claude", "claude-acp"),
    ("codex", "codex-acp"),
    ("kimi", "kimi"),
    ("opencode", "opencode"),
];

/// One registry row reduced to what boop spawns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRow {
    pub id: String,
    pub name: String,
    pub version: String,
    /// `npx`, `uvx` or `binary`.
    pub distribution: String,
    /// The command to run, as the row describes it on this platform.
    pub argv: Vec<String>,
}

/// The registry id a boop harness id names.
pub fn agent_id(harness: &str) -> Option<&'static str> {
    HARNESS_AGENTS
        .iter()
        .find(|(id, _)| *id == harness)
        .map(|(_, agent)| *agent)
}

/// Every row in the compiled-in snapshot.
pub fn snapshot() -> Result<Vec<AgentRow>> {
    parse(SNAPSHOT)
}

/// The argv one harness id spawns, read out of the snapshot.
pub fn adapter(harness: &str) -> Result<Option<Vec<String>>> {
    let Some(wanted) = agent_id(harness) else {
        return Ok(None);
    };
    Ok(snapshot()?
        .into_iter()
        .find(|row| row.id == wanted)
        .map(|row| row.argv))
}

/// Parse a registry document. A row whose distribution boop cannot turn into
/// an argv is skipped rather than failing the whole read.
pub fn parse(json: &str) -> Result<Vec<AgentRow>> {
    let document: Value = serde_json::from_str(json).context("parse the acp agent registry")?;
    let agents = document
        .get("agents")
        .and_then(Value::as_array)
        .context("the acp agent registry has no `agents` array")?;
    let mut rows = Vec::with_capacity(agents.len());
    for agent in agents {
        let (Some(id), Some(distribution)) = (
            agent.get("id").and_then(Value::as_str),
            agent.get("distribution"),
        ) else {
            continue;
        };
        let Some((kind, argv)) = argv_for(distribution) else {
            continue;
        };
        rows.push(AgentRow {
            id: id.to_owned(),
            name: text(agent, "name"),
            version: text(agent, "version"),
            distribution: kind.to_owned(),
            argv,
        });
    }
    Ok(rows)
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The platform key a `binary` distribution lists this machine under.
fn platform() -> String {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86_64" => "x86_64",
        other => other,
    };
    format!("{}-{arch}", std::env::consts::OS)
}

fn argv_for(distribution: &Value) -> Option<(&'static str, Vec<String>)> {
    if let Some(npx) = distribution.get("npx") {
        let package = npx.get("package").and_then(Value::as_str)?;
        let mut argv = vec!["npx".to_owned(), "-y".to_owned(), package.to_owned()];
        argv.extend(args(npx));
        return Some(("npx", argv));
    }
    if let Some(uvx) = distribution.get("uvx") {
        let package = uvx.get("package").and_then(Value::as_str)?;
        let mut argv = vec!["uvx".to_owned(), package.to_owned()];
        argv.extend(args(uvx));
        return Some(("uvx", argv));
    }
    let binary = distribution.get("binary")?.as_object()?;
    // The archive is not fetched: the row is read for the command name and
    // its ACP-mode argument, which is what a locally installed copy answers to.
    let row = binary
        .get(&platform())
        .or_else(|| binary.values().next())?;
    let command = row.get("cmd").and_then(Value::as_str)?;
    let mut argv = vec![command.trim_start_matches("./").to_owned()];
    argv.extend(args(row));
    Some(("binary", argv))
}

fn args(row: &Value) -> Vec<String> {
    row.get("args")
        .and_then(Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RECEIPT: the four adapters the design lab probed on 2026-08-20 are all
    /// in the snapshot, and each argv matches what boop spawns today.
    #[test]
    fn every_boop_harness_resolves_to_an_argv() {
        assert_eq!(
            adapter("claude").unwrap().unwrap(),
            vec![
                "npx",
                "-y",
                "@agentclientprotocol/claude-agent-acp@0.70.0"
            ]
        );
        assert_eq!(
            adapter("codex").unwrap().unwrap(),
            vec!["npx", "-y", "@agentclientprotocol/codex-acp@1.6.2"]
        );
        assert_eq!(adapter("kimi").unwrap().unwrap(), vec!["kimi", "acp"]);
        assert_eq!(
            adapter("opencode").unwrap().unwrap(),
            vec!["opencode", "acp"]
        );
    }

    /// The snapshot pins a version where the compiled roster floated on the
    /// npm dist-tag, which is the whole reason it is read.
    #[test]
    fn the_npx_rows_are_version_pinned() {
        let row = snapshot()
            .unwrap()
            .into_iter()
            .find(|row| row.id == "claude-acp")
            .unwrap();
        assert_eq!(row.version, "0.70.0");
        assert!(row.argv[2].ends_with("@0.70.0"), "{:?}", row.argv);
    }

    #[test]
    fn a_harness_with_no_registry_row_is_none() {
        assert_eq!(agent_id("nothing-like-this"), None);
        assert!(adapter("nothing-like-this").unwrap().is_none());
    }

    /// A refresh is judged against this: the snapshot carries every agent the
    /// document lists, not a hand-picked four.
    #[test]
    fn the_snapshot_carries_the_whole_registry() {
        let rows = snapshot().unwrap();
        assert!(rows.len() >= 39, "{} rows", rows.len());
        assert!(rows.iter().any(|row| row.id == "gemini"));
        assert!(rows.iter().any(|row| row.id == "goose"));
    }

    /// A row boop cannot spawn is skipped; a malformed document is an error
    /// naming the file, never a silently empty roster.
    #[test]
    fn an_unspawnable_row_is_skipped_and_a_broken_document_is_an_error() {
        let rows = parse(r#"{"agents":[{"id":"x"},{"id":"y","distribution":{"docker":{}}}]}"#)
            .unwrap();
        assert!(rows.is_empty());
        assert!(parse("{}").is_err());
        assert!(parse("not json").is_err());
    }
}
