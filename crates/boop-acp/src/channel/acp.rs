//! The Agent Client Protocol lane channel, on the `agent-client-protocol`
//! crate. `LaneChannel` is sync and the ACP connection is async and scoped to
//! `Builder::connect_with`, so the connection owns a thread and the two sides
//! trade `Command`/`Note`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    CancelNotification, ClientCapabilities, ContentBlock, CreateTerminalRequest,
    CreateTerminalResponse, InitializeRequest, KillTerminalRequest, KillTerminalResponse,
    LoadSessionRequest, NewSessionRequest, PermissionOptionKind, PromptRequest,
    ReleaseTerminalRequest, ReleaseTerminalResponse, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome,
    SessionConfigKind, SessionConfigOption, SessionConfigOptionValue, SessionConfigSelectOptions,
    SessionId, SessionNotification, SessionUpdate, SetSessionConfigOptionRequest, StopReason,
    TerminalOutputRequest, TerminalOutputResponse, WaitForTerminalExitRequest,
    WaitForTerminalExitResponse,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, ConnectionTo, LineDirection};
use anyhow::{Context, Result};
use boop_store::session::ModelSpec;
use tracing::{debug, info, warn};

use crate::channel::terminal::{await_exit, Terminals};
use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEvent, TurnReceipt};

/// The config option every ACP agent names its model with.
const MODEL_CONFIG_ID: &str = "model";

/// The config option codex-acp carries reasoning effort on. Its `model`
/// option takes the family alone, so `gpt-5.6-luna@medium` matches no value.
const EFFORT_CONFIG_ID: &str = "reasoning_effort";

/// The config option codex-acp names its permission mode with, and the value
/// that means no sandbox and no approval prompts.
///
/// codex-acp opens every session on its `agent` mode: `workspace-write` plus
/// `on-request` approvals. A lane writes outside its worktree on every boop
/// call (the mail dir, `~/.agent/boop.db`, the repo's `.git/worktrees`), and a
/// codex native subagent the lane spawns inherits the session's sandbox, so
/// its `boop` calls fail with `Operation not permitted` and `attempt to write
/// a readonly database`. The mode is set once at the handshake and every agent
/// in the conversation runs under it.
///
/// The value id is codex-acp's own; no other adapter offers it, so no other
/// harness's permissions move.
const MODE_CONFIG_ID: &str = "mode";
const FULL_ACCESS_MODE: &str = "agent-full-access";

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

/// The argv a roster row spawns as. An `executable` override replaces the
/// row's program and keeps its arguments, so `["kimi", "acp"]` under `ccz`
/// spawns `ccz acp` and a bare `["codex-acp"]` spawns `ccz` alone.
pub fn adapter_command(adapter: &[&str], executable: Option<&str>) -> Vec<String> {
    let mut command: Vec<String> = adapter.iter().map(|part| (*part).to_owned()).collect();
    if let (Some(executable), Some(program)) = (executable, command.first_mut()) {
        *program = executable.to_owned();
    }
    command
}

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
                    debug!(
                        lane = lane.as_deref().unwrap_or_default(),
                        line, "acp agent stderr"
                    )
                }
                _ => debug!(?direction, line, "acp wire"),
            },
        );

        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (note_tx, note_rx) = std::sync::mpsc::channel();
        let last_update_ms = Arc::new(AtomicU64::new(0));
        // The agent's `model` option takes the family alone, so an `@effort`
        // suffix is split off here and set as its own option below.
        let model_spec = spec
            .model
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(str::parse::<ModelSpec>)
            .transpose()?;
        let plan = SessionPlan {
            cwd,
            model: model_spec.as_ref().map(|spec| spec.name.clone()),
            effort: spec
                .effort
                .clone()
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    model_spec
                        .as_ref()
                        .and_then(|spec| spec.effort)
                        .map(|effort| effort.as_str().to_owned())
                }),
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

    /// Open one of the roster adapters. The roster rows are `&[&str]`, so a
    /// caller names a const instead of building a `Vec<String>`.
    pub fn open_adapter(spec: &ChannelSpec, adapter: &[&str]) -> Result<AcpChannel> {
        AcpChannel::open(spec, &adapter_command(adapter, spec.executable.as_deref()))
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
    effort: Option<String>,
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
    let turn_receipt = Arc::new(Mutex::new(TurnReceipt::default()));
    let observed_receipt = Arc::clone(&turn_receipt);
    let mut commands = commands;
    let terminals = Arc::new(Terminals::new(plan.cwd.clone()));
    agent_client_protocol::Client
        .builder()
        .name("boop")
        .on_receive_notification(
            async move |notification: SessionNotification, _connection| {
                clock.store(crate::channel::now_ms(), Ordering::Relaxed);
                observe_turn(
                    &mut observed_receipt.lock().expect("turn receipt mutex poisoned"),
                    &notification.update,
                );
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
        .on_receive_request(
            {
                let terminals = Arc::clone(&terminals);
                async move |request: CreateTerminalRequest, responder, _connection| {
                    match terminals.create(&request) {
                        Ok(id) => responder.respond(CreateTerminalResponse::new(id)),
                        Err(error) => responder.respond_with_error(terminal_error(error)),
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let terminals = Arc::clone(&terminals);
                async move |request: TerminalOutputRequest, responder, _connection| {
                    match terminals.output(&request.terminal_id) {
                        Ok((output, truncated, status)) => responder.respond(
                            TerminalOutputResponse::new(output, truncated).exit_status(status),
                        ),
                        Err(error) => responder.respond_with_error(terminal_error(error)),
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let terminals = Arc::clone(&terminals);
                async move |request: WaitForTerminalExitRequest, responder, connection| {
                    // The wait outlives the dispatch loop, so it rides its own
                    // task and the connection keeps reading frames meanwhile.
                    match terminals.exit_watch(&request.terminal_id) {
                        Ok(mut exit) => connection.spawn(async move {
                            let status = await_exit(&mut exit).await;
                            responder.respond(WaitForTerminalExitResponse::new(status))
                        }),
                        Err(error) => responder.respond_with_error(terminal_error(error)),
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let terminals = Arc::clone(&terminals);
                async move |request: KillTerminalRequest, responder, _connection| {
                    match terminals.kill(&request.terminal_id) {
                        Ok(()) => responder.respond(KillTerminalResponse::new()),
                        Err(error) => responder.respond_with_error(terminal_error(error)),
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let terminals = Arc::clone(&terminals);
                async move |request: ReleaseTerminalRequest, responder, _connection| {
                    match terminals.release(&request.terminal_id) {
                        Ok(()) => responder.respond(ReleaseTerminalResponse::new()),
                        Err(error) => responder.respond_with_error(terminal_error(error)),
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
                        *turn_receipt.lock().expect("turn receipt mutex poisoned") =
                            TurnReceipt::default();
                        let outcome = connection
                            .send_request(PromptRequest::new(
                                session.clone(),
                                vec![text.into()],
                            ))
                            .block_task()
                            .await
                            .map(|response| response.stop_reason);
                        let receipt = std::mem::take(
                            &mut *turn_receipt.lock().expect("turn receipt mutex poisoned"),
                        );
                        if notes
                            .send(Note::Turn(turn_verdict(outcome, Some(receipt))))
                            .is_err()
                        {
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

/// The JSON-RPC error a failed terminal call answers with. The default
/// internal error hides its cause in `data`, which no agent prints.
fn terminal_error(error: anyhow::Error) -> agent_client_protocol::Error {
    let mut frame = agent_client_protocol::Error::internal_error();
    frame.message = error.to_string();
    frame
}

/// What boop serves back. `terminal` is the whole of it: kimi runs every
/// shell command through the client, and `fs/*` is left to the agent's own.
fn client_capabilities() -> ClientCapabilities {
    ClientCapabilities::new().terminal(true)
}

/// `initialize`, session, then model. Under ACP opencode ignores
/// `opencode.json` and `OPENCODE_MODEL` and hangs on its dead default, so the
/// config-option call is the only model lever.
async fn handshake(
    connection: &ConnectionTo<Agent>,
    plan: &SessionPlan,
) -> Result<agent_client_protocol::schema::v1::SessionId, agent_client_protocol::Error> {
    let initialized = connection
        .send_request(
            InitializeRequest::new(ProtocolVersion::V1).client_capabilities(client_capabilities()),
        )
        .block_task()
        .await?;
    info!(
        agent = ?initialized.agent_info,
        protocol_version = ?initialized.protocol_version,
        load_session = initialized.agent_capabilities.load_session,
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

    let offered = config_options.as_deref().unwrap_or_default();
    select_full_access_mode(connection, &session, offered).await;
    if let Some(model) = plan.model.as_deref() {
        select_model(connection, &session, model, offered).await?;
    }
    if let Some(effort) = plan.effort.as_deref() {
        select_effort(connection, &session, effort, offered).await;
    }
    Ok(session)
}

/// Put the session on the agent's full-access mode when it offers one. An
/// agent that offers no such value is left alone, and a refusal is a warning
/// rather than an open failure: the lane still runs, it just cannot write
/// outside its worktree, and the first `boop` call says so in its own words.
async fn select_full_access_mode(
    connection: &ConnectionTo<Agent>,
    session: &SessionId,
    config_options: &[SessionConfigOption],
) {
    if !offers_value(config_options, MODE_CONFIG_ID, FULL_ACCESS_MODE) {
        return;
    }
    match connection
        .send_request(SetSessionConfigOptionRequest::new(
            session.clone(),
            MODE_CONFIG_ID,
            SessionConfigOptionValue::value_id(FULL_ACCESS_MODE.to_owned()),
        ))
        .block_task()
        .await
    {
        Ok(_) => info!(mode = FULL_ACCESS_MODE, "acp session mode set"),
        Err(error) => warn!(
            mode = FULL_ACCESS_MODE,
            error = error.message,
            "acp agent refused the full-access mode; the lane runs sandboxed"
        ),
    }
}

/// Whether the agent's `option_id` select offers `value_id`.
fn offers_value(config_options: &[SessionConfigOption], option_id: &str, value_id: &str) -> bool {
    select_value_ids(config_options, option_id)
        .is_some_and(|values| values.iter().any(|offered| offered.as_str() == value_id))
}

/// Name the reasoning effort on the agent's own option. An agent that offers
/// no such option is left alone; a refusal is a warning, never an open failure.
async fn select_effort(
    connection: &ConnectionTo<Agent>,
    session: &SessionId,
    effort: &str,
    config_options: &[SessionConfigOption],
) {
    if !offers_value(config_options, EFFORT_CONFIG_ID, effort) {
        return;
    }
    match connection
        .send_request(SetSessionConfigOptionRequest::new(
            session.clone(),
            EFFORT_CONFIG_ID,
            SessionConfigOptionValue::value_id(effort.to_owned()),
        ))
        .block_task()
        .await
    {
        Ok(_) => info!(effort, "acp session reasoning effort set"),
        Err(error) => warn!(effort, error = %error.message, "acp reasoning effort refused"),
    }
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
    Some(select_value_ids(config_options, MODEL_CONFIG_ID)?.join(", "))
}

/// Every value id the agent's `option_id` select offers, in the order it sent
/// them. `None` for an option it does not carry or one that is not a select.
fn select_value_ids(
    config_options: &[SessionConfigOption],
    option_id: &str,
) -> Option<Vec<String>> {
    let select = config_options
        .iter()
        .find(|option| option.id.0.as_ref() == option_id)
        .and_then(|option| match &option.kind {
            SessionConfigKind::Select(select) => Some(select),
            _ => None,
        })?;
    let values: Vec<String> = match &select.options {
        SessionConfigSelectOptions::Ungrouped(options) => options
            .iter()
            .map(|option| option.value.0.as_ref().to_owned())
            .collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .map(|option| option.value.0.as_ref().to_owned())
            .collect(),
        _ => return None,
    };
    Some(values)
}

/// The turn verdict for one `session/prompt` outcome. A JSON-RPC error is a
/// flake the agent never saw; a non-`end_turn` stop reason is its own answer.
fn turn_verdict(
    outcome: Result<StopReason, agent_client_protocol::Error>,
    receipt: Option<TurnReceipt>,
) -> TurnEvent {
    match outcome {
        Ok(StopReason::EndTurn) => match receipt {
            Some(receipt) => TurnEvent::ok_with_receipt("end_turn", receipt),
            None => TurnEvent::ok("end_turn"),
        },
        Ok(other) => TurnEvent::failed(format!("stop_reason={}", stop_reason_name(other))),
        Err(error) => TurnEvent::flaked(error.message),
    }
}

fn observe_turn(receipt: &mut TurnReceipt, update: &SessionUpdate) {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            if let ContentBlock::Text(text) = &chunk.content {
                receipt.text.push_str(&text.text);
            }
        }
        SessionUpdate::ToolCall(_) => receipt.tool_calls += 1,
        _ => {}
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
        ContentChunk, PermissionOption, PermissionOptionId, SessionConfigId,
        SessionConfigSelectOption, SessionConfigValueId, TextContent, ToolCall, ToolCallId,
        ToolCallUpdate, ToolCallUpdateFields,
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
        let verdict = turn_verdict(
            Err(prompt_error_frame(
                "AI_APICallError: Upstream request failed: Endpoint is unavailable.",
            )),
            None,
        );
        assert!(verdict.retryable(), "{verdict:?}");
        assert_eq!(
            verdict.detail(),
            "AI_APICallError: Upstream request failed: Endpoint is unavailable."
        );
    }

    #[test]
    fn end_turn_is_the_only_clean_verdict() {
        assert!(turn_verdict(Ok(StopReason::EndTurn), None).is_done());
        assert_eq!(
            turn_verdict(Ok(StopReason::EndTurn), None).detail(),
            "end_turn"
        );
    }

    #[test]
    fn every_other_stop_reason_is_a_terminal_failure() {
        for reason in [
            StopReason::Cancelled,
            StopReason::Refusal,
            StopReason::MaxTokens,
            StopReason::MaxTurnRequests,
        ] {
            let verdict = turn_verdict(Ok(reason), None);
            assert!(!verdict.is_done(), "{verdict:?}");
            assert!(!verdict.retryable(), "{verdict:?}");
            assert_eq!(
                verdict.detail(),
                format!("stop_reason={}", stop_reason_name(reason))
            );
        }
    }

    #[test]
    fn turn_observation_collects_agent_text_and_counts_tool_calls() {
        let mut receipt = TurnReceipt::default();
        for text in ["bo", "op"] {
            observe_turn(
                &mut receipt,
                &SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                    TextContent::new(text),
                ))),
            );
        }
        observe_turn(
            &mut receipt,
            &SessionUpdate::ToolCall(ToolCall::new(ToolCallId::new("call_1"), "list files")),
        );
        assert_eq!(
            receipt,
            TurnReceipt {
                text: "boop".into(),
                tool_calls: 1,
            }
        );
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

    /// Defect 1 (addendum 2026-08-25): codex-acp opens on `agent`
    /// (workspace-write), so the lane and every native subagent it spawns run
    /// sandboxed and boop cannot write the mail dir or the store.
    #[test]
    fn the_codex_full_access_mode_is_taken_when_the_agent_offers_it() {
        let options = vec![SessionConfigOption::select(
            SessionConfigId::new(MODE_CONFIG_ID),
            "Mode",
            SessionConfigValueId::new("agent"),
            vec![
                SessionConfigSelectOption::new(SessionConfigValueId::new("read-only"), "Read-only"),
                SessionConfigSelectOption::new(SessionConfigValueId::new("agent"), "Agent"),
                SessionConfigSelectOption::new(
                    SessionConfigValueId::new(FULL_ACCESS_MODE),
                    "Agent (full access)",
                ),
            ],
        )];
        assert!(offers_value(&options, MODE_CONFIG_ID, FULL_ACCESS_MODE));
        assert_eq!(
            select_value_ids(&options, MODE_CONFIG_ID),
            Some(vec![
                "read-only".to_owned(),
                "agent".to_owned(),
                FULL_ACCESS_MODE.to_owned(),
            ])
        );
    }

    /// An adapter with different mode ids keeps its own permissions: only the
    /// exact codex-acp value is taken.
    #[test]
    fn an_agent_offering_no_full_access_mode_is_left_alone() {
        let claude_modes = vec![SessionConfigOption::select(
            SessionConfigId::new(MODE_CONFIG_ID),
            "Mode",
            SessionConfigValueId::new("default"),
            vec![
                SessionConfigSelectOption::new(SessionConfigValueId::new("default"), "Default"),
                SessionConfigSelectOption::new(
                    SessionConfigValueId::new("bypassPermissions"),
                    "Bypass permissions",
                ),
            ],
        )];
        assert!(!offers_value(
            &claude_modes,
            MODE_CONFIG_ID,
            FULL_ACCESS_MODE
        ));
        assert!(!offers_value(&[], MODE_CONFIG_ID, FULL_ACCESS_MODE));
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
        }
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

    /// RECEIPT. An executable override replaces the roster row's program and
    /// nothing else; no override leaves the row byte-identical. Sabotage:
    /// replacing the whole command drops `acp` and the child never speaks the
    /// protocol.
    #[test]
    fn an_executable_override_replaces_only_the_adapter_program() {
        assert_eq!(
            adapter_command(KIMI_ADAPTER, Some("ccz")),
            ["ccz", "acp"],
            "kimi row"
        );
        assert_eq!(
            adapter_command(OPENCODE_ADAPTER, Some("ccz")),
            ["ccz", "acp"],
            "opencode row"
        );
        assert_eq!(
            adapter_command(&["codex-acp"], Some("ccz")),
            ["ccz"],
            "one-word row"
        );
        assert_eq!(
            adapter_command(CODEX_ADAPTER, None),
            CODEX_ADAPTER
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>(),
            "no override"
        );
        assert!(adapter_command(&[], Some("ccz")).is_empty());
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
        channel
            .last_update_ms
            .store(1_700_000_000_000, Ordering::Relaxed);
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
            effort: None,
            model: model.map(str::to_owned),
            cwd: std::env::temp_dir(),
            resume: None,
            lane: None,
            executable: None,
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
            effort: None,
            model: model.map(str::to_owned),
            cwd: std::env::temp_dir(),
            resume: None,
            lane: None,
            executable: None,
        };
        let mut first = AcpChannel::open_adapter(&spec, adapter).unwrap();
        let session = first.conversation_id().expect("a session id");
        first.start_turn("remember the number 41").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while first
            .next_event(Duration::from_millis(200))
            .unwrap()
            .is_none()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "first turn never ended"
            );
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
            assert!(
                std::time::Instant::now() < deadline,
                "resumed turn never ended"
            );
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
            effort: None,
            model: None,
            cwd: std::env::temp_dir(),
            resume: None,
            lane: None,
            executable: None,
        };
        let error = match AcpChannel::open(&spec, &[]) {
            Ok(_) => panic!("an empty command opened a channel"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("needs a command"), "{error}");
    }
}

/// The wire leg of the terminal methods, against a fake agent that speaks
/// just enough ACP to drive all five and write down what it saw.
#[cfg(test)]
mod terminal_wire_tests {
    use super::*;
    use std::sync::OnceLock;

    const FAKE_AGENT: &str = r#"
import json, sys, time

report_path = sys.argv[1]
report = {}
next_id = [100]

def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()

def call(method, params):
    next_id[0] += 1
    ident = next_id[0]
    send({"jsonrpc": "2.0", "id": ident, "method": method, "params": params})
    while True:
        line = sys.stdin.readline()
        if not line:
            raise SystemExit(0)
        frame = json.loads(line)
        if frame.get("id") == ident and ("result" in frame or "error" in frame):
            if "error" in frame:
                return {"error": frame["error"].get("message", "")}
            return frame["result"]

def run_terminals(session):
    first = call("terminal/create", {
        "sessionId": session,
        "command": "sh",
        "args": ["-c", "printf hello; printf oops 1>&2; exit 5"],
        "env": [{"name": "BOOP_FAKE_TERMINAL", "value": "on"}],
        "outputByteLimit": 65536,
    })
    report["create"] = first
    terminal = first.get("terminalId")
    report["wait_for_exit"] = call("terminal/wait_for_exit", {"sessionId": session, "terminalId": terminal})
    report["output"] = call("terminal/output", {"sessionId": session, "terminalId": terminal})
    report["release"] = call("terminal/release", {"sessionId": session, "terminalId": terminal})
    report["output_after_release"] = call("terminal/output", {"sessionId": session, "terminalId": terminal})

    second = call("terminal/create", {
        "sessionId": session,
        "command": "sh",
        "args": ["-c", "printf running; sleep 30"],
        "outputByteLimit": 65536,
    })
    standing = second.get("terminalId")
    for _ in range(200):
        seen = call("terminal/output", {"sessionId": session, "terminalId": standing})
        if "running" in seen.get("output", ""):
            break
        time.sleep(0.01)
    report["kill"] = call("terminal/kill", {"sessionId": session, "terminalId": standing})
    report["kill_exit"] = call("terminal/wait_for_exit", {"sessionId": session, "terminalId": standing})
    report["kill_output"] = call("terminal/output", {"sessionId": session, "terminalId": standing})
    call("terminal/release", {"sessionId": session, "terminalId": standing})
    report["missing_terminal"] = call("terminal/kill", {"sessionId": session, "terminalId": "term_404"})

session_id = "ses_fake"
while True:
    line = sys.stdin.readline()
    if not line:
        break
    frame = json.loads(line)
    method = frame.get("method")
    if method == "initialize":
        report["client_capabilities"] = frame["params"].get("clientCapabilities", {})
        send({"jsonrpc": "2.0", "id": frame["id"], "result": {
            "protocolVersion": 1,
            "agentCapabilities": {},
            "authMethods": [],
        }})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": frame["id"], "result": {"sessionId": session_id}})
    elif method == "session/prompt":
        run_terminals(frame["params"]["sessionId"])
        with open(report_path, "w") as handle:
            json.dump(report, handle)
        send({"jsonrpc": "2.0", "id": frame["id"], "result": {"stopReason": "end_turn"}})
    elif "id" in frame:
        send({"jsonrpc": "2.0", "id": frame["id"], "result": {}})
"#;

    /// One turn against the fake agent, shared by every assertion below.
    fn report() -> &'static serde_json::Value {
        static REPORT: OnceLock<serde_json::Value> = OnceLock::new();
        REPORT.get_or_init(|| {
            let root =
                std::env::temp_dir().join(format!("boop-acp-terminal-{}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            let script = root.join("fake_agent.py");
            let written = root.join("report.json");
            std::fs::write(&script, FAKE_AGENT).unwrap();

            let spec = ChannelSpec {
                model: None,
                effort: None,
                cwd: root.clone(),
                resume: None,
                lane: None,
                executable: None,
            };
            let command = vec![
                "python3".to_owned(),
                script.display().to_string(),
                written.display().to_string(),
            ];
            let mut channel = AcpChannel::open(&spec, &command).unwrap();
            channel.start_turn("run the terminals").unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(60);
            let verdict = loop {
                if let Some(event) = channel.next_event(Duration::from_millis(100)).unwrap() {
                    break event;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "the fake agent never answered"
                );
            };
            channel.close().unwrap();
            assert!(verdict.is_done(), "{verdict:?}");
            serde_json::from_str(&std::fs::read_to_string(&written).unwrap()).unwrap()
        })
    }

    #[test]
    fn initialize_advertises_the_terminal_capability() {
        assert_eq!(
            report()["client_capabilities"]["terminal"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn terminal_create_answers_with_an_id() {
        let id = report()["create"]["terminalId"].as_str().unwrap();
        assert!(id.starts_with("term_"), "{id}");
    }

    #[test]
    fn terminal_wait_for_exit_answers_with_the_code() {
        assert_eq!(report()["wait_for_exit"]["exitCode"], serde_json::json!(5));
    }

    #[test]
    fn terminal_output_carries_both_pipes_and_the_status() {
        let output = &report()["output"];
        let text = output["output"].as_str().unwrap();
        assert!(text.contains("hello"), "{text}");
        assert!(text.contains("oops"), "{text}");
        assert_eq!(output["truncated"], serde_json::json!(false));
        assert_eq!(output["exitStatus"]["exitCode"], serde_json::json!(5));
    }

    #[test]
    fn terminal_kill_stops_the_child_and_leaves_it_readable() {
        assert!(
            report()["kill"].get("error").is_none(),
            "{:?}",
            report()["kill"]
        );
        assert_eq!(
            report()["kill_exit"]["signal"],
            serde_json::json!("SIGKILL")
        );
        assert_eq!(
            report()["kill_output"]["output"],
            serde_json::json!("running")
        );
    }

    #[test]
    fn terminal_release_frees_the_id() {
        assert!(
            report()["release"].get("error").is_none(),
            "{:?}",
            report()["release"]
        );
        let after = report()["output_after_release"]["error"].as_str().unwrap();
        assert!(after.starts_with("no terminal term_"), "{after}");
    }

    #[test]
    fn an_unknown_terminal_id_answers_an_error_rather_than_wedging_the_turn() {
        let error = report()["missing_terminal"]["error"].as_str().unwrap();
        assert_eq!(error, "no terminal term_404");
    }
}
