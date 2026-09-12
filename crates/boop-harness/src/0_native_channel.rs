//! A lane channel backed by the harness TUI in the lane's own pane.

use std::process::Stdio;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use boop_acp::channel::{Delivery, LaneChannel, TurnEvent};
use boop_store::bus::Route;

use crate::harness::{Harness, HarnessId, NativeTuiEvent, NativeTuiPlan, NativeTuiSpec};
use crate::live::LiveSession;

pub struct NativeLaneChannel {
    adapter_id: HarnessId,
    plan: NativeTuiPlan,
    session: Option<String>,
    route: Route,
    target: String,
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
            args.push("--no-alt-screen".into());
        }
        HarnessId::Opencode => {
            if let Some(session) = spec.resume.as_deref() {
                args.extend(["--session".into(), session.into()]);
            }
            if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
                args.extend(["--model".into(), model.into()]);
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
    plan.frontend = Some(
        std::process::Command::new(&plan.program)
            .args(&plan.args)
            .envs(native.env.iter().cloned())
            .current_dir(&native.cwd)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(boop_store::trail::child_stderr(Some(lane)))
            .spawn()
            .with_context(|| format!("start native {harness} lane TUI"))?,
    );
    let route = Route {
        kind: "lane".into(),
        harness: Some(harness),
        tmux: std::env::var("TMUX_PANE").ok(),
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
        target: lane.to_owned(),
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
        let Some(observer) = self.plan.observer.as_mut() else {
            return Ok(());
        };
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
                NativeTuiEvent::Settings { .. } | NativeTuiEvent::Closed { .. } => {}
            }
        }
        Ok(())
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

    fn submit_terminal(&self, text: &str) -> Result<()> {
        // Codex announces the fresh thread before its composer has accepted
        // terminal input. Give the native frontend a bounded startup window
        // before submitting the first materializing turn.
        thread::sleep(Duration::from_secs(1));
        let target = &self.target;
        let socket = std::env::var("BOOP_TMUX_SOCKET").ok();
        boop_store::tmux::mux().send_text(socket.as_deref(), &target, text)?;
        boop_store::tmux::mux().send_key_named(socket.as_deref(), &target, "Enter")?;
        Ok(())
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
        if self.session.is_some() {
            let session = self.live_session()?;
            match self.adapter().door().deliver(&session, text) {
                Ok(crate::Delivered::Injected | crate::Delivered::QueuedForTurnBoundary) => {
                    return Ok(())
                }
                Ok(crate::Delivered::Unreachable(detail)) => {
                    if !detail.contains("not materialized") {
                        anyhow::bail!("native TUI delivery failed: {detail}");
                    }
                }
                Err(error) if error.to_string().contains("not materialized") => {}
                Err(error) => return Err(error),
            }
        }
        self.submit_terminal(text)
    }

    fn steer(&mut self, text: &str) -> Result<Delivery> {
        self.start_turn(text)?;
        Ok(Delivery::NextTurn)
    }

    fn next_event(&mut self, timeout: Duration) -> Result<Option<TurnEvent>> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            self.observe()?;
            if self.session.is_some() {
                let session = self.live_session()?;
                return match self.adapter().door().notify_idle(
                    &session,
                    deadline.saturating_duration_since(std::time::Instant::now()),
                ) {
                    Ok(_) => Ok(Some(TurnEvent::ok("native TUI idle"))),
                    Err(error)
                        if error.to_string().contains("stayed busy")
                            || error.to_string().contains("stayed active") =>
                    {
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
