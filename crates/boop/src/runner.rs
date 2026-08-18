//! `boop run <program.dl6>`: the resident coroutine as one rx operator.
//!
//! ```text
//! session_turns$ -> engine(turn) -> resident_ask deltas
//!   -> concatMap(resident.ask) -> engine(resident)
//! ```
//!
//! The compiled program holds every rule; this file holds the two arrivals it
//! cannot make for itself. Source turns come out of `~/.agent/boop.db` into rel
//! `turn`. Each `resident_ask` row goes to one live chat, one at a time, and
//! the reply comes back as a `resident` row. "Answered" is a query against
//! `GET /rel/resident`, so a restart re-reads it and asks nothing twice.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;

use crate::channel::{ChannelSpec, LaneChannel};
use crate::harness::Harness;
use crate::ident::TurnQuery;
use crate::registry::Registry;
use crate::rows::TurnRow;

/// The demand rel the runner follows and the response rel it writes.
pub const ASK_REL: &str = "resident_ask";
pub const REPLY_REL: &str = "resident";
/// The source-turn rel the runner fills.
pub const TURN_REL: &str = "turn";

/// A `GET /rel/<name>` or `POST /arrive` is a local socket round trip.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// The deltas route long-polls, so its budget is the wait, not the work.
const DELTA_TIMEOUT: Duration = Duration::from_secs(120);
/// Compiling and booting the program before the first health answer.
const BOOT_BUDGET: Duration = Duration::from_secs(120);
const BOOT_POLL: Duration = Duration::from_millis(200);
/// A finished turn reaches the transcript, and the store, after the harness
/// flushes; this is how long the reply read waits for it.
const REPLY_BUDGET: Duration = Duration::from_secs(120);
const REPLY_POLL: Duration = Duration::from_secs(1);

// ---------------------------------------------------------------- wire types

/// One arrival as `POST /arrive` takes it: the engine's `ArrivalDto`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Arrival {
    pub rel: String,
    pub sign: String,
    pub row: Vec<serde_json::Value>,
}

impl Arrival {
    pub fn add(rel: &str, row: Vec<serde_json::Value>) -> Arrival {
        Arrival {
            rel: rel.to_owned(),
            sign: "add".to_owned(),
            row,
        }
    }
}

/// One `resident_ask(session, user_run, prompt)` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    pub session: String,
    pub user_run: i64,
    pub prompt: String,
}

impl Ask {
    pub fn key(&self) -> (String, i64) {
        (self.session.clone(), self.user_run)
    }
}

/// What one resident turn produced: its own turn number and its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResidentReply {
    pub turn: i64,
    pub text: String,
}

/// `resident(session, user_run, reply_turn, reply)`, the rel's column order.
pub fn reply_arrival(ask: &Ask, reply: &ResidentReply) -> Arrival {
    Arrival::add(
        REPLY_REL,
        vec![
            serde_json::json!(ask.session),
            serde_json::json!(ask.user_run),
            serde_json::json!(reply.turn),
            serde_json::json!(reply.text),
        ],
    )
}

/// `turn(session, turn, ts, role, said)`, the rel's column order.
pub fn turn_arrivals(rows: &[TurnRow]) -> Vec<Arrival> {
    rows.iter()
        .map(|row| {
            Arrival::add(
                TURN_REL,
                vec![
                    serde_json::json!(row.session),
                    serde_json::json!(row.turn),
                    serde_json::json!(row.ts),
                    serde_json::json!(row.role),
                    serde_json::json!(crate::concatmap::trim_double_encoded(&row.said)),
                ],
            )
        })
        .collect()
}

/// A row as either door spells it: the rel read answers objects keyed by
/// column name, the deltas route answers positional rows.
fn field<'a>(row: &'a serde_json::Value, name: &str, at: usize) -> Option<&'a serde_json::Value> {
    match row {
        serde_json::Value::Object(map) => map.get(name),
        serde_json::Value::Array(items) => items.get(at),
        _ => None,
    }
}

fn text_field(row: &serde_json::Value, name: &str, at: usize) -> Result<String> {
    let value = field(row, name, at).with_context(|| format!("row has no {name}: {row}"))?;
    match value {
        serde_json::Value::String(text) => Ok(text.clone()),
        other => Ok(other.to_string()),
    }
}

fn int_field(row: &serde_json::Value, name: &str, at: usize) -> Result<i64> {
    let value = field(row, name, at).with_context(|| format!("row has no {name}: {row}"))?;
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .with_context(|| format!("{name} is not an integer: {value}"))
}

pub fn ask_from_json(row: &serde_json::Value) -> Result<Ask> {
    Ok(Ask {
        session: text_field(row, "session", 0)?,
        user_run: int_field(row, "user_run", 1)?,
        prompt: text_field(row, "prompt", 2)?,
    })
}

/// The `(session, user_run)` half of a `resident` row: which ask it answered.
pub fn answered_key(row: &serde_json::Value) -> Result<(String, i64)> {
    Ok((
        text_field(row, "session", 0)?,
        int_field(row, "user_run", 1)?,
    ))
}

// ------------------------------------------------------------- engine client

/// One HTTP answer off the socket file.
#[derive(Clone, Debug)]
pub struct Answer {
    pub status: u32,
    pub body: Vec<u8>,
}

impl Answer {
    pub fn json(&self) -> Result<serde_json::Value> {
        serde_json::from_slice(&self.body).with_context(|| {
            format!(
                "decode the engine answer: {}",
                String::from_utf8_lossy(&self.body)
            )
        })
    }
}

/// The compiled program's HTTP surface on its socket file. One trait so the
/// tests can drive the loop without a socket, and so the client library is
/// named in exactly one place.
pub trait EngineClient {
    fn get(&self, path: &str, timeout: Duration) -> Result<Answer>;
    fn post(&self, path: &str, body: &[u8], timeout: Duration) -> Result<Answer>;
}

/// libcurl over `CURLOPT_UNIX_SOCKET_PATH`. The url host is ignored once the
/// socket path is set; it only has to parse.
pub struct UdsClient {
    socket: PathBuf,
}

impl UdsClient {
    pub fn new(socket: PathBuf) -> UdsClient {
        UdsClient { socket }
    }

    fn call(&self, path: &str, body: Option<&[u8]>, timeout: Duration) -> Result<Answer> {
        let mut easy = curl::easy::Easy::new();
        easy.unix_socket_path(Some(&self.socket))
            .with_context(|| format!("target the socket {}", self.socket.display()))?;
        easy.url(&format!("http://localhost{path}"))
            .with_context(|| format!("set the url {path}"))?;
        easy.timeout(timeout).context("set the call timeout")?;
        let mut headers = curl::easy::List::new();
        headers.append("Content-Type: application/json")?;
        // Without this curl waits for a 100 Continue on any body past 1KB.
        headers.append("Expect:")?;
        easy.http_headers(headers).context("set the headers")?;
        if let Some(payload) = body {
            easy.post(true).context("select POST")?;
            easy.post_fields_copy(payload).context("set the body")?;
        }
        let mut out = Vec::new();
        {
            let mut transfer = easy.transfer();
            transfer.write_function(|chunk| {
                out.extend_from_slice(chunk);
                Ok(chunk.len())
            })?;
            transfer
                .perform()
                .with_context(|| format!("call {path} on {}", self.socket.display()))?;
        }
        Ok(Answer {
            status: easy.response_code().context("read the status")?,
            body: out,
        })
    }
}

impl EngineClient for UdsClient {
    fn get(&self, path: &str, timeout: Duration) -> Result<Answer> {
        self.call(path, None, timeout)
    }

    fn post(&self, path: &str, body: &[u8], timeout: Duration) -> Result<Answer> {
        self.call(path, Some(body), timeout)
    }
}

/// Fold a batch of arrivals; the engine answers the tick's deltas.
pub fn arrive(client: &dyn EngineClient, arrivals: &[Arrival]) -> Result<serde_json::Value> {
    let body = serde_json::to_vec(arrivals).context("encode the arrival batch")?;
    let answer = client.post("/arrive", &body, CALL_TIMEOUT)?;
    ensure!(
        answer.status == 200,
        "POST /arrive answered {}: {}",
        answer.status,
        String::from_utf8_lossy(&answer.body)
    );
    answer.json()
}

/// Every row of one rel, as the rel read spells them.
pub fn read_rel(client: &dyn EngineClient, rel: &str) -> Result<Vec<serde_json::Value>> {
    let answer = client.get(&format!("/rel/{rel}"), CALL_TIMEOUT)?;
    ensure!(
        answer.status == 200,
        "GET /rel/{rel} answered {}: {}",
        answer.status,
        String::from_utf8_lossy(&answer.body)
    );
    let body = answer.json()?;
    Ok(body
        .get("rows")
        .and_then(|rows| rows.as_array())
        .cloned()
        .unwrap_or_default())
}

/// The asks already carrying a `resident` row. Read at boot; a restart reads
/// the same set, which is the whole of the no-re-ask rule.
pub fn read_answered(client: &dyn EngineClient) -> Result<BTreeSet<(String, i64)>> {
    read_rel(client, REPLY_REL)?
        .iter()
        .map(answered_key)
        .collect()
}

// ---------------------------------------------------------------- ask feed

/// The demand feed. One method, so swapping the deltas route for the whole-rel
/// read is one impl and nothing in the loop moves.
pub trait AskFeed {
    /// The asks the engine has now. Rows may repeat; the caller filters.
    fn poll(&mut self) -> Result<Vec<Ask>>;
}

/// `GET /rel/<rel>/deltas?since=<tick>`, long-poll. A `del` needs no handling:
/// a reply already posted cannot be unsent, and the response rel keeps it.
pub struct DeltaRoute<'a> {
    client: &'a dyn EngineClient,
    rel: String,
    since: u64,
    pending: Vec<Ask>,
}

impl<'a> DeltaRoute<'a> {
    pub fn new(client: &'a dyn EngineClient, rel: &str, since: u64) -> DeltaRoute<'a> {
        DeltaRoute {
            client,
            rel: rel.to_owned(),
            since,
            pending: Vec::new(),
        }
    }
}

impl AskFeed for DeltaRoute<'_> {
    fn poll(&mut self) -> Result<Vec<Ask>> {
        if !self.pending.is_empty() {
            return Ok(std::mem::take(&mut self.pending));
        }
        let path = format!("/rel/{}/deltas?since={}", self.rel, self.since);
        let answer = self.client.get(&path, DELTA_TIMEOUT)?;
        ensure!(
            answer.status == 200,
            "GET {path} answered {}: {}",
            answer.status,
            String::from_utf8_lossy(&answer.body)
        );
        let body = answer.json()?;
        if let Some(tick) = body.get("tick").and_then(|tick| tick.as_u64()) {
            self.since = tick;
        }
        body.get("add")
            .and_then(|add| add.as_array())
            .map(|rows| rows.iter().map(ask_from_json).collect())
            .unwrap_or_else(|| Ok(Vec::new()))
    }
}

/// The whole rel every pass, for an engine with no deltas route. Correct for
/// the same reason a restart is: the answered set is the filter.
pub struct RelScan<'a> {
    client: &'a dyn EngineClient,
    rel: String,
}

impl<'a> RelScan<'a> {
    pub fn new(client: &'a dyn EngineClient, rel: &str) -> RelScan<'a> {
        RelScan {
            client,
            rel: rel.to_owned(),
        }
    }
}

impl AskFeed for RelScan<'_> {
    fn poll(&mut self) -> Result<Vec<Ask>> {
        read_rel(self.client, &self.rel)?
            .iter()
            .map(ask_from_json)
            .collect()
    }
}

/// Take the deltas route where the engine has one, the whole-rel read where it
/// answers 404.
pub fn open_feed(client: &dyn EngineClient) -> Result<Box<dyn AskFeed + '_>> {
    let probe = client.get(&format!("/rel/{ASK_REL}/deltas?since=0"), CALL_TIMEOUT)?;
    if probe.status == 200 {
        tracing::info!(rel = ASK_REL, "following the deltas route");
        let body = probe.json()?;
        let since = body.get("tick").and_then(|tick| tick.as_u64()).unwrap_or(0);
        let pending = body
            .get("add")
            .and_then(|add| add.as_array())
            .map(|rows| rows.iter().map(ask_from_json).collect())
            .unwrap_or_else(|| Ok(Vec::new()))?;
        return Ok(Box::new(DeltaRoute {
            client,
            rel: ASK_REL.to_owned(),
            since,
            pending,
        }));
    }
    tracing::info!(
        rel = ASK_REL,
        status = probe.status,
        "no deltas route; reading the whole rel each pass"
    );
    Ok(Box::new(RelScan::new(client, ASK_REL)))
}

// ---------------------------------------------------------------- resident

/// The resident: one live chat, one turn at a time.
pub trait Resident {
    fn ask(&mut self, prompt: &str) -> Result<ResidentReply>;
}

/// One `Rewriter::Chat`-shaped channel plus the store read that recovers what
/// it said. A channel reports only that its turn ended, so the reply text is
/// read back out of the resident's own transcript.
pub struct ChatResident {
    adapter: &'static dyn Harness,
    spec: ChannelSpec,
    channel: Box<dyn LaneChannel>,
    pending_goal: Option<String>,
    compact_tokens: usize,
    /// Read-write: this ingests the resident's own transcript rather than
    /// waiting on whatever else may be syncing the store.
    store: crate::Store,
    /// The newest resident turn already claimed as a reply; `None` before the
    /// conversation exists.
    seen_turn: Option<i64>,
}

impl ChatResident {
    pub fn open(
        registry: &'static Registry,
        model: &str,
        cwd: &Path,
        goal: Option<String>,
        compact_tokens: usize,
        store: crate::Store,
    ) -> Result<ChatResident> {
        let harness = crate::lane::harness_for_model(model)?
            .with_context(|| format!("model `{model}` names no harness"))?;
        let adapter = registry
            .by_id(&harness)
            .with_context(|| format!("no adapter registered for harness `{harness}`"))?;
        let spec = ChannelSpec {
            model: Some(model.to_owned()),
            cwd: cwd.to_path_buf(),
            resume: None,
            lane: None,
        };
        let channel = adapter
            .open_channel(&spec)
            .context("open the resident chat")?;
        Ok(ChatResident {
            adapter,
            spec,
            channel,
            pending_goal: goal,
            compact_tokens,
            store,
            seen_turn: None,
        })
    }

    fn conversation(&self) -> Result<String> {
        self.channel
            .conversation_id()
            .context("the resident chat has not named its conversation yet")
    }

    /// Project the resident's transcript into the store. A conversation the
    /// harness has not written yet is not an error; the reply read retries.
    fn ingest(&self) -> Result<()> {
        let conversation = self.conversation()?;
        let sessions = self
            .adapter
            .sessions()
            .context("list the resident harness sessions")?;
        let Some(session) = sessions
            .into_iter()
            .find(|candidate| candidate.session_id == conversation)
        else {
            return Ok(());
        };
        crate::ident::sync_session(&self.store, self.adapter, &session)
            .map(|_| ())
            .context("ingest the resident transcript")
    }

    fn newest_assistant(&self) -> Result<Option<TurnRow>> {
        let conversation = self.conversation()?;
        let rows = self.store.turn_rows(&TurnQuery {
            session: Some(conversation),
            role: Some("assistant".to_owned()),
            turn_from: self.seen_turn.map(|turn| turn as u64 + 1),
            ..Default::default()
        })?;
        Ok(rows.into_iter().rfind(|row| !row.said.trim().is_empty()))
    }

    /// Wait for the turn just finished to reach the store, then take it.
    fn pull_reply(&mut self) -> Result<ResidentReply> {
        let deadline = Instant::now() + REPLY_BUDGET;
        loop {
            self.ingest()?;
            if let Some(row) = self.newest_assistant()? {
                self.seen_turn = Some(row.turn);
                return Ok(ResidentReply {
                    turn: row.turn,
                    text: crate::concatmap::trim_double_encoded(&row.said).to_owned(),
                });
            }
            if Instant::now() >= deadline {
                bail!(
                    "the resident's reply never reached the store within {}s",
                    REPLY_BUDGET.as_secs()
                );
            }
            std::thread::sleep(REPLY_POLL);
        }
    }

    /// The goal turn is the resident's opening instruction; its own answer is
    /// no ask's reply, so it only moves the high-water mark.
    fn send_goal(&mut self, goal: &str) -> Result<()> {
        self.channel
            .start_turn(goal)
            .context("send the resident's goal turn")?;
        crate::concatmap::wait_done(self.channel.as_mut())?;
        if let Ok(Some(row)) = self.newest_assistant() {
            self.seen_turn = Some(row.turn);
        }
        Ok(())
    }

    /// Context ceiling: restart the chat past the limit, with a resume goal.
    /// The artifact the resident owns already carries the folded history.
    fn compact_if_over(&mut self) -> Result<()> {
        if self.compact_tokens == 0 {
            return Ok(());
        }
        let context = self
            .channel
            .conversation_id()
            .and_then(|id| crate::concatmap::context_tokens(&self.store, &id))
            .unwrap_or(0);
        if context < self.compact_tokens as i64 {
            return Ok(());
        }
        self.channel.close().context("close the resident chat")?;
        self.channel = self.adapter.open_channel(&self.spec)?;
        self.pending_goal = Some(crate::concatmap::COMPACT_RESUME_GOAL.to_owned());
        self.seen_turn = None;
        Ok(())
    }
}

impl Resident for ChatResident {
    fn ask(&mut self, prompt: &str) -> Result<ResidentReply> {
        if let Some(goal) = self.pending_goal.take() {
            self.send_goal(&goal)?;
        }
        self.channel.start_turn(prompt).context("send the ask")?;
        crate::concatmap::wait_done(self.channel.as_mut())?;
        let reply = self.pull_reply()?;
        self.compact_if_over()?;
        Ok(reply)
    }
}

// ------------------------------------------------------------- engine process

/// The compiled program serving its rels on a socket file, killed on drop.
pub struct EngineProcess {
    child: Child,
    socket: PathBuf,
}

impl EngineProcess {
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// Compile `program`, build the sprefa harness binary, and boot it on
    /// `dir/engine.sock` with an empty schedule; every arrival then comes over
    /// the socket.
    pub fn boot(program: &Path, sprefa_root: &Path, dir: &Path) -> Result<EngineProcess> {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        let module = dir.join("program.rs");
        compile_program(program, sprefa_root, &module)?;
        let harness = build_harness(sprefa_root)?;
        let schedule = dir.join("schedule.json");
        std::fs::write(&schedule, "[]").with_context(|| format!("write {}", schedule.display()))?;
        let socket = dir.join("engine.sock");
        let _ = std::fs::remove_file(&socket);
        let log_path = dir.join("engine.log");
        let log = std::fs::File::create(&log_path)
            .with_context(|| format!("create {}", log_path.display()))?;
        let child = Command::new(&harness)
            .arg(&module)
            .arg(&schedule)
            .arg("--socket")
            .arg(&socket)
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                log.try_clone().context("clone the engine log")?,
            ))
            .stderr(Stdio::from(log))
            .spawn()
            .with_context(|| format!("spawn {}", harness.display()))?;
        let mut engine = EngineProcess { child, socket };
        engine.wait_healthy(&log_path)?;
        Ok(engine)
    }

    fn wait_healthy(&mut self, log_path: &Path) -> Result<()> {
        let client = UdsClient::new(self.socket.clone());
        let deadline = Instant::now() + BOOT_BUDGET;
        loop {
            if let Some(status) = self.child.try_wait().context("poll the engine child")? {
                bail!(
                    "the engine exited before serving ({status}); {}",
                    log_tail(log_path)
                );
            }
            if let Ok(answer) = client.get("/health", CALL_TIMEOUT) {
                if answer.status == 200 {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                bail!(
                    "the engine never answered /health on {} within {}s; {}",
                    self.socket.display(),
                    BOOT_BUDGET.as_secs(),
                    log_tail(log_path)
                );
            }
            std::thread::sleep(BOOT_POLL);
        }
    }
}

impl Drop for EngineProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn log_tail(path: &Path) -> String {
    let mut text = String::new();
    if let Ok(mut file) = std::fs::File::open(path) {
        let _ = file.read_to_string(&mut text);
    }
    let tail: Vec<&str> = text.lines().rev().take(10).collect();
    format!(
        "last engine output: {}",
        tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
    )
}

/// A prolog goal carries the two paths as quoted atoms, so a quote in either
/// one would end the atom rather than travel in it.
fn atom(path: &Path) -> Result<String> {
    let text = path.display().to_string();
    ensure!(!text.contains('\''), "path carries a quote: {text}");
    Ok(text)
}

pub fn compile_program(program: &Path, sprefa_root: &Path, out: &Path) -> Result<()> {
    let goal = format!(
        "compile_dl6('{}', '{}', [emitter(emit_rust:emit_program)])",
        atom(program)?,
        atom(out)?
    );
    let status = Command::new("swipl")
        .arg("-q")
        .arg("-l")
        .arg(sprefa_root.join("v6/prolog/compile.pl"))
        .arg("-l")
        .arg(sprefa_root.join("v6/prolog/emit_rust.pl"))
        .arg("-g")
        .arg(&goal)
        .arg("-g")
        .arg("halt")
        .status()
        .context("run swipl; the dl6 compiler needs it on PATH")?;
    ensure!(status.success(), "compiling {} stopped", program.display());
    ensure!(
        out.exists(),
        "compiling {} wrote no module at {}",
        program.display(),
        out.display()
    );
    Ok(())
}

pub fn build_harness(sprefa_root: &Path) -> Result<PathBuf> {
    let manifest = sprefa_root.join("v6/sprefa-engine-rs/Cargo.toml");
    ensure!(
        manifest.exists(),
        "no sprefa engine at {}; name the checkout with SPREFA_ROOT",
        manifest.display()
    );
    let status = Command::new("cargo")
        .arg("build")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--bin")
        .arg("emit_rust_harness")
        .status()
        .context("run cargo to build the sprefa engine harness")?;
    ensure!(status.success(), "building emit_rust_harness stopped");
    Ok(sprefa_root.join("v6/sprefa-engine-rs/target/debug/emit_rust_harness"))
}

// ------------------------------------------------------------------ the loop

/// One run, one row per CLI flag.
#[derive(Clone, Debug)]
pub struct Args {
    pub program: PathBuf,
    /// The source session whose turns fill rel `turn`.
    pub session: String,
    /// The resident model, in the harness's own flag spelling.
    pub model: String,
    /// The resident's opening turn.
    pub goal: Option<String>,
    pub poll: Duration,
    /// Where the compiled module, schedule, socket and log live.
    pub run_dir: PathBuf,
    /// The sprefa checkout holding the compiler and the engine harness.
    pub sprefa_root: PathBuf,
    /// The boop store to read source turns from; `None` takes the default.
    pub store_path: Option<PathBuf>,
    /// The resident chat's context ceiling in tokens; 0 never compacts.
    pub compact_tokens: usize,
}

/// `SPREFA_ROOT`, or the standing checkout path.
pub fn default_sprefa_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("SPREFA_ROOT").filter(|root| !root.is_empty()) {
        return Ok(PathBuf::from(root));
    }
    let home = dirs::home_dir().context("resolve home directory")?;
    Ok(home.join("projects").join("sprefa"))
}

/// `~/.agent/run/<name>`.
pub fn default_run_dir(name: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolve home directory")?;
    Ok(home.join(".agent").join("run").join(name))
}

/// Source turns the engine has not seen, as `turn` arrivals. `from_turn` is
/// the highest turn already pushed; turn numbers are unique inside a session,
/// so this never re-pushes and never skips a same-millisecond pair.
pub fn push_turns(
    client: &dyn EngineClient,
    store: &crate::Store,
    session: &str,
    from_turn: &mut Option<i64>,
) -> Result<usize> {
    let rows = store
        .turn_rows(&TurnQuery {
            session: Some(session.to_owned()),
            turn_from: from_turn.map(|turn| turn as u64 + 1),
            ..Default::default()
        })
        .context("query the source session's turns")?;
    if rows.is_empty() {
        return Ok(0);
    }
    let arrivals = turn_arrivals(&rows);
    arrive(client, &arrivals)?;
    if let Some(highest) = rows.iter().map(|row| row.turn).max() {
        *from_turn = Some(highest);
    }
    Ok(arrivals.len())
}

/// Asks with no `resident` row yet, in `user_run` order. Serial answering is
/// this sort plus the caller's loop, and nothing else.
pub fn unanswered(asks: Vec<Ask>, answered: &BTreeSet<(String, i64)>) -> Vec<Ask> {
    let mut pending: Vec<Ask> = asks
        .into_iter()
        .filter(|ask| !answered.contains(&ask.key()))
        .collect();
    pending.sort_by(|left, right| {
        left.session
            .cmp(&right.session)
            .then(left.user_run.cmp(&right.user_run))
    });
    pending.dedup_by(|left, right| left.key() == right.key());
    pending
}

/// One pass of the operator: take the batch, answer each ask in order, post
/// each reply before the next ask goes out.
pub fn answer_pending(
    client: &dyn EngineClient,
    feed: &mut dyn AskFeed,
    resident: &mut dyn Resident,
    answered: &mut BTreeSet<(String, i64)>,
) -> Result<usize> {
    let pending = unanswered(feed.poll()?, answered);
    let mut count = 0;
    for ask in pending {
        let reply = resident
            .ask(&ask.prompt)
            .with_context(|| format!("resident ask {}/{}", ask.session, ask.user_run))?;
        arrive(client, &[reply_arrival(&ask, &reply)])?;
        answered.insert(ask.key());
        count += 1;
    }
    Ok(count)
}

pub fn run(args: Args) -> Result<()> {
    let engine = EngineProcess::boot(&args.program, &args.sprefa_root, &args.run_dir)?;
    let client = UdsClient::new(engine.socket().to_path_buf());
    let store_path = match &args.store_path {
        Some(path) => path.clone(),
        None => crate::ident::Store::default_path().context("resolve the default boop store")?,
    };
    // Read-only for the source session, so this never fights whatever writes
    // the store; read-write for the resident, whose own transcript it ingests.
    let source = crate::ident::Store::open_readonly(store_path.clone())
        .context("open boop store read-only")?;
    let resident_store = crate::ident::Store::open(store_path).context("open boop store")?;
    // Leaked so the adapter is 'static, as the concatmap loop does: one small
    // fixed allocation in a process that does not return.
    let registry: &'static Registry = Box::leak(Box::new(Registry::discover()));
    let mut resident = ChatResident::open(
        registry,
        &args.model,
        &args.run_dir,
        args.goal.clone(),
        args.compact_tokens,
        resident_store,
    )?;
    let mut feed = open_feed(&client)?;
    let mut answered = read_answered(&client)?;
    let mut from_turn: Option<i64> = None;
    tracing::info!(
        program = %args.program.display(),
        socket = %engine.socket().display(),
        session = args.session,
        already_answered = answered.len(),
        "resident coroutine up"
    );
    loop {
        match push_turns(&client, &source, &args.session, &mut from_turn) {
            Ok(0) => {}
            Ok(pushed) => tracing::info!(pushed, "source turns arrived"),
            Err(error) => {
                tracing::warn!(error = %format!("{error:#}"), "pushing source turns failed")
            }
        }
        match answer_pending(&client, feed.as_mut(), &mut resident, &mut answered) {
            Ok(0) => {}
            Ok(answered_now) => tracing::info!(answered_now, "asks answered"),
            Err(error) => tracing::warn!(error = %format!("{error:#}"), "answering stopped"),
        }
        std::thread::sleep(args.poll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn ask(session: &str, user_run: i64) -> Ask {
        Ask {
            session: session.to_owned(),
            user_run,
            prompt: format!("prompt {user_run}"),
        }
    }

    /// A feed that hands back one canned batch per pass.
    struct FakeFeed {
        batches: Vec<Vec<Ask>>,
    }

    impl AskFeed for FakeFeed {
        fn poll(&mut self) -> Result<Vec<Ask>> {
            if self.batches.is_empty() {
                return Ok(Vec::new());
            }
            Ok(self.batches.remove(0))
        }
    }

    /// Echoes each prompt back and numbers its own turns.
    struct FakeResident {
        seen: Rc<RefCell<Vec<String>>>,
        turn: i64,
    }

    impl Resident for FakeResident {
        fn ask(&mut self, prompt: &str) -> Result<ResidentReply> {
            self.seen.borrow_mut().push(prompt.to_owned());
            self.turn += 1;
            Ok(ResidentReply {
                turn: self.turn,
                text: format!("reply to {prompt}"),
            })
        }
    }

    /// Records every arrival and answers `/rel/resident` from them.
    #[derive(Default)]
    struct FakeClient {
        posted: RefCell<Vec<Arrival>>,
    }

    impl EngineClient for FakeClient {
        fn get(&self, path: &str, _timeout: Duration) -> Result<Answer> {
            let rows: Vec<serde_json::Value> = self
                .posted
                .borrow()
                .iter()
                .filter(|arrival| path == format!("/rel/{}", arrival.rel))
                .map(|arrival| serde_json::Value::Array(arrival.row.clone()))
                .collect();
            Ok(Answer {
                status: 200,
                body: serde_json::to_vec(&serde_json::json!({ "rows": rows }))?,
            })
        }

        fn post(&self, path: &str, body: &[u8], _timeout: Duration) -> Result<Answer> {
            assert_eq!(path, "/arrive");
            let batch: Vec<serde_json::Value> = serde_json::from_slice(body)?;
            for item in batch {
                self.posted.borrow_mut().push(Arrival {
                    rel: item["rel"].as_str().unwrap_or_default().to_owned(),
                    sign: item["sign"].as_str().unwrap_or_default().to_owned(),
                    row: item["row"].as_array().cloned().unwrap_or_default(),
                });
            }
            Ok(Answer {
                status: 200,
                body: b"{}".to_vec(),
            })
        }
    }

    // Safe here: every test drives one thread.
    unsafe impl Send for FakeClient {}
    unsafe impl Send for FakeFeed {}
    unsafe impl Send for FakeResident {}

    #[test]
    fn a_batch_is_answered_in_user_run_order() {
        let client = FakeClient::default();
        let mut feed = FakeFeed {
            batches: vec![vec![ask("s", 7), ask("s", 2), ask("s", 5)]],
        };
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut resident = FakeResident {
            seen: seen.clone(),
            turn: 0,
        };
        let mut answered = BTreeSet::new();
        let count = answer_pending(&client, &mut feed, &mut resident, &mut answered).unwrap();
        assert_eq!(count, 3);
        assert_eq!(
            *seen.borrow(),
            vec!["prompt 2", "prompt 5", "prompt 7"],
            "the resident sees one batch in user_run order"
        );
        let replies: Vec<i64> = client
            .posted
            .borrow()
            .iter()
            .filter(|arrival| arrival.rel == REPLY_REL)
            .map(|arrival| arrival.row[1].as_i64().unwrap())
            .collect();
        assert_eq!(replies, vec![2, 5, 7], "one resident row per ask, in order");
    }

    #[test]
    fn an_answered_ask_is_never_asked_again() {
        let client = FakeClient::default();
        let mut feed = FakeFeed {
            batches: vec![
                vec![ask("s", 1), ask("s", 2)],
                vec![ask("s", 2), ask("s", 3)],
            ],
        };
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut resident = FakeResident {
            seen: seen.clone(),
            turn: 0,
        };
        let mut answered = BTreeSet::new();
        answer_pending(&client, &mut feed, &mut resident, &mut answered).unwrap();
        answer_pending(&client, &mut feed, &mut resident, &mut answered).unwrap();
        assert_eq!(seen.borrow().len(), 3, "the repeated ask went out once");
        assert_eq!(
            client
                .posted
                .borrow()
                .iter()
                .filter(|arrival| arrival.rel == REPLY_REL)
                .count(),
            3
        );
    }

    #[test]
    fn a_restart_reads_the_answered_set_off_the_response_rel() {
        let client = FakeClient::default();
        let mut feed = FakeFeed {
            batches: vec![vec![ask("s", 1), ask("s", 2)]],
        };
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut resident = FakeResident {
            seen: seen.clone(),
            turn: 0,
        };
        let mut answered = BTreeSet::new();
        answer_pending(&client, &mut feed, &mut resident, &mut answered).unwrap();
        // Second boot: nothing in memory, the answered set comes off the rel.
        let mut restarted = read_answered(&client).unwrap();
        assert_eq!(restarted.len(), 2);
        let mut again = FakeFeed {
            batches: vec![vec![ask("s", 1), ask("s", 2)]],
        };
        let count = answer_pending(&client, &mut again, &mut resident, &mut restarted).unwrap();
        assert_eq!(count, 0, "a restart re-asks nothing");
        assert_eq!(seen.borrow().len(), 2);
    }

    #[test]
    fn duplicate_rows_in_one_batch_collapse() {
        let pending = unanswered(
            vec![ask("s", 4), ask("s", 4), ask("s", 1)],
            &BTreeSet::new(),
        );
        assert_eq!(
            pending.iter().map(|ask| ask.user_run).collect::<Vec<_>>(),
            vec![1, 4]
        );
    }

    #[test]
    fn a_row_reads_the_same_from_either_door() {
        let positional = serde_json::json!(["ses", 12, "say something"]);
        let keyed = serde_json::json!({
            "session": "ses", "user_run": 12, "prompt": "say something"
        });
        assert_eq!(
            ask_from_json(&positional).unwrap(),
            ask("ses", 12).clone_prompt("say something")
        );
        assert_eq!(
            ask_from_json(&keyed).unwrap(),
            ask_from_json(&positional).unwrap()
        );
        assert_eq!(answered_key(&keyed).unwrap(), ("ses".to_owned(), 12));
    }

    impl Ask {
        fn clone_prompt(mut self, prompt: &str) -> Ask {
            self.prompt = prompt.to_owned();
            self
        }
    }

    #[test]
    fn the_deltas_route_advances_its_since_by_the_answered_tick() {
        struct Route {
            asked: RefCell<Vec<String>>,
        }
        impl EngineClient for Route {
            fn get(&self, path: &str, _timeout: Duration) -> Result<Answer> {
                self.asked.borrow_mut().push(path.to_owned());
                Ok(Answer {
                    status: 200,
                    body: serde_json::to_vec(&serde_json::json!({
                        "tick": 9,
                        "add": [["ses", 3, "p"]],
                        "del": []
                    }))?,
                })
            }
            fn post(&self, _path: &str, _body: &[u8], _timeout: Duration) -> Result<Answer> {
                unreachable!("the feed never posts")
            }
        }
        unsafe impl Send for Route {}
        let route = Route {
            asked: RefCell::new(Vec::new()),
        };
        let mut feed = DeltaRoute::new(&route, ASK_REL, 0);
        assert_eq!(feed.poll().unwrap().len(), 1);
        feed.poll().unwrap();
        assert_eq!(
            *route.asked.borrow(),
            vec![
                "/rel/resident_ask/deltas?since=0",
                "/rel/resident_ask/deltas?since=9"
            ]
        );
    }

    #[test]
    fn a_missing_deltas_route_falls_back_to_the_whole_rel() {
        struct Absent;
        impl EngineClient for Absent {
            fn get(&self, path: &str, _timeout: Duration) -> Result<Answer> {
                if path.contains("/deltas") {
                    return Ok(Answer {
                        status: 404,
                        body: b"{}".to_vec(),
                    });
                }
                Ok(Answer {
                    status: 200,
                    body: serde_json::to_vec(&serde_json::json!({
                        "rows": [{ "session": "ses", "user_run": 1, "prompt": "p" }]
                    }))?,
                })
            }
            fn post(&self, _path: &str, _body: &[u8], _timeout: Duration) -> Result<Answer> {
                unreachable!("the feed never posts")
            }
        }
        unsafe impl Send for Absent {}
        let absent = Absent;
        let mut feed = open_feed(&absent).unwrap();
        assert_eq!(feed.poll().unwrap(), vec![ask("ses", 1).clone_prompt("p")]);
    }

    #[test]
    fn source_turns_carry_the_rels_column_order() {
        let rows = vec![TurnRow {
            session: "ses".to_owned(),
            harness: "claude".to_owned(),
            turn: 4,
            ts: 1700,
            role: "user".to_owned(),
            said: "\"hello\"".to_owned(),
        }];
        let arrivals = turn_arrivals(&rows);
        assert_eq!(arrivals.len(), 1);
        assert_eq!(arrivals[0].rel, TURN_REL);
        assert_eq!(arrivals[0].sign, "add");
        assert_eq!(
            arrivals[0].row,
            vec![
                serde_json::json!("ses"),
                serde_json::json!(4),
                serde_json::json!(1700),
                serde_json::json!("user"),
                serde_json::json!("hello"),
            ]
        );
    }

    #[test]
    fn a_reply_carries_the_response_rels_column_order() {
        let arrival = reply_arrival(
            &ask("ses", 12),
            &ResidentReply {
                turn: 3,
                text: "ok".to_owned(),
            },
        );
        assert_eq!(arrival.rel, REPLY_REL);
        assert_eq!(
            arrival.row,
            vec![
                serde_json::json!("ses"),
                serde_json::json!(12),
                serde_json::json!(3),
                serde_json::json!("ok"),
            ]
        );
    }
}
