//! The opencode door: the TUI is a client of `opencode serve`, so the same
//! HTTP API lists its sessions, prompts one, and streams turn-end events.
//!
//! Routes read from the server's own `/doc` on opencode 1.18.21:
//! `GET /session` (session.list), `GET /session/status` (session.status, a map
//! of session id to `{"type":"idle"|"busy"|"retry"}`),
//! `POST /session/{sessionID}/prompt_async` (204), `GET /event` (SSE).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Deserialize;
use url::Url;

use crate::door::{Delivered, Door, IdleNotice};
use crate::harness::{HarnessId, NativeTuiPlan, NativeTuiSpec};
use crate::live::{now_ms, DoorAddress, LiveSession, LiveSessions, LiveStatus};

/// Overrides the server a session list is read from.
pub const BASE_ENV: &str = "BOOP_OPENCODE_BASE";

/// Where boop's own `opencode serve` listens. 4096 is opencode's default and
/// an `opencode acp` lane on this machine already holds it; the TUIs boop
/// launches attach to this one.
const DEFAULT_BASE: &str = "http://127.0.0.1:4097/";

/// How long a freshly started server gets to answer `GET /session`.
const SERVE_START: Duration = Duration::from_secs(15);

/// A list or status read that has not answered by now has no server behind it.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Talks to one `opencode serve`.
pub struct OpencodeDoor {
    base: Option<Url>,
}

impl OpencodeDoor {
    /// The server on this machine's default port.
    pub const fn machine() -> Self {
        OpencodeDoor { base: None }
    }

    /// A server named outright, which is what a test hands in.
    pub fn at(base: Url) -> Self {
        OpencodeDoor { base: Some(base) }
    }

    fn base(&self) -> Result<Url> {
        if let Some(base) = &self.base {
            return Ok(base.clone());
        }
        let text = std::env::var(BASE_ENV)
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE.to_string());
        let text = if text.ends_with('/') {
            text
        } else {
            format!("{text}/")
        };
        Url::parse(&text).with_context(|| format!("parse opencode base url `{text}`"))
    }

    /// A server answering `GET /session` at `base`, started here when none
    /// does. The child is detached: it outlives the TUI and every later TUI
    /// attaches to the same one.
    fn ensure_server(&self, executable: &str, base: &Url) -> Result<()> {
        if self.get("session", Duration::from_secs(2)).is_ok() {
            return Ok(());
        }
        let port = base
            .port()
            .with_context(|| format!("opencode base `{base}` names no port"))?;
        let host = base.host_str().unwrap_or("127.0.0.1").to_owned();
        let mut command = Command::new(executable);
        command
            .args(["serve", "--port", &port.to_string(), "--hostname", &host])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Its own session: the pane that started it closing must not HUP it.
        // SAFETY: setsid is async-signal-safe and touches no shared state.
        unsafe {
            std::os::unix::process::CommandExt::pre_exec(&mut command, || {
                libc::setsid();
                Ok(())
            });
        }
        command
            .spawn()
            .with_context(|| format!("start `{executable} serve` on {base}"))?;
        let deadline = Instant::now() + SERVE_START;
        while Instant::now() < deadline {
            if self.get("session", Duration::from_secs(2)).is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        anyhow::bail!("`{executable} serve` did not answer on {base} within {SERVE_START:?}")
    }

    /// `POST /session` for `directory`; the id of the session the server made.
    fn create_session(&self, base: &Url, cwd: &std::path::Path) -> Result<String> {
        let url = base.join("session")?;
        let payload = serde_json::json!({ "directory": cwd });
        let mut response = agent(READ_TIMEOUT)
            .post(url.as_str())
            .header("content-type", "application/json")
            .send(serde_json::to_string(&payload)?)
            .context("opencode POST /session")?;
        let text = response.body_mut().read_to_string()?;
        let value: serde_json::Value = serde_json::from_str(&text).context("decode POST /session")?;
        value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .context("opencode POST /session answered without an id")
    }

    fn message_count(&self, session: &str) -> Result<usize> {
        let text = self.get(&format!("session/{session}/message"), READ_TIMEOUT)?;
        let messages: Vec<serde_json::Value> = serde_json::from_str(&text)?;
        Ok(messages.len())
    }

    /// `{providerID, modelID}` for a first prompt: the configured `model`
    /// when its provider lists it, else that provider's default. A config
    /// naming a retired model (`glm-4.6` on 2026-08-23) otherwise sends a
    /// prompt the server accepts and never answers.
    fn default_model(&self) -> Option<serde_json::Value> {
        let config: serde_json::Value =
            serde_json::from_str(&self.get("config", READ_TIMEOUT).ok()?).ok()?;
        let configured = config.get("model")?.as_str()?;
        let (provider, model) = configured.split_once('/')?;
        let providers: serde_json::Value =
            serde_json::from_str(&self.get("config/providers", READ_TIMEOUT).ok()?).ok()?;
        let listed = providers
            .get("providers")?
            .as_array()?
            .iter()
            .find(|entry| entry.get("id").and_then(serde_json::Value::as_str) == Some(provider))?
            .get("models")?;
        let model = if listed.get(model).is_some() {
            model.to_owned()
        } else {
            providers
                .pointer(&format!("/default/{provider}"))?
                .as_str()?
                .to_owned()
        };
        Some(serde_json::json!({ "providerID": provider, "modelID": model }))
    }

    fn get(&self, path: &str, timeout: Duration) -> Result<String> {
        let url = self.base()?.join(path)?;
        let agent = agent(timeout);
        let mut response = agent.get(url.as_str()).call()?;
        Ok(response.body_mut().read_to_string()?)
    }

    fn statuses(&self) -> BTreeMap<String, LiveStatus> {
        let Ok(text) = self.get("session/status", READ_TIMEOUT) else {
            return BTreeMap::new();
        };
        serde_json::from_str::<BTreeMap<String, StatusEntry>>(&text)
            .map(|map| {
                map.into_iter()
                    .map(|(id, entry)| (id, entry.status()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into()
}

/// One entry of `GET /session`.
#[derive(Deserialize)]
struct SessionEntry {
    id: String,
    #[serde(default)]
    directory: Option<String>,
    #[serde(default)]
    time: SessionTime,
}

#[derive(Deserialize, Default)]
struct SessionTime {
    #[serde(default)]
    updated: Option<u64>,
    #[serde(default)]
    created: Option<u64>,
}

/// One entry of `GET /session/status`.
#[derive(Deserialize)]
struct StatusEntry {
    #[serde(rename = "type")]
    kind: String,
}

impl StatusEntry {
    fn status(&self) -> LiveStatus {
        match self.kind.as_str() {
            "idle" => LiveStatus::Idle,
            "busy" | "retry" => LiveStatus::Busy,
            _ => LiveStatus::Unknown,
        }
    }
}

/// One SSE payload of `GET /event`.
#[derive(Deserialize)]
struct EventLine {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    properties: EventProperties,
}

#[derive(Deserialize, Default)]
struct EventProperties {
    #[serde(rename = "sessionID", default)]
    session_id: Option<String>,
}

impl LiveSessions for OpencodeDoor {
    /// The sessions the running server holds. A server that does not answer
    /// is a server that is not running, so the list is empty rather than an
    /// error, and the sessions come back newest update first.
    fn live_sessions(&self) -> Result<Vec<LiveSession>> {
        let Ok(text) = self.get("session", READ_TIMEOUT) else {
            return Ok(Vec::new());
        };
        let entries: Vec<SessionEntry> =
            serde_json::from_str(&text).context("decode opencode session list")?;
        let statuses = self.statuses();
        let base = self.base()?;
        let mut live = entries
            .into_iter()
            .map(|entry| LiveSession {
                harness: HarnessId::Opencode,
                status: statuses
                    .get(&entry.id)
                    .copied()
                    .unwrap_or(LiveStatus::Unknown),
                door: DoorAddress::Http {
                    base: base.clone(),
                    session: entry.id.clone(),
                },
                observed_ms: entry
                    .time
                    .updated
                    .or(entry.time.created)
                    .unwrap_or_else(now_ms),
                cwd: entry.directory.map(PathBuf::from),
                // The server records neither the pid nor the pane of an
                // attached TUI; a route supplies those.
                pid: None,
                tmux_pane: None,
                started_ms: entry.time.created,
                session_id: entry.id,
            })
            .collect::<Vec<_>>();
        live.sort_by_key(|session| std::cmp::Reverse(session.observed_ms));
        Ok(live)
    }
}

impl Door for OpencodeDoor {
    /// The TUI attaches to boop's server, so the session it opens is one
    /// `live_sessions` lists and `deliver` can reach.
    fn tui_launch(&self, spec: &NativeTuiSpec) -> Result<NativeTuiPlan> {
        let base = self.base()?;
        self.ensure_server(&spec.executable, &base)?;
        // The session exists before the TUI attaches, so the route names it
        // from the start and the first hail is its first prompt.
        let session = self.create_session(&base, &spec.cwd)?;
        let mut args: Vec<std::ffi::OsString> = vec![
            "attach".into(),
            base.as_str().into(),
            "--session".into(),
            session.clone().into(),
        ];
        args.extend(spec.args.iter().map(std::ffi::OsString::from));
        Ok(NativeTuiPlan {
            program: spec.executable.clone(),
            args,
            mode: "native-remote".into(),
            session_id: Some(session.clone()),
            source_path: Some(format!("managed-opencode-serve={base};started-session={session}")),
            app_server_socket: Some(base.to_string()),
        })
    }

    fn deliver(&self, session: &LiveSession, body: &str) -> Result<Delivered> {
        let DoorAddress::Http { base, session: id } = &session.door else {
            return Ok(Delivered::Unreachable(format!(
                "opencode session `{}` names no server",
                session.session_id
            )));
        };
        let url = base.join(&format!("session/{id}/prompt_async"))?;
        let mut payload = serde_json::json!({
            "parts": [{ "type": "text", "text": body }],
        });
        // A session with no turn yet has no model; the server stays silent
        // rather than refusing, so the first prompt names one.
        if self.message_count(id).unwrap_or(0) == 0 {
            if let Some(model) = self.default_model() {
                payload["model"] = model;
            }
        }
        let sent = agent(READ_TIMEOUT)
            .post(url.as_str())
            .header("content-type", "application/json")
            .send(serde_json::to_string(&payload)?);
        match sent {
            // The route starts the session's turn and returns at once.
            Ok(response) if response.status().is_success() => Ok(Delivered::Injected),
            Ok(response) => Ok(Delivered::Unreachable(format!(
                "opencode {} answered {}",
                url,
                response.status()
            ))),
            Err(error) => Ok(Delivered::Unreachable(format!("opencode {url}: {error}"))),
        }
    }

    /// The status map answers when the session is already idle; otherwise the
    /// event stream carries a `session.idle` for this session id.
    fn notify_idle(&self, session: &LiveSession, timeout: Duration) -> Result<IdleNotice> {
        if self.statuses().get(&session.session_id) == Some(&LiveStatus::Idle) {
            return Ok(IdleNotice::now(Some("idle".into())));
        }
        let deadline = Instant::now() + timeout;
        let url = self.base()?.join("event")?;
        let response = agent(timeout).get(url.as_str()).call()?;
        let reader = BufReader::new(response.into_body().into_reader());
        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(error) => anyhow::bail!("opencode event stream: {error}"),
            };
            if Instant::now() >= deadline {
                break;
            }
            let Some(payload) = line.strip_prefix("data: ") else {
                continue;
            };
            let Ok(event) = serde_json::from_str::<EventLine>(payload) else {
                continue;
            };
            if event.kind == "session.idle"
                && event.properties.session_id.as_deref() == Some(session.session_id.as_str())
            {
                return Ok(IdleNotice::now(Some(event.kind)));
            }
        }
        anyhow::bail!(
            "opencode session `{}` reported no idle event within {timeout:?}",
            session.session_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;

    /// An HTTP server on a loopback port that answers the four routes this
    /// door calls and records the request bodies it was sent.
    struct Stub {
        base: Url,
        seen: mpsc::Receiver<(String, String)>,
    }

    impl Stub {
        fn start(sessions: &'static str, statuses: &'static str, events: &'static str) -> Stub {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let (sender, seen) = mpsc::channel();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { break };
                    let sender = sender.clone();
                    std::thread::spawn(move || {
                        serve(stream, sessions, statuses, events, sender);
                    });
                }
            });
            Stub {
                base: Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap(),
                seen,
            }
        }

        fn door(&self) -> OpencodeDoor {
            OpencodeDoor::at(self.base.clone())
        }
    }

    fn serve(
        mut stream: TcpStream,
        sessions: &str,
        statuses: &str,
        events: &str,
        sender: mpsc::Sender<(String, String)>,
    ) {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(0) | Err(_) => return,
                Ok(_) => head.push(byte[0]),
            }
        }
        let head = String::from_utf8_lossy(&head).to_string();
        let target = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/")
            .to_string();
        let length = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            })
            .unwrap_or(0);
        let mut body = vec![0u8; length];
        if length > 0 && stream.read_exact(&mut body).is_err() {
            return;
        }
        let _ = sender.send((target.clone(), String::from_utf8_lossy(&body).to_string()));

        let write_json = |stream: &mut TcpStream, payload: &str| {
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                payload.len()
            );
        };
        match target.as_str() {
            "/session" => write_json(&mut stream, sessions),
            "/session/status" => write_json(&mut stream, statuses),
            "/event" => {
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n"
                );
                let _ = stream.write_all(events.as_bytes());
            }
            path if path.ends_with("/prompt_async") => {
                let _ = write!(
                    stream,
                    "HTTP/1.1 204 No Content\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                );
            }
            _ => {
                let _ = write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                );
            }
        }
        let _ = stream.flush();
    }

    const SESSIONS: &str = r#"[
      {"id":"ses_old","directory":"/Users/someone/old","title":"older","time":{"created":100,"updated":100}},
      {"id":"ses_new","directory":"/Users/someone/projects","title":"newer","time":{"created":200,"updated":900}}
    ]"#;
    const STATUSES: &str = r#"{"ses_new":{"type":"busy"},"ses_old":{"type":"idle"}}"#;
    const EVENTS: &str = "data: {\"id\":\"evt_1\",\"type\":\"server.connected\",\"properties\":{}}\n\ndata: {\"id\":\"evt_2\",\"type\":\"session.idle\",\"properties\":{\"sessionID\":\"ses_new\"}}\n\n";

    /// RECEIPT. `GET /session` plus `GET /session/status` become live
    /// sessions, newest update first, each addressed by its own server.
    #[test]
    fn the_session_list_becomes_live_sessions() {
        let stub = Stub::start(SESSIONS, STATUSES, EVENTS);
        let live = stub.door().live_sessions().unwrap();
        assert_eq!(
            live.iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ses_new", "ses_old"]
        );
        assert_eq!(live[0].harness, HarnessId::Opencode);
        assert_eq!(live[0].status, LiveStatus::Busy);
        assert_eq!(live[1].status, LiveStatus::Idle);
        assert_eq!(live[0].observed_ms, 900);
        assert_eq!(live[0].cwd, Some(PathBuf::from("/Users/someone/projects")));
        assert_eq!(
            live[0].door,
            DoorAddress::Http {
                base: stub.base.clone(),
                session: "ses_new".into(),
            }
        );
    }

    /// RECEIPT. A delivery posts one text part to prompt_async and reads the
    /// 204 as injected.
    #[test]
    fn a_delivery_posts_one_text_part() {
        let stub = Stub::start(SESSIONS, STATUSES, EVENTS);
        let door = stub.door();
        let session = door
            .live_sessions()
            .unwrap()
            .into_iter()
            .find(|session| session.session_id == "ses_new")
            .unwrap();
        assert_eq!(
            door.deliver(&session, "ping from boop").unwrap(),
            Delivered::Injected
        );
        let posted = std::iter::from_fn(|| stub.seen.try_recv().ok())
            .find(|(target, _)| target.ends_with("/prompt_async"))
            .expect("prompt_async request");
        assert_eq!(posted.0, "/session/ses_new/prompt_async");
        let body: serde_json::Value = serde_json::from_str(&posted.1).unwrap();
        assert_eq!(body["parts"][0]["type"], "text");
        assert_eq!(body["parts"][0]["text"], "ping from boop");
    }

    /// RECEIPT. A busy session's idle arrives on the event stream; the
    /// stream's other events are ignored.
    #[test]
    fn the_event_stream_reports_idle() {
        let stub = Stub::start(SESSIONS, STATUSES, EVENTS);
        let door = stub.door();
        let session = door
            .live_sessions()
            .unwrap()
            .into_iter()
            .find(|session| session.session_id == "ses_new")
            .unwrap();
        let notice = door.notify_idle(&session, Duration::from_secs(5)).unwrap();
        assert_eq!(notice.status_line.as_deref(), Some("session.idle"));
        assert!(notice.at_ms > 0);
    }

    /// RECEIPT. A session the status map already calls idle needs no stream.
    #[test]
    fn an_already_idle_session_answers_from_the_status_map() {
        let stub = Stub::start(SESSIONS, STATUSES, EVENTS);
        let door = stub.door();
        let session = door
            .live_sessions()
            .unwrap()
            .into_iter()
            .find(|session| session.session_id == "ses_old")
            .unwrap();
        assert_eq!(
            door.notify_idle(&session, Duration::from_millis(50))
                .unwrap()
                .status_line
                .as_deref(),
            Some("idle")
        );
    }

    /// RECEIPT. No server on the port is no sessions, not a raised error.
    #[test]
    fn a_server_that_is_not_running_lists_nothing() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let door = OpencodeDoor::at(Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap());
        assert!(door.live_sessions().unwrap().is_empty());
    }
}
