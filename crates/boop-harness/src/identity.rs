//! Who is calling: the `--as` flag, then the env stamp boop writes into every
//! process it spawns. No pane lookup, no process tree (issue env-only-identity).

use std::collections::BTreeMap;

use anyhow::Result;

use boop_store::bus::Route;

/// The rung that produced an identity. Ordered most explicit first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rung {
    /// `--as <name>`: the caller named itself on the command line.
    As,
    /// `BOOP_SESSION` (or `BOOP_LANE` from a pre-2026-08-24 spawn).
    Env,
    /// Neither rung answered.
    None,
}

impl Rung {
    pub fn as_str(self) -> &'static str {
        match self {
            Rung::As => "as",
            Rung::Env => "env",
            Rung::None => "none",
        }
    }

    /// Where the name came from: the caller's own word, or the stamp its
    /// spawner wrote.
    pub fn confidence(self) -> &'static str {
        match self {
            Rung::As => "named",
            Rung::Env => "stamped",
            Rung::None => "unresolved",
        }
    }
}

/// The caller's own identity. Every field is optional because an unresolved
/// caller is a real answer.
#[derive(Clone, Debug, Default)]
pub struct Identity {
    pub session: Option<String>,
    pub lane: Option<String>,
    pub parent: Option<String>,
    pub harness: Option<String>,
    pub pane: Option<String>,
    pub rung: Option<Rung>,
}

impl Identity {
    pub fn to_json(&self) -> serde_json::Value {
        let rung = self.rung.unwrap_or(Rung::None);
        serde_json::json!({
            "session": self.session,
            "lane": self.lane,
            "parent": self.parent,
            "harness": self.harness,
            "pane": self.pane,
            "rung": rung.as_str(),
            "confidence": rung.confidence(),
        })
    }

    /// Whether either rung named this caller.
    pub fn is_resolved(&self) -> bool {
        matches!(self.rung, Some(Rung::As) | Some(Rung::Env))
    }
}

/// The one line an unresolved caller reads. It names `--as` because that is
/// the only thing the caller can do about it.
pub const UNRESOLVED: &str =
    "boop cannot tell who is calling: this process carries no BOOP_SESSION stamp; name yourself with `--as <name>`";

/// The exit code an unresolved caller exits with.
pub const UNRESOLVED_EXIT: i32 = 2;

/// The whole ladder: `--as`, then the env stamp. A caller neither rung names
/// carries `Rung::None`.
pub fn resolve_as(as_name: Option<&str>) -> Identity {
    if let Some(name) = as_name.filter(|name| !name.is_empty()) {
        return Identity {
            session: Some(name.to_owned()),
            lane: Some(name.to_owned()),
            parent: env("BOOP_PARENT"),
            harness: env("BOOP_HARNESS"),
            pane: env("TMUX_PANE"),
            rung: Some(Rung::As),
        };
    }
    from_env().unwrap_or(Identity {
        rung: Some(Rung::None),
        ..Default::default()
    })
}

/// The env rung alone, for a caller with no flag to offer. Kept as the old
/// name so no call site outside this module changes.
pub fn resolve(_routes: &BTreeMap<String, Route>) -> Result<Identity> {
    Ok(resolve_as(None))
}

/// The env rung alone. The registry argument is vestigial: no harness owns a
/// rung any more.
pub fn resolve_with(
    _registry: &crate::registry::Registry,
    _routes: &BTreeMap<String, Route>,
) -> Result<Identity> {
    Ok(resolve_as(None))
}

/// The caller, or one line and exit 2. Every verb that must put a name on
/// something calls this rather than inventing a fallback.
pub fn require(as_name: Option<&str>) -> Identity {
    let identity = resolve_as(as_name);
    if identity.is_resolved() {
        return identity;
    }
    eprintln!("{UNRESOLVED}");
    std::process::exit(UNRESOLVED_EXIT)
}

/// The stamp a boop spawn wrote into the child's own environment. `BOOP_LANE`
/// alone answers for a spawn made before boop stamped `BOOP_SESSION`.
pub fn from_env() -> Option<Identity> {
    let session = env("BOOP_SESSION");
    let lane = env("BOOP_LANE");
    if session.is_none() && lane.is_none() {
        return None;
    }
    Some(Identity {
        session: session.clone().or_else(|| lane.clone()),
        lane: lane.or(session),
        parent: env("BOOP_PARENT"),
        harness: env("BOOP_HARNESS"),
        pane: env("TMUX_PANE"),
        rung: Some(Rung::Env),
    })
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

/// The env stamp a spawn writes into its CHILD. Every value describes the
/// child; the spawner appears only as `BOOP_PARENT`.
pub fn child_stamp(session: &str, lane: &str, harness: &str, parent: Option<&str>) -> String {
    let mut stamp = format!(
        "BOOP_SESSION={} BOOP_LANE={} BOOP_HARNESS={}",
        shell_word(session),
        shell_word(lane),
        shell_word(harness)
    );
    if let Some(parent) = parent {
        stamp.push_str(&format!(" BOOP_PARENT={}", shell_word(parent)));
    }
    stamp
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::{child_stamp, resolve_as, Rung};

    /// The flag wins over a stamp: a native subagent inherits its spawner's
    /// env and must be able to say it is somebody else.
    #[test]
    fn the_as_flag_outranks_the_env_stamp() {
        temp_env::with_vars(
            [
                ("BOOP_SESSION", Some("spawner-1")),
                ("BOOP_LANE", Some("spawner-1")),
                ("BOOP_PARENT", Some("coord")),
            ],
            || {
                let identity = resolve_as(Some("native-n1"));
                assert_eq!(identity.rung, Some(Rung::As));
                assert_eq!(identity.session.as_deref(), Some("native-n1"));
                assert_eq!(identity.lane.as_deref(), Some("native-n1"));
                assert_eq!(identity.parent.as_deref(), Some("coord"));
                assert_eq!(identity.to_json()["confidence"], "named");
            },
        );
    }

    #[test]
    fn the_env_stamp_is_the_second_rung() {
        temp_env::with_vars(
            [
                ("BOOP_SESSION", Some("s-1")),
                ("BOOP_LANE", Some("lane-a")),
                ("BOOP_PARENT", Some("coord")),
            ],
            || {
                let identity = resolve_as(None);
                assert_eq!(identity.rung, Some(Rung::Env));
                assert_eq!(identity.session.as_deref(), Some("s-1"));
                assert_eq!(identity.lane.as_deref(), Some("lane-a"));
                assert_eq!(identity.to_json()["rung"], "env");
            },
        );
    }

    /// A spawn from before the `BOOP_SESSION` stamp still carries `BOOP_LANE`.
    #[test]
    fn boop_lane_alone_answers_for_an_old_spawn() {
        temp_env::with_vars(
            [
                ("BOOP_SESSION", None::<&str>),
                ("BOOP_LANE", Some("lane-a")),
                ("BOOP_PARENT", None::<&str>),
            ],
            || {
                let identity = resolve_as(None);
                assert_eq!(identity.rung, Some(Rung::Env));
                assert_eq!(identity.lane.as_deref(), Some("lane-a"));
                assert_eq!(identity.session.as_deref(), Some("lane-a"));
            },
        );
    }

    /// No pane, no process tree: a caller with neither rung is unresolved and
    /// nothing plausible is invented for it.
    #[test]
    fn a_caller_with_neither_rung_is_unresolved() {
        temp_env::with_vars(
            [
                ("BOOP_SESSION", None::<&str>),
                ("BOOP_LANE", None::<&str>),
                ("TMUX_PANE", Some("%1206")),
                ("CODEX_THREAD_ID", Some("thread-7")),
                ("CLAUDE_CODE_SESSION_ID", Some("555ec3f8")),
            ],
            || {
                let identity = resolve_as(None);
                assert_eq!(identity.rung, Some(Rung::None));
                assert!(identity.session.is_none());
                assert!(identity.lane.is_none());
                assert!(!identity.is_resolved());
                assert_eq!(identity.to_json()["confidence"], "unresolved");
            },
        );
    }

    /// The bus incident, in one assertion: the stamp describes the CHILD, and
    /// the spawner appears only as the parent.
    #[test]
    fn the_child_stamp_never_carries_the_spawners_session_as_its_own() {
        let stamp = child_stamp("child-1", "lane-a", "opencode", Some("coordinator-9"));
        assert!(stamp.contains("BOOP_SESSION='child-1'"));
        assert!(stamp.contains("BOOP_LANE='lane-a'"));
        assert!(stamp.contains("BOOP_PARENT='coordinator-9'"));
        assert!(
            !stamp.contains("BOOP_SESSION='coordinator-9'"),
            "the spawner's id must never become the child's own: {stamp}"
        );
    }

    #[test]
    fn a_stamp_with_no_parent_omits_the_variable() {
        let stamp = child_stamp("child-1", "lane-a", "claude", None);
        assert!(!stamp.contains("BOOP_PARENT"), "{stamp}");
    }

    mod temp_env {
        /// Env is process-global; these tests set and restore it around one
        /// closure and are marked serial by running inside a single mutex.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        pub fn with_vars<const N: usize>(vars: [(&str, Option<&str>); N], body: impl FnOnce()) {
            let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
            let saved: Vec<(String, Option<String>)> = vars
                .iter()
                .map(|(key, _)| ((*key).to_owned(), std::env::var(key).ok()))
                .collect();
            for (key, value) in &vars {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
            body();
            for (key, value) in saved {
                match value {
                    Some(value) => std::env::set_var(&key, value),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }
}
