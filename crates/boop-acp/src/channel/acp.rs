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
    AgentCapabilities, CancelNotification, InitializeRequest, InitializeResponse,
    LoadSessionRequest, NewSessionRequest, PermissionOptionKind, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigKind, SessionConfigOption, SessionConfigOptionValue,
    SessionConfigSelectOptions, SessionId, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, StopReason,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, ConnectionTo, LineDirection};
use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEvent};

/// The config option every ACP agent names its model with.
const MODEL_CONFIG_ID: &str = "model";

/// The command that speaks ACP, one row per harness. Compiled in rather than
/// read from `config.json`: that file is parsed by boop-proc, which depends on
/// this crate, so a lookup here would invert the crate order. The npx rows
/// float on the npm dist-tag; the versions that answered `end_turn` on this
/// machine were `claude-agent-acp@0.70.0` and `codex-acp@1.4.0`
/// (`~/projects/labs/acp-lab/README.md`, 2026-08-19).
pub const CLAUDE_ADAPTER: &[&str] = &["npx", "-y", "@agentclientprotocol/claude-agent-acp"];
pub const CODEX_ADAPTER: &[&str] = &["npx", "-y", "@agentclientprotocol/codex-acp"];
pub const KIMI_ADAPTER: &[&str] = &["kimi", "acp"];
pub const OPENCODE_ADAPTER: &[&str] = &["opencode", "acp"];

/// How long the opening handshake (spawn, `initialize`, session) may take.
const OPEN_TIMEOUT: Duration = Duration::from_secs(120);

/// The `_meta` key an adapter advertises prompt queueing under. Vendor-scoped
/// by the protocol, so the vendor object is found by this leaf key rather than
/// by a compiled-in vendor name: `claudeCode` on claude-agent-acp 0.70.0 is one
/// spelling among the four adapters in the roster, and the other three
/// advertise nothing at all.
const PROMPT_QUEUEING_KEY: &str = "promptQueueing";

/// What an adapter puts in `data.code` when a turn is already running. Kimi
/// 0.37.2 is the one adapter on this machine that answers this way, with
/// JSON-RPC code -32600.
const BUSY_DATA_CODE: &str = "turn.agent_busy";

/// Whether this adapter takes a second `session/prompt` before the first
/// resolves. Read off `initialize`, demoted on a typed error, never assumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptQueueing {
    /// `agentCapabilities._meta.<vendor>.promptQueueing == true`.
    Advertised,
    /// A prompt came back with the typed busy code.
    Rejects,
    /// Nothing advertised and nothing refused yet.
    Unknown,
}

impl PromptQueueing {
    /// The word logged and printed; never parsed.
    pub fn as_str(self) -> &'static str {
        match self {
            PromptQueueing::Advertised => "advertised",
            PromptQueueing::Rejects => "rejects",
            PromptQueueing::Unknown => "unknown",
        }
    }

    /// Read the capability off one `initialize` reply.
    pub fn read(capabilities: &AgentCapabilities) -> PromptQueueing {
        let Some(meta) = capabilities.meta.as_ref() else {
            return PromptQueueing::Unknown;
        };
        let Ok(value) = serde_json::to_value(meta) else {
            return PromptQueueing::Unknown;
        };
        match advertises_queueing(&value) {
            true => PromptQueueing::Advertised,
            false => PromptQueueing::Unknown,
        }
    }
}

/// Whether any vendor object under `_meta` carries `promptQueueing: true`.
fn advertises_queueing(meta: &serde_json::Value) -> bool {
    let Some(object) = meta.as_object() else {
        return false;
    };
    object.values().any(|vendor| {
        vendor
            .get(PROMPT_QUEUEING_KEY)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    })
}

/// Whether a JSON-RPC error is the adapter saying a turn is already running.
/// The typed `data.code` is the machine-readable half; the message text is
/// per-adapter prose and is never matched on.
pub fn busy(error: &agent_client_protocol::Error) -> bool {
    error
        .data
        .as_ref()
        .and_then(|data| data.get("code"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|code| code == BUSY_DATA_CODE)
}

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
    /// The session exists, with its ACP `sessionId` and the queueing
    /// capability its `initialize` reply advertised.
    Opened {
        session: String,
        queueing: PromptQueueing,
    },
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
    queueing: PromptQueueing,
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
            queueing: PromptQueueing::Unknown,
        };
        match channel.notes.recv_timeout(OPEN_TIMEOUT) {
            Ok(Note::Opened { session, queueing }) => {
                info!(
                    conversation_id = session,
                    conversation_id_kind = "acp_session",
                    prompt_queueing = queueing.as_str(),
                    "acp session opened"
                );
                channel.session = Some(session);
                channel.queueing = queueing;
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

    /// Open one of the roster adapters. The roster rows are `&[&str]`, so a
    /// caller names a const instead of building a `Vec<String>`.
    pub fn open_adapter(spec: &ChannelSpec, adapter: &[&str]) -> Result<AcpChannel> {
        let command: Vec<String> = adapter.iter().map(|part| (*part).to_owned()).collect();
        AcpChannel::open(spec, &command)
    }

    /// What this adapter's `initialize` said about a second `session/prompt`
    /// during a running turn.
    pub fn queueing(&self) -> PromptQueueing {
        self.queueing
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
        // Three of the four roster adapters accept a second `session/prompt`
        // during a running turn and kimi 0.37.2 answers `turn.agent_busy`, so
        // the capability is read and carried. Which policy the delivery timing
        // follows is the user's call; until it is made, every offer waits for
        // the turn boundary, which is what every lane has always done.
        debug!(
            prompt_queueing = self.queueing.as_str(),
            "acp steer held for the turn boundary"
        );
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
                Ok(Note::Opened { session, queueing }) => {
                    self.session = Some(session);
                    self.queueing = queueing;
                }
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
pub(crate) struct SessionPlan {
    pub(crate) cwd: PathBuf,
    pub(crate) model: Option<String>,
    pub(crate) resume: Option<String>,
    pub(crate) clock: Arc<AtomicU64>,
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
            let opened = handshake(&connection, &plan).await?;
            let session = opened.session;
            let _ = notes.send(Note::Opened {
                session: session.0.to_string(),
                queueing: PromptQueueing::read(&opened.initialized.agent_capabilities),
            });
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
pub(crate) struct Handshake {
    pub(crate) session: SessionId,
    /// Cached so a proxy answers a downstream `initialize` with the upstream's
    /// own capabilities instead of inventing a second set.
    pub(crate) initialized: InitializeResponse,
}

pub(crate) async fn handshake(
    connection: &ConnectionTo<Agent>,
    plan: &SessionPlan,
) -> Result<Handshake, agent_client_protocol::Error> {
    let initialized = connection
        .send_request(InitializeRequest::new(ProtocolVersion::V1))
        .block_task()
        .await?;
    info!(
        agent = ?initialized.agent_info,
        protocol_version = ?initialized.protocol_version,
        load_session = initialized.agent_capabilities.load_session,
        prompt_queueing = PromptQueueing::read(&initialized.agent_capabilities).as_str(),
        "acp agent initialized"
    );

    let (session, config_options) = match plan.resume.as_deref() {
        Some(resume) if initialized.agent_capabilities.load_session => {
            let session = SessionId::new(resume);
            let loaded = connection
                .send_request(LoadSessionRequest::new(session.clone(), plan.cwd.clone()))
                .block_task()
                .await?;
            (session, loaded.config_options)
        }
        Some(resume) => {
            warn!(
                conversation_id = resume,
                "acp agent does not advertise loadSession; opening a new session"
            );
            let opened = connection
                .send_request(NewSessionRequest::new(plan.cwd.clone()))
                .block_task()
                .await?;
            (opened.session_id, opened.config_options)
        }
        None => {
            let opened = connection
                .send_request(NewSessionRequest::new(plan.cwd.clone()))
                .block_task()
                .await?;
            (opened.session_id, opened.config_options)
        }
    };

    if let Some(model) = plan.model.as_deref() {
        select_model(
            connection,
            &session,
            model,
            config_options.as_deref().unwrap_or_default(),
        )
        .await?;
    }
    Ok(Handshake {
        session,
        initialized,
    })
}

/// Name the model. The spelling is the harness's own and is sent through
/// untouched: an id the agent rejects is a loud open failure, never a silent
/// fall back to its default model.
async fn select_model(
    connection: &ConnectionTo<Agent>,
    session: &SessionId,
    model: &str,
    config_options: &[SessionConfigOption],
) -> Result<(), agent_client_protocol::Error> {
    match connection
        .send_request(SetSessionConfigOptionRequest::new(
            session.clone(),
            MODEL_CONFIG_ID,
            SessionConfigOptionValue::value_id(model.to_owned()),
        ))
        .block_task()
        .await
    {
        Ok(_) => {
            info!(model, "acp session model set");
            Ok(())
        }
        Err(error) => Err(agent_client_protocol::Error::new(
            i32::from(error.code),
            model_rejection(&error.message, model, config_options),
        )),
    }
}

/// A rejected model id reaches the wire as a bare "Invalid params". The agent
/// listed what it does take on its session reply, so the open failure carries
/// those ids instead of leaving the reader to probe for them.
fn model_rejection(message: &str, model: &str, config_options: &[SessionConfigOption]) -> String {
    match offered_models(config_options) {
        Some(offered) => format!("{message}: model `{model}` (this agent takes: {offered})"),
        None => format!("{message}: model `{model}`"),
    }
}

/// The value ids of the agent's `model` config option, comma joined.
fn offered_models(config_options: &[SessionConfigOption]) -> Option<String> {
    let select = config_options
        .iter()
        .find(|option| option.id.0.as_ref() == MODEL_CONFIG_ID)
        .and_then(|option| match &option.kind {
            SessionConfigKind::Select(select) => Some(select),
            _ => None,
        })?;
    let values: Vec<&str> = match &select.options {
        SessionConfigSelectOptions::Ungrouped(options) => {
            options.iter().map(|option| option.value.0.as_ref()).collect()
        }
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .map(|option| option.value.0.as_ref())
            .collect(),
        _ => return None,
    };
    Some(values.join(", "))
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
        PermissionOption, PermissionOptionId, SessionConfigId, SessionConfigSelectOption,
        SessionConfigValueId, ToolCallUpdate, ToolCallUpdateFields,
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

    /// RECEIPT. codex-acp answers a model id it does not offer with a bare
    /// "Invalid params"; the ids it does offer are on its session reply, so
    /// the open failure names them. Measured 2026-08-20, codex-acp 1.6.2:
    /// its `model` option takes the family alone and carries the effort on a
    /// separate `reasoning_effort` option, so boop's `gpt-5.6-luna@medium`
    /// spelling reaches no option value.
    #[test]
    fn a_rejected_model_id_names_what_the_agent_does_take() {
        let options = vec![
            SessionConfigOption::select(
                SessionConfigId::new("mode"),
                "Mode",
                SessionConfigValueId::new("agent"),
                vec![SessionConfigSelectOption::new(
                    SessionConfigValueId::new("agent"),
                    "Agent",
                )],
            ),
            SessionConfigOption::select(
                SessionConfigId::new(MODEL_CONFIG_ID),
                "Model",
                SessionConfigValueId::new("gpt-5.6-sol"),
                vec![
                    SessionConfigSelectOption::new(
                        SessionConfigValueId::new("gpt-5.6-sol"),
                        "GPT-5.6-Sol",
                    ),
                    SessionConfigSelectOption::new(
                        SessionConfigValueId::new("gpt-5.6-luna"),
                        "GPT-5.6-Luna",
                    ),
                ],
            ),
        ];
        let message = model_rejection("Invalid params", "gpt-5.6-luna@medium", &options);
        assert_eq!(
            message,
            "Invalid params: model `gpt-5.6-luna@medium` (this agent takes: gpt-5.6-sol, gpt-5.6-luna)"
        );
    }

    /// An agent that lists no model option leaves the message unadorned.
    #[test]
    fn a_rejection_without_a_model_option_says_only_the_id() {
        assert_eq!(
            model_rejection("Invalid params", "whatever", &[]),
            "Invalid params: model `whatever`"
        );
    }

    /// A channel with no connection behind it, for the sync-side mapping.
    fn idle_channel() -> AcpChannel {
        let (commands, _commands_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_notes_tx, notes) = std::sync::mpsc::channel();
        AcpChannel {
            commands,
            notes,
            driver: None,
            session: Some("ses_1".to_owned()),
            last_update_ms: Arc::new(AtomicU64::new(0)),
            turn_running: false,
            queueing: PromptQueueing::Unknown,
        }
    }

    fn capabilities(meta: serde_json::Value) -> AgentCapabilities {
        let mut capabilities = AgentCapabilities::new();
        capabilities.meta = serde_json::from_value(meta).unwrap();
        capabilities
    }

    /// RECEIPT: claude-agent-acp 0.70.0 advertises exactly this shape
    /// (PLAN section 13, probe 1).
    #[test]
    fn a_vendor_meta_flag_reads_as_advertised() {
        let capabilities = capabilities(serde_json::json!({
            "claudeCode": { "promptQueueing": true }
        }));
        assert_eq!(
            PromptQueueing::read(&capabilities),
            PromptQueueing::Advertised
        );
    }

    /// The vendor key is the adapter's own, so a second spelling reads the
    /// same rather than needing a roster row.
    #[test]
    fn any_vendor_key_carries_the_flag() {
        let capabilities = capabilities(serde_json::json!({
            "someOtherVendor": { "promptQueueing": true }
        }));
        assert_eq!(
            PromptQueueing::read(&capabilities),
            PromptQueueing::Advertised
        );
    }

    /// codex 1.6.2, kimi 0.37.2 and opencode 1.18.18 advertise nothing, and
    /// silence is never read as a yes.
    #[test]
    fn silence_is_unknown_and_never_advertised() {
        assert_eq!(
            PromptQueueing::read(&AgentCapabilities::new()),
            PromptQueueing::Unknown
        );
        let capabilities = capabilities(serde_json::json!({
            "claudeCode": { "promptQueueing": false }
        }));
        assert_eq!(PromptQueueing::read(&capabilities), PromptQueueing::Unknown);
    }

    /// RECEIPT: kimi 0.37.2's typed refusal, verbatim off the wire.
    #[test]
    fn the_typed_busy_code_is_what_demotes_an_adapter() {
        let refused = agent_client_protocol::Error::new(
            -32600,
            "Invalid request: another turn is already in progress",
        )
        .data(serde_json::json!({ "code": "turn.agent_busy" }));
        assert!(busy(&refused));
    }

    /// Prose is never matched on: the same message with no typed code is not
    /// evidence of a busy turn.
    #[test]
    fn an_untyped_error_is_not_a_busy_turn() {
        let error = agent_client_protocol::Error::new(
            -32600,
            "Invalid request: another turn is already in progress",
        );
        assert!(!busy(&error));
        let other = agent_client_protocol::Error::new(-32602, "Invalid params")
            .data(serde_json::json!({ "code": "model.rejected" }));
        assert!(!busy(&other));
    }

    /// The delivery timing is unchanged while the policy is unsettled, even on
    /// an adapter that advertises queueing.
    #[test]
    fn steer_still_waits_for_the_turn_boundary() {
        let mut channel = idle_channel();
        channel.queueing = PromptQueueing::Advertised;
        assert_eq!(channel.steer("anything").unwrap(), Delivery::NextTurn);
    }

    /// Each harness names its adapter once, and every row is a program plus
    /// the argument that puts it in ACP server mode.
    #[test]
    fn every_roster_row_spawns_something_in_acp_mode() {
        for adapter in [
            CLAUDE_ADAPTER,
            CODEX_ADAPTER,
            KIMI_ADAPTER,
            OPENCODE_ADAPTER,
        ] {
            assert!(adapter.len() >= 2, "{adapter:?}");
            assert!(!adapter[0].is_empty(), "{adapter:?}");
            assert!(
                adapter.iter().any(|part| part.contains("acp")),
                "{adapter:?}"
            );
        }
        assert_eq!(CLAUDE_ADAPTER[0], "npx");
        assert_eq!(CODEX_ADAPTER[0], "npx");
        assert_eq!(KIMI_ADAPTER, ["kimi", "acp"]);
    }

    /// RECEIPT for the capability flip: claude and kimi advertised
    /// `send_midflight` on their old transports and cannot on this one, so
    /// every steer is held for the next turn.
    #[test]
    fn no_text_reaches_a_turn_already_in_flight() {
        let mut channel = idle_channel();
        assert_eq!(channel.steer("more context").unwrap(), Delivery::NextTurn);
    }

    /// The stall watchdog reads `last_activity_ms`; an unwritten clock must
    /// read as no signal rather than as the epoch.
    #[test]
    fn an_unwritten_update_clock_is_no_signal() {
        let channel = idle_channel();
        assert_eq!(channel.last_activity_ms(), None);
        channel.last_update_ms.store(1_700_000_000_000, Ordering::Relaxed);
        assert_eq!(channel.last_activity_ms(), Some(1_700_000_000_000));
    }

    /// A join with no turn running is answered, never blocked on.
    #[test]
    fn a_join_without_a_running_turn_answers_at_once() {
        let mut channel = idle_channel();
        let event = channel
            .next_event(Duration::from_millis(1))
            .unwrap()
            .expect("an idle channel answers a join");
        assert!(!event.is_done(), "{event:?}");
        assert_eq!(event.detail(), "no acp turn to join");
    }

    /// One live pong turn against a real adapter. The caller asserts, so a
    /// failure names its harness through the test name.
    fn live_pong_turn(adapter: &[&str], model: Option<&str>, cap: Duration) -> TurnEvent {
        let spec = ChannelSpec {
            model: model.map(str::to_owned),
            cwd: std::env::temp_dir(),
            resume: None,
            lane: None,
        };
        let opened = std::time::Instant::now();
        let mut channel = AcpChannel::open_adapter(&spec, adapter).unwrap();
        println!(
            "{} session {:?} in {:?}",
            adapter.join(" "),
            channel.conversation_id(),
            opened.elapsed()
        );
        assert_eq!(channel.conversation_id_kind(), "acp_session");

        let started = std::time::Instant::now();
        channel
            .start_turn("reply with the single word pong")
            .unwrap();
        let deadline = started + cap;
        let verdict = loop {
            if let Some(event) = channel.next_event(Duration::from_millis(200)).unwrap() {
                break event;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no turn verdict within {cap:?}"
            );
        };
        println!("verdict {verdict:?} in {:?}", started.elapsed());
        println!("last_activity_ms {:?}", channel.last_activity_ms());
        channel.close().unwrap();
        verdict
    }

    /// The live legs. Each needs its adapter reachable and a working provider,
    /// so they stay off the default run: `cargo test -p boop-acp --lib
    /// channel::acp -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn a_real_opencode_acp_turn_ends_the_turn() {
        let verdict = live_pong_turn(
            OPENCODE_ADAPTER,
            // Under ACP opencode ignores its own config and hangs on a dead
            // default, so this one names its model.
            Some("openrouter/deepseek/deepseek-v4-flash-0731"),
            Duration::from_secs(60),
        );
        assert!(verdict.is_done(), "{verdict:?}");
        assert_eq!(verdict.detail(), "end_turn");
    }

    /// Each model id is the adapter's own spelling, read off its session
    /// reply, so these legs measure the model lever as well as the transport.
    #[test]
    #[ignore]
    fn a_real_claude_acp_turn_ends_the_turn() {
        let verdict = live_pong_turn(CLAUDE_ADAPTER, Some("sonnet"), Duration::from_secs(60));
        assert!(verdict.is_done(), "{verdict:?}");
        assert_eq!(verdict.detail(), "end_turn");
    }

    /// codex-acp's `model` option takes the family alone; the effort rides a
    /// separate `reasoning_effort` option, so boop's `gpt-5.6-luna@medium`
    /// spelling reaches no value here.
    #[test]
    #[ignore]
    fn a_real_codex_acp_turn_ends_the_turn() {
        let verdict = live_pong_turn(CODEX_ADAPTER, Some("gpt-5.6-luna"), Duration::from_secs(60));
        assert!(verdict.is_done(), "{verdict:?}");
        assert_eq!(verdict.detail(), "end_turn");
    }

    #[test]
    #[ignore]
    fn a_real_kimi_acp_turn_ends_the_turn() {
        let verdict = live_pong_turn(KIMI_ADAPTER, Some("kimi-code/k3"), Duration::from_secs(60));
        assert!(verdict.is_done(), "{verdict:?}");
        assert_eq!(verdict.detail(), "end_turn");
    }

    /// The resume leg. Every adapter on the roster advertises `loadSession`,
    /// so a second child takes the first one's session id back over
    /// `session/load` and keeps it as the conversation id.
    fn live_resumed_turn(adapter: &[&str], model: Option<&str>) -> (String, String) {
        let spec = ChannelSpec {
            model: model.map(str::to_owned),
            cwd: std::env::temp_dir(),
            resume: None,
            lane: None,
        };
        let mut first = AcpChannel::open_adapter(&spec, adapter).unwrap();
        let session = first.conversation_id().expect("a session id");
        first.start_turn("remember the number 41").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while first.next_event(Duration::from_millis(200)).unwrap().is_none() {
            assert!(std::time::Instant::now() < deadline, "first turn never ended");
        }
        first.close().unwrap();

        let resumed = ChannelSpec {
            resume: Some(session.clone()),
            ..spec
        };
        let mut second = AcpChannel::open_adapter(&resumed, adapter).unwrap();
        let carried = second.conversation_id().expect("a resumed session id");
        second
            .start_turn("reply with the single word pong")
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        let verdict = loop {
            if let Some(event) = second.next_event(Duration::from_millis(200)).unwrap() {
                break event;
            }
            assert!(std::time::Instant::now() < deadline, "resumed turn never ended");
        };
        second.close().unwrap();
        assert!(verdict.is_done(), "{verdict:?}");
        (session, carried)
    }

    #[test]
    #[ignore]
    fn a_real_claude_acp_session_is_resumed_by_a_second_child() {
        let (session, carried) = live_resumed_turn(CLAUDE_ADAPTER, Some("sonnet"));
        println!("claude session {session} resumed as {carried}");
        assert_eq!(session, carried);
    }

    #[test]
    #[ignore]
    fn a_real_kimi_acp_session_is_resumed_by_a_second_child() {
        let (session, carried) = live_resumed_turn(KIMI_ADAPTER, Some("kimi-code/k3"));
        println!("kimi session {session} resumed as {carried}");
        assert_eq!(session, carried);
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
