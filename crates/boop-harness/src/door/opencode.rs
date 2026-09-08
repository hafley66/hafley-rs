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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Deserialize;
use url::Url;

use crate::door::{Delivered, Door, IdleNotice};
use crate::harness::{HarnessId, NativeTuiEvent, NativeTuiObserver, NativeTuiPlan, NativeTuiSpec};
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

    /// Borrow an explicitly addressed server, or own the process started here.
    /// The plan holds it before readiness checks, including startup errors.
    fn ensure_server(
        &self,
        spec: &NativeTuiSpec,
        base: &Url,
        plan: &mut NativeTuiPlan,
    ) -> Result<()> {
        if self.get("session", Duration::from_secs(2)).is_ok() {
            return Ok(());
        }
        let port = base
            .port()
            .with_context(|| format!("opencode base `{base}` names no port"))?;
        let host = base.host_str().unwrap_or("127.0.0.1").to_owned();
        let mut command = Command::new(&spec.executable);
        command
            .args(["serve", "--port", &port.to_string(), "--hostname", &host])
            .envs(spec.env.iter().cloned())
            .current_dir(&spec.cwd)
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
        plan.backend = Some(
            command
                .spawn()
                .with_context(|| format!("start `{} serve` on {base}", spec.executable))?,
        );
        let deadline = Instant::now() + SERVE_START;
        while Instant::now() < deadline {
            if let Some(status) = plan.backend.as_mut().unwrap().try_wait()? {
                anyhow::bail!("opencode server exited during startup: {status}");
            }
            if self.get("session", Duration::from_secs(2)).is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        anyhow::bail!(
            "`{} serve` did not answer on {base} within {SERVE_START:?}",
            spec.executable
        )
    }

    /// `POST /session` for `directory`; the id of the session the server made.
    fn create_session(&self, base: &Url, cwd: &std::path::Path) -> Result<String> {
        // The server reads the directory from the query string only; a
        // `directory` field in the body is ignored and the session lands in
        // the server's own cwd.
        let mut url = base.join("session")?;
        url.query_pairs_mut()
            .append_pair("directory", &cwd.display().to_string());
        let mut response = agent(READ_TIMEOUT)
            .post(url.as_str())
            .header("content-type", "application/json")
            .send("{}")
            .context("opencode POST /session")?;
        let text = response.body_mut().read_to_string()?;
        let value: serde_json::Value =
            serde_json::from_str(&text).context("decode POST /session")?;
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

    /// Preserve the configured model exactly. Provider rejection remains an
    /// execution outcome; a different provider default is not authorization.
    fn default_model(&self) -> Option<serde_json::Value> {
        let configured = self.configured_model()?;
        let (provider, model) = configured.split_once('/')?;
        Some(serde_json::json!({ "providerID": provider, "modelID": model }))
    }

    fn configured_model(&self) -> Option<String> {
        let config: serde_json::Value =
            serde_json::from_str(&self.get("config", READ_TIMEOUT).ok()?).ok()?;
        config.get("model")?.as_str().map(str::to_owned)
    }

    fn get(&self, path: &str, timeout: Duration) -> Result<String> {
        let url = self.base()?.join(path)?;
        self.get_url(url.as_str(), timeout)
    }

    fn get_url(&self, url: &str, timeout: Duration) -> Result<String> {
        let agent = agent(timeout);
        let mut response = agent.get(url).call()?;
        Ok(response.body_mut().read_to_string()?)
    }

    /// Every worktree directory `GET /project` names, the server's own cwd
    /// included.
    fn project_worktrees(&self) -> Vec<String> {
        let Ok(text) = self.get("project", READ_TIMEOUT) else {
            return Vec::new();
        };
        serde_json::from_str::<Vec<serde_json::Value>>(&text)
            .map(|projects| {
                projects
                    .iter()
                    .filter_map(|project| project.get("worktree")?.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
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

    /// Observe the selected TUI session from OpenCode's own event stream.
    /// The directory query binds this subscriber to the launched instance;
    /// no transcript discovery participates in `/clear` rebinding.
    fn observe(
        &self,
        cwd: &std::path::Path,
        initial_session: Option<&str>,
    ) -> Result<NativeTuiObserver> {
        let mut url = self.base()?.join("event")?;
        url.query_pairs_mut()
            .append_pair("directory", &cwd.display().to_string());
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (sender, events) = mpsc::channel();
        let configured_model = self.configured_model();
        if let Some(session_id) = initial_session {
            sender.send(NativeTuiEvent::Session {
                session_id: session_id.to_owned(),
                model: configured_model.clone(),
                effort: None,
            })?;
        }
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                let response = agent(Duration::from_secs(1)).get(url.as_str()).call();
                let Ok(response) = response else {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                };
                let reader = BufReader::new(response.into_body().into_reader());
                for line in reader.lines() {
                    if worker_stop.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok(line) = line else { break };
                    let Some(payload) = line.strip_prefix("data:").map(str::trim) else {
                        continue;
                    };
                    let Ok(event) = serde_json::from_str::<EventLine>(payload) else {
                        continue;
                    };
                    if let Some(mut event) = native_event(event) {
                        if let NativeTuiEvent::Session { model, .. } = &mut event {
                            if model.is_none() {
                                *model = configured_model.clone();
                            }
                        }
                        if sender.send(event).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        Ok(NativeTuiObserver {
            events,
            stop,
            worker: Some(worker),
        })
    }

    fn for_route(route: &boop_store::bus::Route) -> Result<(Self, String)> {
        let base = route
            .app_server_socket
            .as_deref()
            .context("OpenCode route has no observed server")?;
        let session = route
            .session_id
            .as_deref()
            .context("OpenCode route has no selected session")?;
        Ok((Self::at(Url::parse(base)?), session.to_owned()))
    }
}

/// Root-TUI arguments that `attach` does not accept are applied to the owned
/// server. OPENCODE_CONFIG_CONTENT is an existing native process override;
/// no user configuration file is written.
fn native_request(spec: &NativeTuiSpec) -> Result<(NativeTuiSpec, Option<String>, bool)> {
    let mut prepared = spec.clone();
    prepared.args.clear();
    let mut resume = None;
    let mut model = None;
    let mut args = spec.args.iter();
    while let Some(arg) = args.next() {
        let (option, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(key, value)| (key, Some(value)));
        match option {
            "--session" | "-s" => {
                resume = Some(
                    inline
                        .map(str::to_owned)
                        .or_else(|| args.next().cloned())
                        .context("opencode session flag requires an id")?,
                )
            }
            "--model" | "-m" => {
                model = Some(
                    inline
                        .map(str::to_owned)
                        .or_else(|| args.next().cloned())
                        .context("opencode model flag requires provider/model")?,
                )
            }
            _ => prepared.args.push(arg.clone()),
        }
    }
    let explicit_model = model.is_some();
    if let Some(model) = model {
        anyhow::ensure!(
            model
                .split_once('/')
                .is_some_and(|(provider, name)| !provider.is_empty() && !name.is_empty()),
            "opencode model requires provider/model"
        );
        const CONFIG: &str = "OPENCODE_CONFIG_CONTENT";
        let content = spec
            .env
            .iter()
            .rev()
            .find(|(key, _)| key == CONFIG)
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var(CONFIG).ok());
        let mut config: serde_json::Value = content
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .context("decode native OPENCODE_CONFIG_CONTENT")?
            .unwrap_or_else(|| serde_json::json!({}));
        anyhow::ensure!(
            config.is_object(),
            "native OPENCODE_CONFIG_CONTENT must be a JSON object"
        );
        config["model"] = model.into();
        prepared.env.retain(|(key, _)| key != CONFIG);
        prepared
            .env
            .push((CONFIG.into(), serde_json::to_string(&config)?));
    }
    anyhow::ensure!(
        resume.as_ref().is_none_or(|id| !id.is_empty()),
        "opencode session flag requires an id"
    );
    Ok((prepared, resume, explicit_model))
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
    #[serde(default)]
    data: EventProperties,
}

#[derive(Deserialize)]
struct EventHistory {
    data: Vec<EventLine>,
}

#[derive(Deserialize, Default)]
struct EventProperties {
    #[serde(rename = "sessionID", default)]
    session_id: Option<String>,
    #[serde(default)]
    model: Option<EventModel>,
}

#[derive(Deserialize)]
struct EventModel {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "modelID", default)]
    model_id: Option<String>,
    #[serde(rename = "providerID", default)]
    provider_id: Option<String>,
    #[serde(default)]
    variant: Option<String>,
}

fn native_event(event: EventLine) -> Option<NativeTuiEvent> {
    let payload = if event.data.session_id.is_some() {
        event.data
    } else {
        event.properties
    };
    let session_id = payload.session_id?;
    match event.kind.as_str() {
        "tui.session.select" => Some(NativeTuiEvent::Session {
            session_id,
            model: None,
            effort: None,
        }),
        "session.next.model.switched" => {
            let model = payload.model?;
            let provider = model.provider_id?;
            let name = model.id.or(model.model_id)?;
            Some(NativeTuiEvent::Settings {
                session_id,
                model: Some(format!("{provider}/{name}")),
                effort: model.variant,
            })
        }
        _ => None,
    }
}

impl LiveSessions for OpencodeDoor {
    fn live_session_for_route(
        &self,
        route: &boop_store::bus::Route,
    ) -> Result<Option<LiveSession>> {
        let Some(id) = route.session_id.as_deref() else {
            return Ok(None);
        };
        let observed = route
            .app_server_socket
            .as_deref()
            .map(Url::parse)
            .transpose()?;
        let door = observed.map(Self::at);
        let source = door.as_ref().unwrap_or(self);
        Ok(source
            .live_sessions()?
            .into_iter()
            .find(|session| session.session_id == id))
    }
    /// The sessions the running server holds. A server that does not answer
    /// is a server that is not running, so the list is empty rather than an
    /// error, and the sessions come back newest update first.
    fn live_sessions(&self) -> Result<Vec<LiveSession>> {
        let Ok(text) = self.get("session", READ_TIMEOUT) else {
            return Ok(Vec::new());
        };
        let mut entries: Vec<SessionEntry> =
            serde_json::from_str(&text).context("decode opencode session list")?;
        // `GET /session` is scoped to the server's own cwd; every other
        // directory the server has a project for is listed by name.
        let mut seen: std::collections::BTreeSet<String> =
            entries.iter().map(|entry| entry.id.clone()).collect();
        for worktree in self.project_worktrees() {
            let mut url = match self.base().and_then(|base| Ok(base.join("session")?)) {
                Ok(url) => url,
                Err(_) => break,
            };
            url.query_pairs_mut().append_pair("directory", &worktree);
            let Ok(text) = self.get_url(url.as_str(), READ_TIMEOUT) else {
                continue;
            };
            let Ok(more) = serde_json::from_str::<Vec<SessionEntry>>(&text) else {
                continue;
            };
            entries.extend(
                more.into_iter()
                    .filter(|entry| seen.insert(entry.id.clone())),
            );
        }
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
                scope: crate::live::LiveSessionScope::Unknown,
                parent_session: None,
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
        let (prepared, resume, explicit_model) = native_request(spec)?;
        let spec = &prepared;
        let base =
            if self.base.is_some() || std::env::var_os(BASE_ENV).is_some_and(|v| !v.is_empty()) {
                self.base()?
            } else {
                let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
                Url::parse(&format!("http://{}/", listener.local_addr()?))?
            };
        let source = Self::at(base.clone());
        let mut plan = NativeTuiPlan::direct(spec);
        source.ensure_server(spec, &base, &mut plan)?;
        anyhow::ensure!(!explicit_model || plan.backend.is_some(),
            "an OpenCode model override requires an owned backend; the configured server is borrowed");
        // The session exists before the TUI attaches, so the route names it
        // from the start and the first hail is its first prompt.
        let session = match resume {
            Some(id) => {
                let _: serde_json::Value =
                    serde_json::from_str(&source.get(&format!("session/{id}"), READ_TIMEOUT)?)?;
                id
            }
            None => source.create_session(&base, &spec.cwd)?,
        };
        let mut args: Vec<std::ffi::OsString> = vec![
            "attach".into(),
            base.as_str().into(),
            "--session".into(),
            session.clone().into(),
            // The shared server keeps the cwd it was started in; without
            // `--dir` the TUI runs there instead of in this pane's directory.
            "--dir".into(),
            spec.cwd.as_os_str().to_owned(),
        ];
        args.extend(spec.args.iter().map(std::ffi::OsString::from));
        plan.args = args;
        plan.mode = if plan.backend.is_some() {
            "native-owned"
        } else {
            "native-remote"
        }
        .into();
        plan.session_id = Some(session.clone());
        plan.source_path = Some(format!(
            "managed-opencode-serve={base};started-session={session}"
        ));
        plan.app_server_socket = Some(base.to_string());
        plan.observer = Some(source.observe(&spec.cwd, Some(&session))?);
        Ok(plan)
    }

    fn deliver(&self, session: &LiveSession, body: &str) -> Result<Delivered> {
        // prompt_async joins an active generation instead of preserving two
        // independent turns. Leave the envelope pending for the wrapper's
        // existing idle drain so the initiating prompt can finish first.
        if session.status == LiveStatus::Busy {
            return Ok(Delivered::Unreachable(
                "opencode is busy; awaiting idle delivery".into(),
            ));
        }
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
        let source = Self::at(base.clone());
        if source.message_count(id).unwrap_or(0) == 0 {
            if let Some(model) = source.default_model() {
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
        let DoorAddress::Http { base, .. } = &session.door else {
            anyhow::bail!(
                "opencode session {} has no HTTP address",
                session.session_id
            );
        };
        let source = Self::at(base.clone());
        if source.statuses().get(&session.session_id) == Some(&LiveStatus::Idle) {
            return Ok(IdleNotice::now(Some("idle".into())));
        }
        let deadline = Instant::now() + timeout;
        let url = base.join("event")?;
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

    fn native_route_settings(
        &self,
        route: &boop_store::bus::Route,
    ) -> Result<Option<NativeTuiEvent>> {
        let (source, session) = Self::for_route(route)?;
        let history: EventHistory = serde_json::from_str(
            &source.get(&format!("api/session/{session}/history"), READ_TIMEOUT)?,
        )?;
        Ok(history.data.into_iter().rev().find_map(native_event))
    }

    fn change_native_settings(
        &self,
        route: &boop_store::bus::Route,
        model: &str,
        effort: &str,
    ) -> Result<NativeTuiEvent> {
        let (provider, id) = model
            .split_once('/')
            .filter(|(provider, id)| !provider.is_empty() && !id.is_empty())
            .context("OpenCode model requires provider/model")?;
        let (source, session) = Self::for_route(route)?;
        let url = source
            .base()?
            .join(&format!("api/session/{session}/model"))?;
        let response = agent(READ_TIMEOUT)
            .post(url.as_str())
            .header("content-type", "application/json")
            .send(serde_json::to_string(&serde_json::json!({
                "id": id,
                "providerID": provider,
                "variant": effort,
            }))?)?;
        anyhow::ensure!(
            response.status().is_success(),
            "OpenCode model control answered {}",
            response.status()
        );
        let expected = NativeTuiEvent::Settings {
            session_id: session,
            model: Some(model.to_owned()),
            effort: Some(effort.to_owned()),
        };
        anyhow::ensure!(
            source.native_route_settings(route)? == Some(expected.clone()),
            "OpenCode did not persist the requested model and effort"
        );
        Ok(expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;

    #[test]
    fn selected_session_and_durable_model_events_become_native_events() {
        let selected: EventLine = serde_json::from_str(
            r#"{"type":"tui.session.select","properties":{"sessionID":"ses_after_clear"}}"#,
        )
        .unwrap();
        assert_eq!(
            native_event(selected),
            Some(NativeTuiEvent::Session {
                session_id: "ses_after_clear".into(),
                model: None,
                effort: None,
            })
        );
        let switched: EventLine = serde_json::from_str(
            r#"{"type":"session.next.model.switched","data":{"sessionID":"ses_after_clear","model":{"id":"model-b","providerID":"provider-a","variant":"high"}}}"#,
        )
        .unwrap();
        assert_eq!(
            native_event(switched),
            Some(NativeTuiEvent::Settings {
                session_id: "ses_after_clear".into(),
                model: Some("provider-a/model-b".into()),
                effort: Some("high".into()),
            })
        );
    }

    #[test]
    fn native_model_is_owned_backend_configuration() {
        let spec = NativeTuiSpec {
            executable: "opencode".into(),
            cwd: PathBuf::from("/tmp"),
            args: vec![
                "--session=ses_test",
                "--model",
                "provider/model",
                "--print-logs",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            env: vec![(
                "OPENCODE_CONFIG_CONTENT".into(),
                r#"{"model":"old/model","theme":"test"}"#.into(),
            )],
        };
        let (prepared, resume, explicit) = native_request(&spec).unwrap();
        assert_eq!(resume.as_deref(), Some("ses_test"));
        assert!(explicit);
        assert_eq!(prepared.args, ["--print-logs"]);
        let config: serde_json::Value = serde_json::from_str(&prepared.env[0].1).unwrap();
        assert_eq!(
            config,
            serde_json::json!({"model":"provider/model","theme":"test"})
        );
        assert_eq!(spec.args.len(), 4);
        for args in [vec!["--model"], vec!["--model=bare"], vec!["--session="]] {
            let mut invalid = spec.clone();
            invalid.args = args.into_iter().map(str::to_owned).collect();
            assert!(native_request(&invalid).is_err());
        }
    }

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
            "/config" => write_json(&mut stream, r#"{"model":"fixture/retired"}"#),
            path if path == "/event" || path.starts_with("/event?") => {
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
            path if path.ends_with("/model") => {
                let _ = write!(
                    stream,
                    "HTTP/1.1 204 No Content\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                );
            }
            path if path.ends_with("/history") => write_json(
                &mut stream,
                r#"{"data":[{"type":"session.next.model.switched","data":{"sessionID":"ses_new","model":{"id":"model","providerID":"fixture","variant":"high"}}}]}"#,
            ),
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

    #[test]
    fn observer_subscribes_to_the_exact_directory_and_reports_clear_selection() {
        let events = "data: {\"type\":\"tui.session.select\",\"properties\":{\"sessionID\":\"ses_after_clear\"}}\n\n";
        let stub = Stub::start(SESSIONS, STATUSES, events);
        let observer = stub
            .door()
            .observe(std::path::Path::new("/fixture/project"), None)
            .unwrap();
        assert_eq!(
            observer
                .events
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            NativeTuiEvent::Session {
                session_id: "ses_after_clear".into(),
                model: Some("fixture/retired".into()),
                effort: None,
            }
        );
        assert_eq!(
            [
                stub.seen.recv_timeout(Duration::from_secs(1)).unwrap().0,
                stub.seen.recv_timeout(Duration::from_secs(1)).unwrap().0,
            ],
            ["/config", "/event?directory=%2Ffixture%2Fproject"]
        );
        drop(observer);
    }

    #[test]
    fn model_and_effort_change_is_read_back_from_durable_session_history() {
        let stub = Stub::start(SESSIONS, STATUSES, EVENTS);
        let route = boop_store::bus::route_from_value(&serde_json::json!({
            "harness":"opencode", "session_id":"ses_new", "kind":"coordinator",
            "appServerSocket":stub.base.as_str(), "mode":"native-owned"
        }));
        assert_eq!(
            stub.door()
                .change_native_settings(&route, "fixture/model", "high")
                .unwrap(),
            NativeTuiEvent::Settings {
                session_id: "ses_new".into(),
                model: Some("fixture/model".into()),
                effort: Some("high".into()),
            }
        );
        let first = stub.seen.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(first.0, "/api/session/ses_new/model");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&first.1).unwrap(),
            serde_json::json!({"id":"model","providerID":"fixture","variant":"high"})
        );
        assert_eq!(
            stub.seen.recv_timeout(Duration::from_secs(1)).unwrap().0,
            "/api/session/ses_new/history"
        );
    }

    #[test]
    fn route_uses_its_observed_http_server() {
        let stub = Stub::start(SESSIONS, STATUSES, EVENTS);
        let route = boop_store::bus::route_from_value(&serde_json::json!({
            "harness":"opencode", "session_id":"ses_new", "kind":"coordinator",
            "appServerSocket":stub.base.as_str(), "mode":"native-owned"
        }));
        let unrelated = OpencodeDoor::at(Url::parse("http://127.0.0.1:1/").unwrap());
        let live = unrelated
            .live_session_for_route(&route)
            .unwrap()
            .expect("route-scoped live session");
        assert_eq!(
            live.door,
            DoorAddress::Http {
                base: stub.base.clone(),
                session: "ses_new".into()
            }
        );
        assert_eq!(
            unrelated
                .notify_idle(&live, Duration::from_secs(2))
                .unwrap()
                .status_line
                .as_deref(),
            Some("session.idle")
        );
    }

    #[test]
    fn first_prompt_preserves_configured_model_without_provider_fallback() {
        let stub = Stub::start(SESSIONS, STATUSES, EVENTS);
        assert_eq!(
            stub.door().default_model(),
            Some(serde_json::json!({"providerID":"fixture","modelID":"retired"}))
        );
        assert_eq!(
            stub.seen.recv_timeout(Duration::from_secs(1)).unwrap().0,
            "/config"
        );
        assert!(stub.seen.try_recv().is_err());
    }

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
    fn a_busy_delivery_stays_pending_without_replacing_the_active_prompt() {
        let stub = Stub::start(SESSIONS, STATUSES, EVENTS);
        let door = stub.door();
        let session = door
            .live_sessions()
            .unwrap()
            .into_iter()
            .find(|session| session.session_id == "ses_new")
            .unwrap();
        assert!(matches!(
            door.deliver(&session, "pending peer message").unwrap(),
            Delivered::Unreachable(_)
        ));
        assert!(!std::iter::from_fn(|| stub.seen.try_recv().ok())
            .any(|(target, _)| target.ends_with("/prompt_async")));
    }

    #[test]
    fn a_delivery_posts_one_text_part() {
        let stub = Stub::start(SESSIONS, "{}", EVENTS);
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
