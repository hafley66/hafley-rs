//! The Agent Client Protocol lane channel, on the `agent-client-protocol`
//! crate. `LaneChannel` is sync and the ACP connection is async and scoped to
//! `Builder::connect_with`, so the connection owns a thread and the two sides
//! trade `Command`/`Note`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    CancelNotification, InitializeRequest, LoadSessionRequest, NewSessionRequest,
    PermissionOptionKind, PromptRequest, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionConfigOptionValue,
    SessionNotification, SessionUpdate, SetSessionConfigOptionRequest, StopReason,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, ConnectionTo, LineDirection};
use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEvent};

/// The config option every ACP agent names its model with.
const MODEL_CONFIG_ID: &str = "model";

/// How long the opening handshake (spawn, `initialize`, session) may take.
const OPEN_TIMEOUT: Duration = Duration::from_secs(120);

/// What the sync side asks the connection thread to do.
#[derive(Debug)]
enum Command {
    Prompt(String),
    Cancel,
    Close,
}

/// What the connection thread reports back.
#[derive(Debug)]
enum Note {
    /// The session exists; the value is its ACP `sessionId`.
    Opened(String),
    /// The handshake never reached a session.
    OpenFailed(String),
    /// A turn reached a verdict.
    Turn(TurnEvent),
}

pub struct AcpChannel {
    commands: tokio::sync::mpsc::UnboundedSender<Command>,
    notes: Receiver<Note>,
    driver: Option<std::thread::JoinHandle<()>>,
    session: Option<String>,
    /// Epoch millis of the newest `session/update`; 0 before the first one.
    last_update_ms: Arc<AtomicU64>,
    turn_running: bool,
}

impl AcpChannel {
    /// Spawn `command` as an ACP agent and open one session, blocking until
    /// the session id is known or the handshake fails.
    pub fn open(spec: &ChannelSpec, command: &[String]) -> Result<AcpChannel> {
        let (program, args) = command
            .split_first()
            .context("an acp channel needs a command to spawn")?;
        // `session/new` takes an absolute cwd by spec; kimi rejects a relative
        // one outright, so canonicalize before the handshake rather than after.
        let cwd = std::fs::canonicalize(&spec.cwd)
            .with_context(|| format!("resolve acp session cwd {}", spec.cwd.display()))?;
        let lane = spec.lane.clone();
        let agent = AcpAgent::new(AcpAgentConfig::new(program).args(args.to_vec())).with_debug(
            move |line, direction| match direction {
                LineDirection::Stderr => {
                    debug!(lane = lane.as_deref().unwrap_or_default(), line, "acp agent stderr")
                }
                _ => debug!(?direction, line, "acp wire"),
            },
        );

        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (note_tx, note_rx) = std::sync::mpsc::channel();
        let last_update_ms = Arc::new(AtomicU64::new(0));
        let plan = SessionPlan {
            cwd,
            model: spec.model.clone().filter(|value| !value.is_empty()),
            resume: spec.resume.clone(),
            clock: Arc::clone(&last_update_ms),
        };
        info!(
            command = %command.join(" "),
            cwd = %plan.cwd.display(),
            model = plan.model.as_deref().unwrap_or_default(),
            "acp channel opening"
        );
        let driver = std::thread::Builder::new()
            .name("boop-acp".to_owned())
            .spawn(move || drive(agent, plan, command_rx, note_tx))
            .context("spawn the acp connection thread")?;

        let mut channel = AcpChannel {
            commands: command_tx,
            notes: note_rx,
            driver: Some(driver),
            session: None,
            last_update_ms,
            turn_running: false,
        };
        match channel.notes.recv_timeout(OPEN_TIMEOUT) {
            Ok(Note::Opened(session)) => {
                info!(
                    conversation_id = session,
                    conversation_id_kind = "acp_session",
                    "acp session opened"
                );
                channel.session = Some(session);
                Ok(channel)
            }
            Ok(Note::OpenFailed(detail)) => anyhow::bail!("acp handshake failed: {detail}"),
            Ok(Note::Turn(event)) => {
                anyhow::bail!("acp handshake yielded a turn verdict: {}", event.detail())
            }
            Err(RecvTimeoutError::Timeout) => {
                anyhow::bail!("acp handshake timed out after {OPEN_TIMEOUT:?}")
            }
            Err(RecvTimeoutError::Disconnected) => {
                anyhow::bail!("the acp connection thread died during the handshake")
            }
        }
    }
}

impl LaneChannel for AcpChannel {
    fn conversation_id(&self) -> Option<String> {
        self.session.clone()
    }

    fn conversation_id_kind(&self) -> &'static str {
        "acp_session"
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        if self.turn_running {
            anyhow::bail!("an acp turn is already running");
        }
        self.commands
            .send(Command::Prompt(text.to_owned()))
            .map_err(|_| anyhow::anyhow!("the acp connection is closed"))?;
        self.turn_running = true;
        info!(
            conversation_id = self.session.as_deref().unwrap_or_default(),
            text_bytes = text.len(),
            "acp prompt turn starting"
        );
        Ok(())
    }

    fn steer(&mut self, _text: &str) -> Result<Delivery> {
        // ACP has no mid-turn prompt: `session/prompt` is one request per turn
        // and a second one before the first resolves is out of protocol.
        Ok(Delivery::NextTurn)
    }

    fn next_event(&mut self, timeout: std::time::Duration) -> Result<Option<TurnEvent>> {
        if !self.turn_running {
            return Ok(Some(TurnEvent::failed("no acp turn to join")));
        }
        loop {
            match self.notes.recv_timeout(timeout) {
                Ok(Note::Turn(event)) => {
                    self.turn_running = false;
                    return Ok(Some(event));
                }
                Ok(Note::Opened(session)) => self.session = Some(session),
                Ok(Note::OpenFailed(detail)) => {
                    self.turn_running = false;
                    return Ok(Some(TurnEvent::flaked(detail)));
                }
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => {
                    self.turn_running = false;
                    return Ok(Some(TurnEvent::flaked("the acp connection thread exited")));
                }
            }
        }
    }

    fn interrupt(&mut self) -> Result<()> {
        if self.commands.send(Command::Cancel).is_err() {
            debug!("acp interrupt reached a closed connection");
        }
        Ok(())
    }

    fn last_activity_ms(&self) -> Option<u64> {
        match self.last_update_ms.load(Ordering::Relaxed) {
            0 => None,
            written => Some(written),
        }
    }

    fn close(&mut self) -> Result<()> {
        let _ = self.commands.send(Command::Close);
        if let Some(driver) = self.driver.take() {
            let _ = driver.join();
        }
        self.turn_running = false;
        Ok(())
    }
}

/// What the connection thread needs beyond the transport.
struct SessionPlan {
    cwd: PathBuf,
    model: Option<String>,
    resume: Option<String>,
    clock: Arc<AtomicU64>,
}

/// Own one ACP connection for the channel's life.
fn drive(
    agent: AcpAgent,
    plan: SessionPlan,
    commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
    notes: Sender<Note>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = notes.send(Note::OpenFailed(format!("build the acp runtime: {error}")));
            return;
        }
    };
    let failure = notes.clone();
    let result = runtime.block_on(connect(agent, plan, commands, notes));
    if let Err(error) = result {
        warn!(error = %error.message, code = ?error.code, "acp connection ended in error");
        let _ = failure.send(Note::OpenFailed(error.message));
    }
}

async fn connect(
    agent: AcpAgent,
    plan: SessionPlan,
    commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
    notes: Sender<Note>,
) -> Result<(), agent_client_protocol::Error> {
    let clock = Arc::clone(&plan.clock);
    let mut commands = commands;
    agent_client_protocol::Client
        .builder()
        .name("boop")
        .on_receive_notification(
            async move |notification: SessionNotification, _connection| {
                clock.store(crate::channel::now_ms(), Ordering::Relaxed);
                debug!(
                    conversation_id = %notification.session_id.0,
                    kind = update_kind(&notification.update),
                    "acp session update"
                );
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _connection| {
                // A lane runs unattended: an unanswered permission request
                // wedges the turn forever, which is the worse failure.
                match allow_option(&request) {
                    Some(option) => {
                        info!(tool_call = ?request.tool_call.tool_call_id, option = %option.0, "acp permission auto-allowed");
                        responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                option,
                            )),
                        ))
                    }
                    None => {
                        warn!(tool_call = ?request.tool_call.tool_call_id, "acp permission request carried no allow option");
                        responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Cancelled,
                        ))
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
            let session = handshake(&connection, &plan).await?;
            let _ = notes.send(Note::Opened(session.0.to_string()));
            while let Some(command) = commands.recv().await {
                match command {
                    Command::Prompt(text) => {
                        let outcome = connection
                            .send_request(PromptRequest::new(
                                session.clone(),
                                vec![text.into()],
                            ))
                            .block_task()
                            .await
                            .map(|response| response.stop_reason);
                        if notes.send(Note::Turn(turn_verdict(outcome))).is_err() {
                            break;
                        }
                    }
                    Command::Cancel => {
                        connection.send_notification(CancelNotification::new(session.clone()))?;
                    }
                    Command::Close => break,
                }
            }
            Ok(())
        })
        .await
}

/// `initialize`, session, then model. Under ACP opencode ignores
/// `opencode.json` and `OPENCODE_MODEL` and hangs on its dead default, so the
/// config-option call is the only model lever.
async fn handshake(
    connection: &ConnectionTo<Agent>,
    plan: &SessionPlan,
) -> Result<agent_client_protocol::schema::v1::SessionId, agent_client_protocol::Error> {
    let initialized = connection
        .send_request(InitializeRequest::new(ProtocolVersion::V1))
        .block_task()
        .await?;
    info!(
        agent = ?initialized.agent_info,
        protocol_version = ?initialized.protocol_version,
        load_session = initialized.agent_capabilities.load_session,
        "acp agent initialized"
    );

    let session = match plan.resume.as_deref() {
        Some(resume) if initialized.agent_capabilities.load_session => {
            let session = agent_client_protocol::schema::v1::SessionId::new(resume);
            connection
                .send_request(LoadSessionRequest::new(session.clone(), plan.cwd.clone()))
                .block_task()
                .await?;
            session
        }
        Some(resume) => {
            warn!(
                conversation_id = resume,
                "acp agent does not advertise loadSession; opening a new session"
            );
            connection
                .send_request(NewSessionRequest::new(plan.cwd.clone()))
                .block_task()
                .await?
                .session_id
        }
        None => {
            connection
                .send_request(NewSessionRequest::new(plan.cwd.clone()))
                .block_task()
                .await?
                .session_id
        }
    };

    if let Some(model) = plan.model.as_deref() {
        connection
            .send_request(SetSessionConfigOptionRequest::new(
                session.clone(),
                MODEL_CONFIG_ID,
                SessionConfigOptionValue::value_id(model.to_owned()),
            ))
            .block_task()
            .await?;
        info!(model, "acp session model set");
    }
    Ok(session)
}

/// The turn verdict for one `session/prompt` outcome. A JSON-RPC error is a
/// flake the agent never saw; a non-`end_turn` stop reason is its own answer.
fn turn_verdict(
    outcome: Result<StopReason, agent_client_protocol::Error>,
) -> TurnEvent {
    match outcome {
        Ok(StopReason::EndTurn) => TurnEvent::ok("end_turn"),
        Ok(other) => TurnEvent::failed(format!("stop_reason={}", stop_reason_name(other))),
        Err(error) => TurnEvent::flaked(error.message),
    }
}

/// The wire spelling of a stop reason; printed, never parsed.
fn stop_reason_name(reason: StopReason) -> &'static str {
    match reason {
        StopReason::EndTurn => "end_turn",
        StopReason::MaxTokens => "max_tokens",
        StopReason::MaxTurnRequests => "max_turn_requests",
        StopReason::Refusal => "refusal",
        StopReason::Cancelled => "cancelled",
        _ => "unknown",
    }
}

/// The first option the agent is willing to call an allow.
fn allow_option(
    request: &RequestPermissionRequest,
) -> Option<agent_client_protocol::schema::v1::PermissionOptionId> {
    request
        .options
        .iter()
        .find(|option| {
            matches!(
                option.kind,
                PermissionOptionKind::AllowOnce | PermissionOptionKind::AllowAlways
            )
        })
        .map(|option| option.option_id.clone())
}

/// The `sessionUpdate` tag of one update, for the trail.
fn update_kind(update: &SessionUpdate) -> &'static str {
    match update {
        SessionUpdate::UserMessageChunk(_) => "user_message_chunk",
        SessionUpdate::AgentMessageChunk(_) => "agent_message_chunk",
        SessionUpdate::AgentThoughtChunk(_) => "agent_thought_chunk",
        SessionUpdate::ToolCall(_) => "tool_call",
        SessionUpdate::ToolCallUpdate(_) => "tool_call_update",
        SessionUpdate::Plan(_) => "plan",
        SessionUpdate::AvailableCommandsUpdate(_) => "available_commands_update",
        SessionUpdate::CurrentModeUpdate(_) => "current_mode_update",
        SessionUpdate::ConfigOptionUpdate(_) => "config_option_update",
        SessionUpdate::SessionInfoUpdate(_) => "session_info_update",
        SessionUpdate::UsageUpdate(_) => "usage_update",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        PermissionOption, PermissionOptionId, ToolCallUpdate, ToolCallUpdateFields,
    };

    /// The `error` member of a JSON-RPC error response, as written on the wire.
    fn prompt_error_frame(message: &str) -> agent_client_protocol::Error {
        let frame: serde_json::Value = serde_json::from_str(&format!(
            r#"{{"jsonrpc":"2.0","id":3,"error":{{"code":-32603,"message":{message:?}}}}}"#
        ))
        .unwrap();
        serde_json::from_value(frame["error"].clone()).unwrap()
    }

    // FAIL-PRE-FIX: a dropped provider stream arrives as a JSON-RPC error on
    // `session/prompt`; anything but `Flaked` costs the supervisor its retry.
    #[test]
    fn a_prompt_error_frame_is_a_retryable_flake() {
        let verdict = turn_verdict(Err(prompt_error_frame(
            "AI_APICallError: Upstream request failed: Endpoint is unavailable.",
        )));
        assert!(verdict.retryable(), "{verdict:?}");
        assert_eq!(
            verdict.detail(),
            "AI_APICallError: Upstream request failed: Endpoint is unavailable."
        );
    }

    #[test]
    fn end_turn_is_the_only_clean_verdict() {
        assert!(turn_verdict(Ok(StopReason::EndTurn)).is_done());
        assert_eq!(turn_verdict(Ok(StopReason::EndTurn)).detail(), "end_turn");
    }

    #[test]
    fn every_other_stop_reason_is_a_terminal_failure() {
        for reason in [
            StopReason::Cancelled,
            StopReason::Refusal,
            StopReason::MaxTokens,
            StopReason::MaxTurnRequests,
        ] {
            let verdict = turn_verdict(Ok(reason));
            assert!(!verdict.is_done(), "{verdict:?}");
            assert!(!verdict.retryable(), "{verdict:?}");
            assert_eq!(
                verdict.detail(),
                format!("stop_reason={}", stop_reason_name(reason))
            );
        }
    }

    #[test]
    fn the_first_allow_option_wins_over_a_leading_reject() {
        let request = RequestPermissionRequest::new(
            agent_client_protocol::schema::v1::SessionId::new("ses_1"),
            ToolCallUpdate::new(
                agent_client_protocol::schema::v1::ToolCallId::new("call_1"),
                ToolCallUpdateFields::default(),
            ),
            vec![
                PermissionOption::new(
                    PermissionOptionId::new("reject"),
                    "Reject",
                    PermissionOptionKind::RejectOnce,
                ),
                PermissionOption::new(
                    PermissionOptionId::new("allow"),
                    "Allow",
                    PermissionOptionKind::AllowOnce,
                ),
            ],
        );
        assert_eq!(allow_option(&request).unwrap().0.as_ref(), "allow");
    }

    #[test]
    fn a_reject_only_permission_request_selects_nothing() {
        let request = RequestPermissionRequest::new(
            agent_client_protocol::schema::v1::SessionId::new("ses_1"),
            ToolCallUpdate::new(
                agent_client_protocol::schema::v1::ToolCallId::new("call_1"),
                ToolCallUpdateFields::default(),
            ),
            vec![PermissionOption::new(
                PermissionOptionId::new("reject"),
                "Reject",
                PermissionOptionKind::RejectAlways,
            )],
        );
        assert!(allow_option(&request).is_none());
    }

    /// The live leg. Needs `opencode` on PATH and a working provider, so it
    /// stays off the default run: `cargo test -p boop --lib channel::acp --
    /// --ignored --nocapture`.
    #[test]
    #[ignore]
    fn a_real_opencode_acp_turn_ends_the_turn() {
        let spec = ChannelSpec {
            model: Some("openrouter/deepseek/deepseek-v4-flash-0731".to_owned()),
            cwd: std::env::temp_dir(),
            resume: None,
            lane: None,
        };
        let opened = std::time::Instant::now();
        let mut channel =
            AcpChannel::open(&spec, &["opencode".to_owned(), "acp".to_owned()]).unwrap();
        println!(
            "session {:?} in {:?}",
            channel.conversation_id(),
            opened.elapsed()
        );
        assert_eq!(channel.conversation_id_kind(), "acp_session");

        let started = std::time::Instant::now();
        channel
            .start_turn("reply with the single word pong")
            .unwrap();
        let deadline = started + Duration::from_secs(30);
        let verdict = loop {
            if let Some(event) = channel
                .next_event(Duration::from_millis(200))
                .unwrap()
            {
                break event;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no turn verdict within 30s"
            );
        };
        println!("verdict {verdict:?} in {:?}", started.elapsed());
        println!("last_activity_ms {:?}", channel.last_activity_ms());
        channel.close().unwrap();
        assert!(verdict.is_done(), "{verdict:?}");
        assert_eq!(verdict.detail(), "end_turn");
    }

    #[test]
    fn an_empty_command_is_refused_before_a_thread_is_spawned() {
        let spec = ChannelSpec {
            model: None,
            cwd: std::env::temp_dir(),
            resume: None,
            lane: None,
        };
        let error = match AcpChannel::open(&spec, &[]) {
            Ok(_) => panic!("an empty command opened a channel"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("needs a command"), "{error}");
    }
}
