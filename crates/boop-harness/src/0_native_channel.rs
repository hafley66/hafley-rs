//! A lane channel backed by the harness TUI in the lane's own pane.

use std::process::Stdio;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use boop_acp::channel::{Delivery, LaneChannel, TurnEvent, TurnReceipt};
use boop_store::bus::Route;

use crate::harness::{Harness, HarnessId, NativeTuiEvent, NativeTuiPlan, NativeTuiSpec};
use crate::live::LiveSession;

pub struct NativeLaneChannel {
    adapter_id: HarnessId,
    plan: NativeTuiPlan,
    session: Option<String>,
    route: Route,
    target: String,
    baseline_seq: Option<u64>,
    pending: bool,
    pending_event: Option<TurnEvent>,
}

pub fn open(
    adapter: &dyn Harness,
    spec: &boop_acp::channel::ChannelSpec,
) -> Result<Box<dyn LaneChannel>> {
    let lane = spec
        .lane
        .as_deref()
        .context("native lane channel needs a lane")?;
    let harness = adapter.id();
    let executable = spec
        .executable
        .clone()
        .unwrap_or_else(|| harness.as_str().to_owned());
    let mut args = Vec::new();
    match harness {
        HarnessId::Claude => {
            if let Some(session) = spec.resume.as_deref() {
                args.extend(["--resume".into(), session.into()]);
            }
            if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
                args.extend(["--model".into(), model.into()]);
            }
        }
        HarnessId::Codex => {
            if let Some(session) = spec.resume.as_deref() {
                args.extend(["resume".into(), session.into()]);
            }
            if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
                args.extend(["--model".into(), model.into()]);
            }
            if let Some(effort) = spec.effort.as_deref().filter(|value| !value.is_empty()) {
                args.extend([
                    "-c".into(),
                    format!("model_reasoning_effort={effort}").into(),
                ]);
            }
            args.push("--no-alt-screen".into());
        }
        HarnessId::Opencode => {
            if let Some(session) = spec.resume.as_deref() {
                args.extend(["--session".into(), session.into()]);
            }
            if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
                args.extend(["--model".into(), model.into()]);
            }
            if let Some(effort) = spec.effort.as_deref().filter(|value| !value.is_empty()) {
                args.extend(["--variant".into(), effort.into()]);
            }
        }
        HarnessId::Kimi => anyhow::bail!("kimi lanes retain their ACP channel"),
    }
    let dir = std::env::var_os("BOOP_MAIL_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| spec.cwd.clone());
    let db = std::env::var_os("BOOP_DB")
        .map(std::path::PathBuf::from)
        .unwrap_or(boop_store::bus::db_path(&dir)?);
    let native = NativeTuiSpec {
        executable,
        cwd: spec.cwd.clone(),
        args,
        env: vec![
            ("BOOP_SESSION".into(), lane.into()),
            ("BOOP_LANE".into(), lane.into()),
            ("BOOP_HARNESS".into(), harness.as_str().into()),
            ("BOOP_MAIL_DIR".into(), dir.display().to_string()),
            ("BOOP_DB".into(), db.display().to_string()),
        ],
    };
    let mut plan = adapter.door().tui_launch(&native)?;
    if harness == HarnessId::Claude {
        // An interactive claude reads trust per canonical directory. A lane
        // worktree is new on every spawn, so the workspace-trust dialog would
        // otherwise park the pane before the composer exists. The ACP channel
        // skipped it through print mode; the TUI needs the persisted grant.
        trust_claude_workspace(&native.cwd);
    }
    if harness == HarnessId::Codex {
        // Same shape as claude: codex compares the cwd spelling literally and
        // a fresh worktree misses the recipe's trust table, so the composer
        // readiness probe would match an overlaid trust dialog instead.
        trust_codex_workspace(&native.cwd);
    }
    plan.frontend = Some(
        std::process::Command::new(&plan.program)
            .args(&plan.args)
            .envs(native.env.iter().cloned())
            .current_dir(&native.cwd)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            // The native frontend owns the pane's rendering fd. Supervisor
            // diagnostics are emitted by its own file logger.
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("start native {harness} lane TUI"))?,
    );
    let route = Route {
        kind: "lane".into(),
        harness: Some(harness),
        tmux: std::env::var("BOOP_TMUX_TARGET")
            .ok()
            .or_else(|| std::env::var("TMUX_PANE").ok()),
        cwd: Some(native.cwd.display().to_string()),
        model: spec.model.clone(),
        mode: Some("native-lane".into()),
        session_id: plan.session_id.clone(),
        source_path: plan.source_path.clone(),
        parent: None,
        goal: None,
        registered_at: None,
        base_sha: None,
        worktree_dir: None,
        app_server_socket: plan.app_server_socket.clone(),
    };
    Ok(Box::new(NativeLaneChannel {
        adapter_id: harness,
        session: plan.session_id.clone(),
        plan,
        route,
        target: std::env::var("BOOP_TMUX_TARGET").unwrap_or_else(|_| lane.to_owned()),
        baseline_seq: None,
        pending: false,
        pending_event: None,
    }))
}

impl NativeLaneChannel {
    fn adapter(&self) -> Box<dyn Harness> {
        match self.adapter_id {
            HarnessId::Claude => Box::new(crate::harness::claude::Claude),
            HarnessId::Codex => Box::new(crate::harness::codex::Codex),
            HarnessId::Opencode => Box::new(crate::harness::opencode::Opencode),
            HarnessId::Kimi => Box::new(crate::harness::kimi::Kimi),
        }
    }

    fn observe(&mut self) -> Result<()> {
        if let Some(observer) = self.plan.observer.as_mut() {
            for event in observer.events.try_iter() {
                match event {
                    NativeTuiEvent::Session { session_id, .. } => self.session = Some(session_id),
                    NativeTuiEvent::Closed { session_id }
                        if self.session.as_deref() == Some(&session_id) =>
                    {
                        self.session = None;
                    }
                    NativeTuiEvent::Failed(error) => {
                        anyhow::bail!("native TUI observation failed: {error}")
                    }
                    NativeTuiEvent::Receipt {
                        session_id,
                        turn_id,
                        status,
                        text,
                        tool_calls,
                    } if self.pending && self.session.as_deref() == Some(&session_id) => {
                        self.pending = false;
                        self.pending_event = Some(match status.as_str() {
                            "completed" if !text.trim().is_empty() => TurnEvent::ok_with_receipt(
                                format!("native TUI turn {turn_id} completed"),
                                TurnReceipt { text, tool_calls },
                            ),
                            "completed" => TurnEvent::failed(format!(
                                "native TUI turn {turn_id} completed without assistant receipt"
                            )),
                            "interrupted" => {
                                TurnEvent::flaked(format!("native TUI turn {turn_id} interrupted"))
                            }
                            _ => TurnEvent::failed(format!(
                                "native TUI turn {turn_id} ended with status {status}"
                            )),
                        });
                    }
                    NativeTuiEvent::Settings { .. }
                    | NativeTuiEvent::Closed { .. }
                    | NativeTuiEvent::Receipt { .. } => {}
                }
            }
        }
        if self.session.is_none() {
            let pid = self.plan.frontend.as_ref().map(std::process::Child::id);
            self.session = self
                .adapter()
                .live()
                .live_sessions()?
                .into_iter()
                .find(|session| pid.is_some_and(|pid| session.pid == Some(pid)))
                .map(|session| session.session_id);
        }
        Ok(())
    }

    fn receipt(&self) -> Option<TurnReceipt> {
        let id = self.session.as_deref()?;
        let adapter = self.adapter();
        let session = adapter.session_by_id(id, self.route.cwd.as_deref())?;
        let messages = adapter.messages(&session, self.baseline_seq);
        let text = messages
            .iter()
            .filter(|m| m.role == "assistant")
            .map(|m| m.text.as_str())
            .collect::<String>();
        (!text.is_empty()).then_some(TurnReceipt {
            text,
            tool_calls: messages.iter().filter(|m| m.role == "tool").count() as u32,
        })
    }

    fn live_session(&self) -> Result<LiveSession> {
        let id = self
            .session
            .as_deref()
            .context("native TUI session is not bound")?;
        let mut route = self.route.clone();
        route.session_id = Some(id.to_owned());
        self.adapter()
            .live()
            .live_session_for_route(&route)?
            .with_context(|| format!("native TUI session {id} is not live"))
    }

    fn submit_terminal(&mut self, text: &str) -> Result<()> {
        let target = &self.target;
        let socket = std::env::var("BOOP_TMUX_SOCKET").ok();
        let input_target = boop_store::tmux::mux()
            .pane_id(socket.as_deref(), target)
            .unwrap_or_else(|| target.clone());
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut stable_screen = None;
        let mut stable_samples = 0;
        loop {
            let screen = boop_store::tmux::mux()
                .capture_pane(socket.as_deref(), target, Some(80))
                .ok();
            let ready = screen.as_deref().is_some_and(|screen| {
                let rows = screen.lines().collect::<Vec<_>>();
                self.adapter().terminal_input_region(&rows).is_some()
                    || (self.adapter_id == HarnessId::Codex
                        && screen.contains("Ask Codex to do anything"))
            });
            if ready && stable_screen.as_deref() == screen.as_deref() {
                stable_samples += 1;
            } else if ready {
                stable_screen = screen;
                stable_samples = 1;
            } else {
                stable_screen = None;
                stable_samples = 0;
            }
            if stable_samples >= 3 {
                break;
            }
            if std::time::Instant::now() >= deadline {
                let mux = boop_store::tmux::mux();
                let pane = mux.pane_id(socket.as_deref(), target);
                let screen = mux.capture_pane(socket.as_deref(), target, Some(80)).ok();
                let artifact = serde_json::json!({
                    "socket": socket,
                    "target": target,
                    "pane_id": pane,
                    "input_target": input_target,
                    "screen": screen,
                    "screen_snapshot": screen.clone(),
                    "input_region": false,
                    "frontend_pid": self.plan.frontend.as_ref().map(std::process::Child::id),
                    "frontend_status": self.plan.frontend.as_mut().and_then(|child| {
                        child.try_wait().ok().flatten().map(|status| status.to_string())
                    }),
                    "argv": std::iter::once(self.plan.program.clone())
                        .chain(self.plan.args.iter().map(|arg| arg.to_string_lossy().into_owned()))
                        .collect::<Vec<_>>(),
                });
                let path = std::env::var_os("BOOP_MAIL_DIR")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("lanes")
                    .join(std::env::var("BOOP_LANE").unwrap_or_else(|_| "native".into()))
                    .join("native-readiness-failure.json");
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(
                    &path,
                    serde_json::to_vec_pretty(&artifact).unwrap_or_default(),
                );
                anyhow::bail!(
                    "native TUI composer readiness timed out for target {target}; artifact {}",
                    path.display()
                );
            }
            thread::sleep(Duration::from_millis(100));
        }
        boop_store::tmux::mux().send_text(socket.as_deref(), &input_target, text)?;
        let after_text = serde_json::json!({
            "socket": socket,
            "target": target,
            "pane_id": boop_store::tmux::mux().pane_id(socket.as_deref(), target),
            "input_target": input_target,
            "screen": boop_store::tmux::mux().capture_pane(socket.as_deref(), target, Some(80)).ok(),
            "stage": "after_text_before_enter",
            "frontend_pid": self.plan.frontend.as_ref().map(std::process::Child::id),
            "frontend_status": self.plan.frontend.as_mut().and_then(|child| {
                child.try_wait().ok().flatten().map(|status| status.to_string())
            }),
        });
        let path = std::env::var_os("BOOP_MAIL_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("lanes")
            .join(std::env::var("BOOP_LANE").unwrap_or_else(|_| "native".into()))
            .join("native-input-evidence.json");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            &path,
            serde_json::to_vec_pretty(&after_text).unwrap_or_default(),
        );
        thread::sleep(Duration::from_millis(250));
        boop_store::tmux::mux().send_key_named(socket.as_deref(), &input_target, "Enter")?;
        let after_enter = serde_json::json!({
            "socket": socket,
            "target": target,
            "pane_id": boop_store::tmux::mux().pane_id(socket.as_deref(), target),
            "input_target": input_target,
            "screen": boop_store::tmux::mux().capture_pane(socket.as_deref(), target, Some(80)).ok(),
            "stage": "after_enter",
            "frontend_pid": self.plan.frontend.as_ref().map(std::process::Child::id),
            "frontend_status": self.plan.frontend.as_mut().and_then(|child| {
                child.try_wait().ok().flatten().map(|status| status.to_string())
            }),
        });
        let _ = std::fs::write(
            &path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "after_text": after_text,
                "after_enter": after_enter,
            }))
            .unwrap_or_default(),
        );
        Ok(())
    }

    fn capture_baseline(&mut self) {
        self.baseline_seq = self.session.as_deref().and_then(|id| {
            self.adapter()
                .session_by_id(id, self.route.cwd.as_deref())
                .and_then(|session| {
                    self.adapter()
                        .messages(&session, None)
                        .last()
                        .map(|m| m.seq)
                })
        });
    }

    fn submit_turn(&mut self, text: &str) -> Result<()> {
        if self.session.is_some() {
            // A turn boundary can be seen here before the harness's own status
            // flips to idle (OpenCode reports busy for a beat after the
            // transcript is complete), and a resumed frontend registers itself
            // live a moment after spawn. Wait both out, then deliver, instead
            // of failing the lane on a transient state.
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            loop {
                let session = match self.live_session() {
                    Ok(session) => session,
                    Err(_error) if std::time::Instant::now() < deadline => {
                        thread::sleep(Duration::from_millis(100));
                        self.observe()?;
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                match self.adapter().door().deliver(&session, text) {
                    Ok(crate::Delivered::Injected | crate::Delivered::QueuedForTurnBoundary) => {
                        return Ok(())
                    }
                    Ok(crate::Delivered::Unreachable(detail)) => {
                        if detail.contains("not materialized") {
                            break;
                        }
                        if detail.contains("busy") {
                            if std::time::Instant::now() >= deadline {
                                break;
                            }
                            let _ = self
                                .adapter()
                                .door()
                                .notify_idle(&session, Duration::from_millis(700));
                            continue;
                        }
                        anyhow::bail!("native TUI delivery failed: {detail}");
                    }
                    Err(error) if error.to_string().contains("not materialized") => break,
                    Err(error) => return Err(error),
                }
            }
        }
        self.submit_terminal(text)
    }
}

impl LaneChannel for NativeLaneChannel {
    fn conversation_id(&self) -> Option<String> {
        self.session.clone()
    }

    fn conversation_id_kind(&self) -> &'static str {
        "native_tui_session"
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        self.observe()?;
        let binding_deadline = std::time::Instant::now() + Duration::from_secs(10);
        while self.session.is_none() && std::time::Instant::now() < binding_deadline {
            thread::sleep(Duration::from_millis(100));
            self.observe()?;
        }
        self.capture_baseline();
        self.submit_turn(text)?;
        self.pending = true;
        Ok(())
    }

    fn steer(&mut self, text: &str) -> Result<Delivery> {
        let _ = text;
        Ok(Delivery::NextTurn)
    }

    fn next_event(&mut self, timeout: Duration) -> Result<Option<TurnEvent>> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            self.observe()?;
            if !self.pending {
                return Ok(self.pending_event.take());
            }
            if let Some(child) = self.plan.frontend.as_mut() {
                if let Some(status) = child.try_wait()? {
                    self.pending = false;
                    return Ok(Some(if status.success() {
                        TurnEvent::flaked("native TUI exited before turn receipt")
                    } else {
                        TurnEvent::failed(format!("native TUI exited with {status}"))
                    }));
                }
            }
            if self.session.is_some() {
                let session = self.live_session()?;
                return match self.adapter().door().notify_idle(
                    &session,
                    deadline.saturating_duration_since(std::time::Instant::now()),
                ) {
                    Ok(_) => {
                        self.observe()?;
                        match self.pending_event.take().or_else(|| {
                            self.receipt().map(|receipt| {
                                TurnEvent::ok_with_receipt("native TUI idle", receipt)
                            })
                        }) {
                            Some(event) => {
                                self.pending = false;
                                Ok(Some(event))
                            }
                            None => Ok(None),
                        }
                    }
                    Err(error)
                        if error.to_string().contains("stayed busy")
                            || error.to_string().contains("stayed active") =>
                    {
                        // OpenCode emits one complete assistant message per
                        // turn, so finished transcript text after the baseline
                        // is turn completion even when the idle event raced
                        // past this subscriber. Other adapters keep the door's
                        // explicit status and only poll again here.
                        if self.adapter_id == HarnessId::Opencode {
                            if let Some(receipt) = self.receipt() {
                                self.pending = false;
                                return Ok(Some(TurnEvent::ok_with_receipt(
                                    "native TUI turn completed",
                                    receipt,
                                )));
                            }
                        }
                        Ok(None)
                    }
                    Err(error) => Err(error),
                };
            }
            if std::time::Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn close(&mut self) -> Result<()> {
        self.plan.stop();
        Ok(())
    }
}

/// Mark `cwd` trusted in the claude config this lane will read. The config dir
/// is `$CLAUDE_CONFIG_DIR` when set, else `~/.claude`; the file is
/// `.claude.json` beside it. Other keys are preserved; a missing or unreadable
/// file becomes a fresh object.
fn trust_claude_workspace(cwd: &std::path::Path) {
    // With `CLAUDE_CONFIG_DIR` the state file is `$CLAUDE_CONFIG_DIR/.claude.json`;
    // without it claude keeps `~/.claude.json`.
    let path = match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => std::path::PathBuf::from(dir).join(".claude.json"),
        _ => match dirs::home_dir() {
            Some(home) => home.join(".claude.json"),
            None => return,
        },
    };
    let Some(key) = std::fs::canonicalize(cwd)
        .ok()
        .map(|path| path.display().to_string())
    else {
        return;
    };
    let mut root: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let Some(object) = root.as_object_mut() else {
        return;
    };
    let projects = object
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}));
    let Some(projects) = projects.as_object_mut() else {
        return;
    };
    projects.insert(key, serde_json::json!({ "hasTrustDialogAccepted": true }));
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&root) {
        let temp = path.with_extension("json.tmp");
        if std::fs::write(&temp, text).is_ok() {
            let _ = std::fs::rename(&temp, &path);
        }
    }
}

/// Append a trusted-project table for `cwd` to the codex config this lane will
/// read. The config dir is `$CODEX_HOME` when set, else `~/.codex`; the file is
/// `config.toml`. An existing table for the same canonical key is left alone.
fn trust_codex_workspace(cwd: &std::path::Path) {
    let config_dir = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")));
    let Some(config_dir) = config_dir else {
        return;
    };
    let Some(key) = std::fs::canonicalize(cwd)
        .ok()
        .map(|path| path.display().to_string())
    else {
        return;
    };
    let Ok(quoted) = serde_json::to_string(&key) else {
        return;
    };
    let path = config_dir.join("config.toml");
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    let header = format!("[projects.{quoted}]");
    if text.contains(&header) {
        return;
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("{header}\ntrust_level = \"trusted\"\n"));
    if std::fs::create_dir_all(&config_dir).is_ok() {
        let _ = std::fs::write(&path, text);
    }
}
