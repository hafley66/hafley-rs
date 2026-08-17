//! Mail a claude coordinator reads at a turn boundary instead of having it
//! typed into its pane.
//!
//! Two hooks do the delivery: `Stop` returns the mail as a block decision so the
//! model continues with it as input, and `UserPromptSubmit` prints it as context
//! on the next prompt. The installed hook is also the routing decision, so
//! `deliver_hail` reads it rather than a second registry field that could
//! disagree with it, and uninstalling restores pane injection by itself.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};

use crate::bus::Message;

/// Seconds a hook may take. The drain is two small file reads and one append.
const HOOK_TIMEOUT_SECS: u64 = 10;
/// The line every drained batch opens with, in both hook shapes.
const BANNER: &str = "boop inbox:";

/// Which hook is asking, and so what shape the drained mail is printed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    /// Claude Code's `Stop`: the mail comes back as a block decision.
    Stop,
    /// Claude Code's `UserPromptSubmit`: the mail is printed as context.
    Prompt,
    /// A human or a script reading the inbox; plain text, no hook contract.
    Plain,
}

impl Hook {
    /// The `--hook` spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Hook::Stop => "stop",
            Hook::Prompt => "prompt",
            Hook::Plain => "plain",
        }
    }

    /// The Claude Code hook event this drain is wired to, `None` for a plain
    /// read that no settings file installs.
    pub fn event(self) -> Option<&'static str> {
        match self {
            Hook::Stop => Some("Stop"),
            Hook::Prompt => Some("UserPromptSubmit"),
            Hook::Plain => None,
        }
    }

    /// What the drained batch looks like on stdout.
    pub fn payload(self, text: &str) -> String {
        match self {
            // A hand-built JSON string would break on a quote or a newline in
            // the mail body; the two-key object is serialized, never formatted.
            Hook::Stop => json!({
                "decision": "block",
                "reason": format!("{BANNER}\n\n{text}"),
            })
            .to_string(),
            Hook::Prompt | Hook::Plain => format!("{BANNER}\n\n{text}"),
        }
    }

    /// The events a coordinator's settings install, in install order.
    pub fn installed() -> [Hook; 2] {
        [Hook::Stop, Hook::Prompt]
    }
}

/// The settings file a coordinator's hooks live in.
pub fn settings_path(cwd: &Path) -> PathBuf {
    cwd.join(".claude").join("settings.json")
}

/// The command one hook runs. The only spelling of it: installer and reader
/// agree because both call this.
pub fn drain_command(name: &str, hook: Hook) -> String {
    format!("boop inbox drain --as {name} --hook {}", hook.as_str())
}

/// Add both hooks, once each. Returns how many were missing beforehand, so a
/// second install reports 0 and writes nothing new.
pub fn install(settings: &mut Map<String, Value>, name: &str) -> usize {
    let mut added = 0;
    for hook in Hook::installed() {
        let Some(event) = hook.event() else {
            continue;
        };
        let command = drain_command(name, hook);
        let groups = event_groups(settings, event);
        if groups.iter().any(|group| group_runs(group, &command)) {
            continue;
        }
        groups.push(json!({
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": HOOK_TIMEOUT_SECS,
            }],
        }));
        added += 1;
    }
    added
}

/// Remove every boop drain hook for `name`, pruning the containers it emptied.
/// Returns how many hook entries went.
pub fn uninstall(settings: &mut Map<String, Value>, name: &str) -> usize {
    let commands: Vec<String> = Hook::installed()
        .into_iter()
        .map(|hook| drain_command(name, hook))
        .collect();
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return 0;
    };
    let mut removed = 0;
    for (_, groups) in hooks.iter_mut() {
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        for group in groups.iter_mut() {
            let Some(entries) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                continue;
            };
            let before = entries.len();
            entries.retain(|entry| !commands.iter().any(|command| runs(entry, command)));
            removed += before - entries.len();
        }
        groups.retain(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|entries| !entries.is_empty())
        });
    }
    hooks.retain(|_, groups| groups.as_array().is_none_or(|groups| !groups.is_empty()));
    if hooks.is_empty() {
        settings.remove("hooks");
    }
    removed
}

/// Whether `name` drains its own mail through hooks in this settings object.
/// The `Stop` hook is the delivery leg, so it is the one that decides.
pub fn drains_by_hook(settings: &Value, name: &str) -> bool {
    let command = drain_command(name, Hook::Stop);
    settings
        .get("hooks")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|hooks| hooks.values())
        .filter_map(Value::as_array)
        .flatten()
        .any(|group| group_runs(group, &command))
}

/// Whether the coordinator working in `cwd` drains by hook. The project file
/// answers first; a coordinator may also have installed into its user settings.
pub fn installed_for(cwd: &Path, name: &str) -> bool {
    let user = dirs::home_dir().map(|home| home.join(".claude").join("settings.json"));
    [Some(settings_path(cwd)), user]
        .into_iter()
        .flatten()
        .filter_map(|path| read_settings_value(&path))
        .any(|settings| drains_by_hook(&settings, name))
}

/// One drained batch as the agent reads it. The id and the sender ride each
/// entry so a reply can name what it answers.
pub fn batch_text(rows: &[Message]) -> String {
    rows.iter()
        .map(|row| format!("[boop {} from {}] {}", row.id, row.from, row.body))
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

fn read_settings_value(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The mutable group array under one event, created empty if absent.
fn event_groups<'a>(settings: &'a mut Map<String, Value>, event: &str) -> &'a mut Vec<Value> {
    settings
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("hooks is an object")
        .entry(event)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("an event holds an array of hook groups")
}

fn group_runs(group: &Value, command: &str) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|entries| entries.iter().any(|entry| runs(entry, command)))
}

fn runs(entry: &Value, command: &str) -> bool {
    entry.get("command").and_then(Value::as_str) == Some(command)
}

#[cfg(test)]
mod tests {
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
        }
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
            assert_eq!(entry["timeout"], HOOK_TIMEOUT_SECS);
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
        let text = batch_text(&[message("m1", "coord", "first"), message("m2", "coord", "second")]);
        assert_eq!(
            text,
            "[boop m1 from coordinator] first\n\n[boop m2 from coordinator] second"
        );
        assert_eq!(batch_text(&[]), "");
    }

    // FAIL-PRE-FIX: a hand-formatted JSON string broke on a quote or a newline
    // in the mail body, and the hook's output stopped parsing.
    #[test]
    fn the_stop_payload_is_json_whatever_the_body_holds() {
        let text = batch_text(&[message("m1", "coord", "say \"stop\"\nnow")]);
        let payload: Value = serde_json::from_str(&Hook::Stop.payload(&text)).unwrap();
        assert_eq!(payload["decision"], "block");
        assert_eq!(
            payload["reason"].as_str().unwrap(),
            "boop inbox:\n\n[boop m1 from coordinator] say \"stop\"\nnow"
        );
    }

    #[test]
    fn the_prompt_payload_is_the_mail_as_plain_context() {
        let text = batch_text(&[message("m1", "coord", "read me")]);
        assert_eq!(
            Hook::Prompt.payload(&text),
            "boop inbox:\n\n[boop m1 from coordinator] read me"
        );
        assert_eq!(Hook::Plain.payload(&text), Hook::Prompt.payload(&text));
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
