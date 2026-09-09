//! Claude Code inbox hook protocol and settings handling.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

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
    let user = crate::harness::reader_home()
        .ok()
        .map(|home| home.join(".claude").join("settings.json"));
    [Some(settings_path(cwd)), user]
        .into_iter()
        .flatten()
        .filter_map(|path| read_settings_value(&path))
        .any(|settings| drains_by_hook(&settings, name))
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
