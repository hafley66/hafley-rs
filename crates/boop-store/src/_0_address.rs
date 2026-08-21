//! Durable addresses for provider sessions and their children.
//!
//! The route registry names people and lanes.  This module names the
//! provider conversation behind a route, independently of a pane or process.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::proc::ProcessInfo;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct HarnessSessionId {
    pub harness: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControlEndpoint {
    CodexRemote {
        socket: PathBuf,
    },
    ClaudePeer {
        socket: PathBuf,
        pid: u32,
        proc_start: String,
    },
    AcpSession {
        agent: String,
        session_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessSession {
    pub id: HarnessSessionId,
    pub cwd: Option<PathBuf>,
    pub control: Option<ControlEndpoint>,
    pub observed_process: Option<u32>,
    pub observed_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChildAddress {
    pub parent: HarnessSessionId,
    pub child_id: String,
    pub kind: ChildKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChildKind {
    Direct,
    ParentMediated,
    ObservableOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentAddress {
    Session(HarnessSessionId),
    Child(ChildAddress),
    Lane { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueReason {
    MissingRoute,
    MissingSession,
    MissingControlEndpoint,
    StaleControlEndpoint,
    CompletedChild,
    UnsupportedChild,
    UnsupportedCapability(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryReceipt {
    Accepted,
    Delivered,
    ParentMediated { parent: HarnessSessionId },
    Queued { reason: QueueReason },
    Unsupported { capability: &'static str },
}

/// Process and pane evidence supplied to a harness adapter during discovery.
/// `ProcessInfo` remains owned by the process observation layer.
#[derive(Clone, Debug)]
pub struct ProcessObservation {
    pub pane: Option<String>,
    pub pane_pid: Option<u32>,
    pub descendants: Vec<ProcessInfo>,
    pub cwd: Option<PathBuf>,
}

pub type SessionMap = BTreeMap<HarnessSessionId, HarnessSession>;

/// File-backed session registry used by callers that need a durable handle
/// rather than passing the mail directory to every operation.
#[derive(Clone, Debug)]
pub struct SessionRegistry {
    pub dir: PathBuf,
}

impl SessionRegistry {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn load(&self) -> Result<SessionMap> {
        read_sessions(&self.dir)
    }

    pub fn upsert(&self, session: HarnessSession) -> Result<()> {
        upsert_session(&self.dir, session)
    }

    pub fn get(&self, id: &HarnessSessionId) -> Result<Option<HarnessSession>> {
        Ok(self.load()?.remove(id))
    }
}

pub fn sessions_path(dir: &Path) -> PathBuf {
    dir.join("sessions.json")
}

/// Read the additive session registry. Empty or missing files represent an
/// empty registry; malformed JSON is reported with its path.
pub fn read_sessions(dir: &Path) -> Result<SessionMap> {
    let path = sessions_path(dir);
    let text = match fs::read_to_string(&path) {
        Ok(text) if !text.trim().is_empty() => text,
        Ok(_) => return Ok(BTreeMap::new()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let rows: Vec<HarnessSession> = serde_json::from_str(&text)
        .with_context(|| format!("sessions.json is invalid JSON at {}", path.display()))?;
    Ok(rows.into_iter().map(|row| (row.id.clone(), row)).collect())
}

/// Atomically replace the session registry. Rows are sorted by the typed key
/// through `BTreeMap`, making writes deterministic.
pub fn write_sessions(dir: &Path, sessions: &SessionMap) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let rows: Vec<&HarnessSession> = sessions.values().collect();
    let bytes = serde_json::to_vec_pretty(&rows).context("serialize sessions")?;
    atomic_replace(&sessions_path(dir), &bytes)
}

pub fn upsert_session(dir: &Path, session: HarnessSession) -> Result<()> {
    let mut sessions = read_sessions(dir)?;
    sessions.insert(session.id.clone(), session);
    write_sessions(dir, &sessions)
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut output = bytes.to_vec();
    output.push(b'\n');
    fs::write(&tmp, output).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("boop-address-{}-{}", std::process::id(), tag));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn session(harness: &str, value: &str, socket: &str) -> HarnessSession {
        HarnessSession {
            id: HarnessSessionId {
                harness: harness.into(),
                value: value.into(),
            },
            cwd: Some(PathBuf::from("/repo")),
            control: Some(ControlEndpoint::CodexRemote {
                socket: socket.into(),
            }),
            observed_process: Some(4),
            observed_at_ms: 10,
        }
    }

    #[test]
    fn two_sessions_in_one_cwd_have_distinct_keys() {
        let dir = temp_dir("keys");
        upsert_session(&dir, session("codex", "thread-a", "/a")).unwrap();
        upsert_session(&dir, session("codex", "thread-b", "/b")).unwrap();
        assert_eq!(read_sessions(&dir).unwrap().len(), 2);
    }

    #[test]
    fn endpoint_replacement_keeps_identity() {
        let dir = temp_dir("replace");
        upsert_session(&dir, session("codex", "thread", "/old")).unwrap();
        upsert_session(&dir, session("codex", "thread", "/new")).unwrap();
        let row = read_sessions(&dir).unwrap().into_values().next().unwrap();
        assert_eq!(row.id.value, "thread");
        assert_eq!(
            row.control,
            Some(ControlEndpoint::CodexRemote {
                socket: "/new".into()
            })
        );
    }

    #[test]
    fn child_key_includes_parent() {
        let child_a = ChildAddress {
            parent: HarnessSessionId {
                harness: "claude".into(),
                value: "p-a".into(),
            },
            child_id: "agent-1".into(),
            kind: ChildKind::ParentMediated,
        };
        let child_b = ChildAddress {
            parent: HarnessSessionId {
                harness: "claude".into(),
                value: "p-b".into(),
            },
            ..child_a.clone()
        };
        assert_ne!(child_a, child_b);
    }
}
