//! One long-lived process per addressed route: it serves the `Agent` role on
//! the route's unix socket and the `Client` role against the adapter child.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PermissionOptionKind, PromptRequest, PromptResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionId, SessionNotification, SessionUpdate,
};
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, Agent, ByteStreams, Client, ConnectionTo, LineDirection,
};
use anyhow::{Context, Result};
use boop_store::bus;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use crate::channel::acp::{busy, PromptQueueing, SessionPlan};

/// The mail poll, the same cadence a lane supervisor already reads its inbox
/// at (`boop-proc/src/supervise.rs`).
pub const POLL: Duration = Duration::from_millis(700);

/// A resident process's whole CPU claim; nothing on this path is compute.
const WORKER_THREADS: usize = 2;

/// The blocking pool `AcpAgent`'s child pipes and the socket accept run on.
const BLOCKING_THREADS: usize = 4;

/// How far behind the fan-out a slow client may fall before it is dropped;
/// the bound is what keeps a stalled reader off the host's memory.
const UPDATE_BACKLOG: usize = 4096;

/// `sun_path` is 104 bytes on Darwin including the terminator, and the bind
/// reports the overrun as an ENAMETOOLONG carrying no path.
const SUN_PATH_MAX: usize = 103;

/// The kinds a host turns into a turn. Its own dispatch and result rows are
/// bookkeeping and would loop straight back into the session.
const DELIVERABLE: &[&str] = &["request", "hail", "note", "retry", "resume"];

/// What one host needs before it binds.
pub struct HostSpec {
    pub route: String,
    /// The adapter argv, one of the roster rows in `channel::acp`.
    pub adapter: Vec<String>,
    pub cwd: PathBuf,
    pub model: Option<String>,
    /// The ACP session id a previous host pinned on this route.
    pub resume: Option<String>,
    pub mail_dir: PathBuf,
    pub poll: Duration,
}

/// The door a route's host binds and an attach shim connects to.
pub fn socket_path(mail_dir: &Path, route: &str) -> PathBuf {
    mail_dir
        .parent()
        .unwrap_or(mail_dir)
        .join("acp")
        .join(format!("{}.sock", route.replace('/', "_")))
}

/// True when a host answers on the route's socket. A path left behind by a
/// killed host refuses the connect, so the file's existence proves nothing.
pub fn route_host_alive(mail_dir: &Path, route: &str) -> bool {
    std::os::unix::net::UnixStream::connect(socket_path(mail_dir, route)).is_ok()
}

/// Bind the route's door. A live bind is the uniqueness proof, so this runs
/// before anything spawns a child: a second host on one route exits here.
fn bind_exclusive(socket: &Path, route: &str) -> Result<std::os::unix::net::UnixListener> {
    if socket.as_os_str().len() > SUN_PATH_MAX {
        anyhow::bail!(
            "the socket path for route `{route}` is {} bytes and a unix socket takes {SUN_PATH_MAX}: {}",
            socket.as_os_str().len(),
            socket.display()
        );
    }
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create the acp socket dir {}", parent.display()))?;
    }
    match std::os::unix::net::UnixListener::bind(socket) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            if std::os::unix::net::UnixStream::connect(socket).is_ok() {
                anyhow::bail!(
                    "route `{route}` already has a live acp host on {}",
                    socket.display()
                );
            }
            std::fs::remove_file(socket).with_context(|| {
                format!("unlink the stale acp socket {}", socket.display())
            })?;
            std::os::unix::net::UnixListener::bind(socket)
                .with_context(|| format!("bind the acp socket {}", socket.display()))
        }
        Err(error) => Err(anyhow::Error::new(error))
            .with_context(|| format!("bind the acp socket {}", socket.display())),
    }
}

/// Unlink the door on the way out so the next host binds instead of probing a
/// file nobody answers.
struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Run one host until its adapter child or its transport ends.
pub fn run(spec: HostSpec) -> Result<()> {
    let socket = socket_path(&spec.mail_dir, &spec.route);
    let listener = bind_exclusive(&socket, &spec.route)?;
    listener
        .set_nonblocking(true)
        .context("put the acp socket in nonblocking mode")?;
    let _guard = SocketGuard(socket.clone());
    info!(
        route = spec.route,
        socket = %socket.display(),
        adapter = %spec.adapter.join(" "),
        cwd = %spec.cwd.display(),
        resume = spec.resume.as_deref().unwrap_or_default(),
        "acp host bound"
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(WORKER_THREADS)
        .max_blocking_threads(BLOCKING_THREADS)
        .thread_name("boop-acp-host")
        .enable_time()
        .build()
        .context("build the acp host runtime")?;
    let result = runtime.block_on(serve(spec, listener));
    info!(?result, "acp host ended");
    result
}

async fn serve(spec: HostSpec, listener: std::os::unix::net::UnixListener) -> Result<()> {
    let listener = async_net::unix::UnixListener::try_from(listener)
        .context("adopt the acp socket onto the reactor")?;
    let (program, args) = spec
        .adapter
        .split_first()
        .context("an acp host needs an adapter command to spawn")?;
    let cwd = std::fs::canonicalize(&spec.cwd)
        .with_context(|| format!("resolve acp session cwd {}", spec.cwd.display()))?;
    let route = spec.route.clone();
    let agent = AcpAgent::new(AcpAgentConfig::new(program).args(args.to_vec())).with_debug(
        move |line, direction| match direction {
            LineDirection::Stderr => debug!(route, line, "acp adapter stderr"),
            _ => debug!(route, ?direction, line, "acp wire"),
        },
    );
    let plan = SessionPlan {
        cwd,
        model: spec.model.clone().filter(|value| !value.is_empty()),
        resume: spec.resume.clone(),
        clock: Arc::new(AtomicU64::new(0)),
    };

    let (messages, inbox) = mpsc::unbounded_channel::<HostMessage>();
    let (updates, _) = broadcast::channel::<SessionNotification>(UPDATE_BACKLOG);
    let fanout = updates.clone();

    let result = agent_client_protocol::Client
        .builder()
        .name("boop-acp-host")
        .on_receive_notification(
            async move |notification: SessionNotification, _connection| {
                // A host with no client attached has no subscribers, and a
                // send into an empty broadcast is not an error.
                let _ = fanout.send(notification);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _connection| {
                // Nobody is elected to answer these yet, and an unanswered
                // request wedges the turn, which is the worse failure.
                match allow_option(&request) {
                    Some(option) => responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option)),
                    )),
                    None => responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    )),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |upstream: ConnectionTo<Agent>| {
            let opened = crate::channel::acp::handshake(&upstream, &plan).await?;
            let queueing = PromptQueueing::read(&opened.initialized.agent_capabilities);
            info!(
                route = spec.route,
                conversation_id = %opened.session.0,
                conversation_id_kind = "acp_session",
                prompt_queueing = queueing.as_str(),
                resumed = plan.resume.is_some(),
                "acp host session open"
            );
            record_session(&spec.mail_dir, &spec.route, &opened.session.0);
            tokio::spawn(accept_loop(listener, messages.clone()));
            tokio::spawn(tick_loop(spec.poll, messages.clone()));
            Arbiter {
                route: spec.route.clone(),
                mail_dir: spec.mail_dir.clone(),
                mood: mood_template(&spec.route),
                session: opened.session,
                initialized: opened.initialized,
                queueing,
                upstream,
                updates,
                messages,
                seen: BTreeSet::new(),
                held: VecDeque::new(),
                inflight: 0,
            }
            .run(inbox)
            .await;
            Ok(())
        })
        .await;
    result.map_err(|error| anyhow::anyhow!("acp host connection ended: {}", error.message))
}

/// Accept downstream clients until the listener dies.
async fn accept_loop(
    listener: async_net::unix::UnixListener,
    messages: mpsc::UnboundedSender<HostMessage>,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                if messages.send(HostMessage::Attach(stream)).is_err() {
                    return;
                }
            }
            Err(error) => {
                warn!(error = %error, "acp host accept failed");
                return;
            }
        }
    }
}

/// The mail poll. One timer, never a spin: the interval is the only thing this
/// task ever waits on.
async fn tick_loop(poll: Duration, messages: mpsc::UnboundedSender<HostMessage>) {
    let mut interval = tokio::time::interval(poll);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        if messages.send(HostMessage::Tick).is_err() {
            return;
        }
    }
}

/// Where a turn came from, so a verdict names its origin in the trail.
#[derive(Clone, Debug)]
enum TurnOrigin {
    Human,
    Mail(String),
}

impl TurnOrigin {
    /// The mail row this turn came from; empty for a turn an attached client
    /// typed itself.
    fn message_id(&self) -> &str {
        match self {
            TurnOrigin::Human => "",
            TurnOrigin::Mail(id) => id,
        }
    }
}

enum HostMessage {
    /// The mail poll fired.
    Tick,
    /// A downstream client connected.
    Attach(async_net::unix::UnixStream),
    /// An attached client sent `session/prompt`.
    HumanPrompt {
        content: Vec<ContentBlock>,
        reply: oneshot::Sender<Result<PromptResponse, agent_client_protocol::Error>>,
    },
    /// An attached client sent `session/cancel`.
    HumanCancel,
    /// One upstream prompt reached a verdict.
    TurnDone {
        origin: TurnOrigin,
        outcome: Result<PromptResponse, agent_client_protocol::Error>,
    },
}

/// The one place `session/prompt` is called. Mail and every attached client
/// converge here, so the queueing policy is decided once.
struct Arbiter {
    route: String,
    mail_dir: PathBuf,
    mood: String,
    session: SessionId,
    initialized: InitializeResponse,
    queueing: PromptQueueing,
    upstream: ConnectionTo<Agent>,
    updates: broadcast::Sender<SessionNotification>,
    messages: mpsc::UnboundedSender<HostMessage>,
    /// Row ids already turned into a prompt by this host.
    seen: BTreeSet<String>,
    held: VecDeque<bus::Message>,
    inflight: usize,
}

impl Arbiter {
    async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<HostMessage>) {
        while let Some(message) = inbox.recv().await {
            match message {
                HostMessage::Tick => self.drain_mailbox(),
                HostMessage::Attach(stream) => self.attach(stream),
                HostMessage::HumanPrompt { content, reply } => self.prompt_human(content, reply),
                HostMessage::HumanCancel => {
                    if let Err(error) = self
                        .upstream
                        .send_notification(CancelNotification::new(self.session.clone()))
                    {
                        warn!(route = self.route, error = %error.message, "acp host cancel failed");
                    }
                }
                HostMessage::TurnDone { origin, outcome } => self.finish_turn(&origin, &outcome),
            }
        }
    }

    /// Every unacked row addressed to this route becomes a turn, or waits for
    /// the turn boundary.
    fn drain_mailbox(&mut self) {
        for row in pending(&self.mail_dir, &self.route, &self.seen) {
            self.seen.insert(row.id.clone());
            match self.inflight {
                0 => self.prompt_mail(row),
                _ => {
                    info!(
                        route = self.route,
                        message_id = row.id,
                        prompt_queueing = self.queueing.as_str(),
                        delivery = "nextturn",
                        "acp host held mail for the turn boundary"
                    );
                    self.held.push_back(row);
                }
            }
        }
    }

    /// Turn one mail row into a `session/prompt`, mirrored into every attached
    /// client's transcript as user content first.
    fn prompt_mail(&mut self, row: bus::Message) {
        let text = render_mail(&self.mood, &row);
        let _ = self.updates.send(SessionNotification::new(
            self.session.clone(),
            SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::from(text.clone()))),
        ));
        let sent = self.upstream.send_request(PromptRequest::new(
            self.session.clone(),
            vec![ContentBlock::from(text)],
        ));
        // Acked after the send and before the response: a host killed
        // mid-turn must not replay a row the session already has.
        ack(&self.mail_dir, &row);
        self.inflight += 1;
        info!(
            route = self.route,
            message_id = row.id,
            from = row.from,
            delivery = "prompt",
            "acp host delivered mail as a turn"
        );
        let messages = self.messages.clone();
        let origin = TurnOrigin::Mail(row.id);
        tokio::spawn(async move {
            let outcome = sent.block_task().await;
            let _ = messages.send(HostMessage::TurnDone { origin, outcome });
        });
    }

    /// Forward one attached client's prompt. Its own client serializes its
    /// turns, so this never waits on the mail queue.
    fn prompt_human(
        &mut self,
        content: Vec<ContentBlock>,
        reply: oneshot::Sender<Result<PromptResponse, agent_client_protocol::Error>>,
    ) {
        let sent = self
            .upstream
            .send_request(PromptRequest::new(self.session.clone(), content));
        self.inflight += 1;
        let messages = self.messages.clone();
        tokio::spawn(async move {
            let outcome = sent.block_task().await;
            let _ = reply.send(outcome.clone());
            let _ = messages.send(HostMessage::TurnDone {
                origin: TurnOrigin::Human,
                outcome,
            });
        });
    }

    fn finish_turn(
        &mut self,
        origin: &TurnOrigin,
        outcome: &Result<PromptResponse, agent_client_protocol::Error>,
    ) {
        self.inflight = self.inflight.saturating_sub(1);
        match outcome {
            Ok(response) => info!(
                route = self.route,
                message_id = origin.message_id(),
                stop_reason = ?response.stop_reason,
                "acp host turn ended"
            ),
            Err(error) if busy(error) => {
                self.queueing = PromptQueueing::Rejects;
                warn!(
                    route = self.route,
                    message_id = origin.message_id(),
                    prompt_queueing = self.queueing.as_str(),
                    error = %error.message,
                    "acp host demoted the adapter to rejects"
                );
            }
            Err(error) => warn!(
                route = self.route,
                message_id = origin.message_id(),
                error = %error.message,
                "acp host turn failed"
            ),
        }
        if self.inflight == 0 {
            if let Some(row) = self.held.pop_front() {
                self.prompt_mail(row);
            }
        }
    }

    /// Adopt one downstream connection: the host serves the `Agent` role on it
    /// and answers with the session it already owns.
    fn attach(&mut self, stream: async_net::unix::UnixStream) {
        let session = self.session.clone();
        let initialized = self.initialized.clone();
        let messages = self.messages.clone();
        let updates = self.updates.subscribe();
        let route = self.route.clone();
        info!(route = self.route, "acp host client attached");
        tokio::spawn(async move {
            if let Err(error) = serve_client(stream, session, initialized, messages, updates).await
            {
                debug!(route, error = %error.message, "acp host client detached");
            }
        });
    }
}

/// Serve the `Agent` role to one attached client. Its rpc ids are its own: the
/// host never forwards a downstream id, it makes its own upstream request and
/// answers the responder it was handed.
async fn serve_client(
    stream: async_net::unix::UnixStream,
    session: SessionId,
    initialized: InitializeResponse,
    messages: mpsc::UnboundedSender<HostMessage>,
    mut updates: broadcast::Receiver<SessionNotification>,
) -> Result<(), agent_client_protocol::Error> {
    let incoming = stream.clone();
    let outgoing = stream;
    let session_for_new = session.clone();
    let prompt_messages = messages.clone();
    agent_client_protocol::Agent
        .builder()
        .name("boop-acp-host-client")
        .on_receive_request(
            async move |_: InitializeRequest, responder, _connection| {
                responder.respond(initialized.clone())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_: NewSessionRequest, responder, _connection| {
                // The client believes it opened a session; it was handed the
                // one this host already owns.
                responder.respond(NewSessionResponse::new(session_for_new.clone()))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: PromptRequest, responder, connection: ConnectionTo<Client>| {
                let messages = prompt_messages.clone();
                // Handlers run on the connection's event loop, so awaiting the
                // turn here would deafen this client until it ends.
                connection.spawn(async move {
                    let (reply, answer) = oneshot::channel();
                    messages
                        .send(HostMessage::HumanPrompt {
                            content: request.prompt,
                            reply,
                        })
                        .map_err(|_| {
                            agent_client_protocol::util::internal_error("the acp host is gone")
                        })?;
                    match answer.await {
                        Ok(Ok(response)) => responder.respond(response),
                        Ok(Err(error)) => responder.respond_with_error(error),
                        Err(_) => responder
                            .respond_with_internal_error("the acp host dropped the prompt"),
                    }
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |_: CancelNotification, _connection| {
                messages
                    .send(HostMessage::HumanCancel)
                    .map_err(|_| agent_client_protocol::util::internal_error("the acp host is gone"))
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .with_spawned(|connection: ConnectionTo<Client>| async move {
            loop {
                match updates.recv().await {
                    Ok(notification) => connection.send_notification(notification)?,
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        warn!(missed, "acp host client fell behind the update fan-out");
                    }
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                }
            }
        })
        .connect_to(ByteStreams::new(outgoing, incoming))
        .await
}

/// The stdio-to-socket shim an unmodified ACP client spawns as if it were an
/// agent binary. It parses nothing: two byte pumps, one per direction.
pub fn attach(mail_dir: &Path, route: &str) -> Result<()> {
    let socket = socket_path(mail_dir, route);
    let upward = std::os::unix::net::UnixStream::connect(&socket).with_context(|| {
        format!(
            "route `{route}` has no live acp host on {}",
            socket.display()
        )
    })?;
    let downward = upward.try_clone().context("clone the acp host socket")?;
    let pump = std::thread::Builder::new()
        .name("boop-acp-attach".to_owned())
        .spawn(move || {
            let mut socket = downward;
            let _ = std::io::copy(&mut socket, &mut std::io::stdout());
        })
        .context("spawn the acp attach pump")?;
    let mut socket = upward;
    let _ = std::io::copy(&mut std::io::stdin(), &mut socket);
    let _ = socket.shutdown(std::net::Shutdown::Write);
    let _ = pump.join();
    Ok(())
}

/// Every unacked row addressed to `route` whose id this host has not already
/// turned into a prompt.
fn pending(dir: &Path, route: &str, seen: &BTreeSet<String>) -> Vec<bus::Message> {
    let mut rows = Vec::new();
    for path in bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(bus::parse_box(&path));
    }
    bus::unacked(&rows)
        .into_iter()
        .filter(|row| row.to == route)
        .filter(|row| !seen.contains(&row.id))
        .filter(|row| DELIVERABLE.contains(&row.kind.as_str()))
        .collect()
}

/// Stamp the row delivered so no later read, in this host or the next one,
/// re-offers it.
fn ack(dir: &Path, row: &bus::Message) {
    let mut rows = Vec::new();
    for path in bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(bus::parse_box(&path));
    }
    let Some(mut acked) = bus::fold(&rows).into_iter().find(|held| held.id == row.id) else {
        return;
    };
    acked.to_timestamp = Some(bus::now_iso());
    let line = bus::message_line(&acked);
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("bus.ndjson"))
    else {
        return;
    };
    use std::io::Write;
    if let Err(error) = writeln!(file, "{line}") {
        error!(message_id = row.id, error = %error, "acp host ack write failed");
    }
}

/// The template mail addressed to this route renders through, so a hosted turn
/// reads exactly as the same row would in a lane pane.
fn mood_template(route: &str) -> String {
    boop_store::Store::default_path()
        .and_then(boop_store::Store::open)
        .and_then(|store| store.effective_mood(route))
        .map(|mood| mood.template)
        .unwrap_or_else(|error| {
            warn!(route, error = %error, "effective mood unresolved");
            boop_store::ident::DEFAULT_MOOD_TEMPLATE.to_owned()
        })
}

fn render_mail(template: &str, row: &bus::Message) -> String {
    template
        .replace("{kind}", &row.kind)
        .replace("{id}", &row.id)
        .replace("{from}", &row.from)
        .replace("{body}", &row.body)
}

/// Write the ACP session id onto the route so the next host resumes it.
fn record_session(dir: &Path, route: &str, session: &str) {
    let path = dir.join("registry.json");
    let route = route.to_owned();
    let session = session.to_owned();
    if let Err(error) = bus::cas_update_json(&path, |map| {
        let entry = map
            .entry(route.clone())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if let Some(object) = entry.as_object_mut() {
            object.insert(
                "sessionId".into(),
                serde_json::Value::String(session.clone()),
            );
        }
        Ok(())
    }) {
        warn!(route, conversation_id = session, error = %error, "acp host route update failed");
    }
}

/// The allow option an unattended permission request is answered with.
fn allow_option(
    request: &RequestPermissionRequest,
) -> Option<agent_client_protocol::schema::v1::PermissionOptionId> {
    request
        .options
        .iter()
        .find(|option| matches!(option.kind, PermissionOptionKind::AllowAlways))
        .or_else(|| {
            request
                .options
                .iter()
                .find(|option| matches!(option.kind, PermissionOptionKind::AllowOnce))
        })
        .map(|option| option.option_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_sits_beside_the_mail_dir() {
        let socket = socket_path(Path::new("/tmp/agent/mail"), "coord");
        assert_eq!(socket, Path::new("/tmp/agent/acp/coord.sock"));
    }

    #[test]
    fn a_route_with_a_slash_stays_one_path_component() {
        let socket = socket_path(Path::new("/tmp/agent/mail"), "feature/x");
        assert_eq!(socket, Path::new("/tmp/agent/acp/feature_x.sock"));
    }

    #[test]
    fn a_dead_route_is_not_alive() {
        assert!(!route_host_alive(
            Path::new("/tmp/boop-acp-nothing-here/mail"),
            "nobody"
        ));
    }

    /// A bind that would exceed `sun_path` names the limit rather than letting
    /// the kernel answer with a pathless ENAMETOOLONG.
    #[test]
    fn an_overlong_socket_path_is_named_not_bound() {
        let long = PathBuf::from(format!("/tmp/{}/x.sock", "n".repeat(SUN_PATH_MAX)));
        let error = bind_exclusive(&long, "long").unwrap_err().to_string();
        assert!(error.contains("takes 103"), "{error}");
    }

    #[test]
    fn mail_renders_through_the_route_mood() {
        let row = bus::Message {
            id: "m1".into(),
            from: "coordinator".into(),
            to: "coord".into(),
            from_timestamp: String::new(),
            to_timestamp: None,
            kind: "hail".into(),
            reply_to: None,
            body: "stop".into(),
            r#ref: None,
            rc: None,
            detail: None,
        };
        assert_eq!(
            render_mail(boop_store::ident::DEFAULT_MOOD_TEMPLATE, &row),
            "[boop m1 from coordinator] stop"
        );
    }
}
