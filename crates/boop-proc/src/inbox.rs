//! Generic inbox draining with compatibility exports for Claude hook users.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use boop_store::bus::Message;

pub use boop_harness::door::claude_hooks::{
    Hook, drain_command, drains_by_hook, install, installed_for, settings_path, uninstall,
};

/// One drained batch as the agent reads it, each row rendered through the
/// draining session's effective mood.
pub fn batch_text(rows: &[Message], template: &str) -> String {
    rows.iter()
        .map(|row| {
            crate::supervise::render_mail(template, row.kind.as_str(), &row.id, &row.from, &row.body)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The ledger of ids already handed to an agent. The bus ack is the durable
/// record; this file makes a second drain a no-op even if that write lost a
/// race, which is the property the interim shell hooks were built on.
pub fn ledger_path(mail_dir: &Path, name: &str) -> PathBuf {
    mail_dir.join(format!("inbox-drained.{name}"))
}

pub fn drained(path: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// One open, one write, whatever the batch size.
pub fn record_drained(path: &Path, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create the drained-id ledger's directory")?;
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open the drained-id ledger {}", path.display()))?;
    let mut batch = ids.join("\n");
    batch.push('\n');
    file.write_all(batch.as_bytes())
        .context("append to the drained-id ledger")
}

/// Every row addressed to `name` that neither the bus nor the ledger records as
/// handed over.
pub fn undelivered(rows: &[Message], name: &str, already: &BTreeSet<String>) -> Vec<Message> {
    crate::mailwait::unread_for(rows, name)
        .into_iter()
        .filter(|row| !already.contains(&row.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map, Value};
    use super::*;

    fn message(id: &str, to: &str, body: &str) -> Message {
        Message {
            id: id.into(),
            from: "coordinator".into(),
            to: to.into(),
            from_timestamp: "2026-08-17T00:00:00.000Z".into(),
            to_timestamp: None,
            kind: "hail".into(),
            reply_to: None,
            body: body.into(),
            r#ref: None,
            rc: None,
            detail: None,
        }
    }

    /// A mood that shares no shape with the default, so a render test cannot
    /// pass by accident.
    const FIXTURE_MOOD: &str = "{kind} {from} -> {id}\n{body}";

    fn plain(rows: &[Message]) -> String {
        batch_text(rows, boop_store::ident::DEFAULT_MOOD_TEMPLATE)
    }

    fn settings(name: &str) -> Map<String, Value> {
        let mut settings = Map::new();
        install(&mut settings, name);
        settings
    }

    /// The installer and the reader must not be able to disagree, so both name
    /// the command through `drain_command`.
    #[test]
    fn each_hook_runs_the_drain_for_its_own_event() {
        assert_eq!(
            drain_command("sprefa-coordinator", Hook::Stop),
            "boop inbox drain --as sprefa-coordinator --hook stop"
        );
        assert_eq!(
            drain_command("sprefa-coordinator", Hook::Prompt),
            "boop inbox drain --as sprefa-coordinator --hook prompt"
        );
        assert_eq!(Hook::Stop.event(), Some("Stop"));
        assert_eq!(Hook::Prompt.event(), Some("UserPromptSubmit"));
        assert_eq!(Hook::Plain.event(), None);
    }

    #[test]
    fn an_install_writes_both_events_under_the_hooks_key() {
        let settings = settings("coord");
        let hooks = settings["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), 2);
        for event in ["Stop", "UserPromptSubmit"] {
            let groups = hooks[event].as_array().unwrap();
            assert_eq!(groups.len(), 1, "{event}");
            let entry = &groups[0]["hooks"].as_array().unwrap()[0];
            assert_eq!(entry["type"], "command");
            assert_eq!(entry["timeout"], 10);
        }
    }

    // FAIL-PRE-FIX: an installer that appends without looking gives a
    // coordinator two Stop hooks and the mail arrives twice.
    #[test]
    fn a_second_install_adds_nothing() {
        let mut settings = settings("coord");
        assert_eq!(install(&mut settings, "coord"), 0);
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    /// A project that already hooks other things keeps them; only this
    /// coordinator's own entries are added and removed.
    #[test]
    fn an_install_leaves_a_foreign_hook_alone() {
        let mut settings: Map<String, Value> = serde_json::from_str(
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"make lint"}]}]}}"#,
        )
        .unwrap();
        assert_eq!(install(&mut settings, "coord"), 2);
        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "make lint");
        assert_eq!(uninstall(&mut settings, "coord"), 2);
        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert_eq!(stop[0]["hooks"][0]["command"], "make lint");
        assert!(settings["hooks"].get("UserPromptSubmit").is_none());
    }

    /// An uninstall that leaves empty arrays behind leaves a settings file
    /// nobody can read as "no hooks here".
    #[test]
    fn an_uninstall_prunes_the_containers_it_empties() {
        let mut settings = settings("coord");
        assert_eq!(uninstall(&mut settings, "coord"), 2);
        assert!(settings.get("hooks").is_none(), "{settings:?}");
        assert_eq!(uninstall(&mut settings, "coord"), 0);
    }

    // FAIL-PRE-FIX: with no way to read the routing decision off the settings
    // file, `deliver_hail` typed into every coordinator pane it could see.
    #[test]
    fn the_installed_stop_hook_is_the_routing_decision() {
        let mut settings = settings("coord");
        let value = Value::Object(settings.clone());
        assert!(drains_by_hook(&value, "coord"));
        assert!(
            !drains_by_hook(&value, "other-coord"),
            "one coordinator's hooks must not speak for another"
        );
        uninstall(&mut settings, "coord");
        assert!(!drains_by_hook(&Value::Object(settings), "coord"));
        assert!(!drains_by_hook(&json!({}), "coord"));
    }

    #[test]
    fn a_batch_names_the_id_and_the_sender_of_every_row() {
        let text = plain(&[
            message("m1", "coord", "first"),
            message("m2", "coord", "second"),
        ]);
        assert_eq!(
            text,
            "[boop m1 from coordinator] first\n\n[boop m2 from coordinator] second"
        );
        assert_eq!(plain(&[]), "");
    }

    // FAIL-PRE-FIX: a hand-formatted JSON string broke on a quote or a newline
    // in the mail body, and the hook's output stopped parsing.
    #[test]
    fn the_stop_payload_is_json_whatever_the_body_holds() {
        let text = plain(&[message("m1", "coord", "say \"stop\"\nnow")]);
        let payload: Value = serde_json::from_str(&Hook::Stop.payload(&text)).unwrap();
        assert_eq!(payload["decision"], "block");
        assert_eq!(
            payload["reason"].as_str().unwrap(),
            "boop inbox:\n\n[boop m1 from coordinator] say \"stop\"\nnow"
        );
    }

    #[test]
    fn the_prompt_payload_is_the_mail_as_plain_context() {
        let text = plain(&[message("m1", "coord", "read me")]);
        assert_eq!(
            Hook::Prompt.payload(&text),
            "boop inbox:\n\n[boop m1 from coordinator] read me"
        );
        assert_eq!(Hook::Plain.payload(&text), Hook::Prompt.payload(&text));
    }

    /// The drain is one of the three delivery paths a mood reaches. Every row
    /// of the batch takes the shape, and the batch separator survives it.
    #[test]
    fn a_drained_batch_renders_through_the_draining_session_mood() {
        let text = batch_text(
            &[
                message("m1", "coord", "first"),
                message("m2", "coord", "second"),
            ],
            FIXTURE_MOOD,
        );
        assert_eq!(
            text,
            "hail coordinator -> m1\nfirst\n\nhail coordinator -> m2\nsecond"
        );
    }

    // FAIL-PRE-FIX: the ledger is what makes a drain idempotent when the bus ack
    // lost a race, which is the property the interim shell hooks ran on.
    #[test]
    fn a_drained_id_is_never_offered_again() {
        let dir = std::env::temp_dir().join(format!("boop-inbox-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ledger = ledger_path(&dir, "coord");
        assert_eq!(ledger.file_name().unwrap(), "inbox-drained.coord");
        let rows = vec![message("m1", "coord", "one"), message("m2", "other", "two")];
        let first = undelivered(&rows, "coord", &drained(&ledger));
        assert_eq!(first.len(), 1, "only this coordinator's mail: {first:?}");
        record_drained(&ledger, &["m1".to_owned()]).unwrap();
        assert!(undelivered(&rows, "coord", &drained(&ledger)).is_empty());
        // A second batch appends rather than replacing the first.
        record_drained(&ledger, &["m3".to_owned(), "m4".to_owned()]).unwrap();
        assert_eq!(drained(&ledger).len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An acked row is history: the drain must not replay what a `boop wait`
    /// already took delivery of.
    #[test]
    fn an_acked_row_is_not_undelivered() {
        let mut acked = message("m1", "coord", "one");
        acked.to_timestamp = Some("2026-08-17T00:00:01.000Z".into());
        assert!(undelivered(&[acked], "coord", &BTreeSet::new()).is_empty());
    }

    #[test]
    fn the_settings_file_is_the_projects_own_claude_settings() {
        assert_eq!(
            settings_path(Path::new("/repo")),
            Path::new("/repo/.claude/settings.json")
        );
    }
}
