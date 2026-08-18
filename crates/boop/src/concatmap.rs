//! The concatMap refinement loop: read new (assistant, user) contact pairs
//! from the store and pipe each through a one-shot model pass to fixed point.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::channel::{ChannelSpec, LaneChannel, TurnEvent};
use crate::harness::{Harness, OneShotSpec};
use crate::ident::TurnQuery;
use crate::registry::Registry;
use crate::rows::TurnRow;

/// A failed rewrite is retried this many times, then the pair is dropped.
const REWRITE_ATTEMPTS: u32 = 3;
const REWRITE_BACKOFF: Duration = Duration::from_secs(10);

/// Default resident-chat context ceiling in tokens; `compact_tokens: 0` in the
/// rules disables compaction, absent means this default.
const DEFAULT_COMPACT_TOKENS: usize = 100_000;

/// One contact pair: a user turn and the assistant turn that preceded it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub session: String,
    pub turn: i64,
    pub ts: i64,
    pub ai_text: String,
    pub user_text: String,
}

/// Loop configuration, one row per CLI flag.
#[derive(Clone)]
pub struct Args {
    pub template: Option<String>,
    pub mode: Option<String>,
    pub model: String,
    pub state_dir: PathBuf,
    /// The boop store to read turns from; `None` resolves the default path.
    pub store_path: Option<PathBuf>,
    pub poll: Duration,
    /// Seed the cursor at 0 so an existing conversation maps in full. Default
    /// remains tail-only; `cursor` names an explicit starting ts instead.
    pub from_start: bool,
    /// An explicit starting cursor ts (ms). Overrides the seed and the tail
    /// default; `from_start` is the `0` spelling of the same knob.
    pub cursor: Option<i64>,
    pub formula: Formula,
    /// One conversation only. The CLI refuses to leave it unset (`--session`
    /// or `--me`); `None` stays a library-internal spelling.
    pub session: Option<String>,
}

/// How a bundle reaches the model. The rules file names the feed; the loop
/// code never changes when the formula does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Feed {
    /// Boot a one-shot process per bundle; fixed-point passes to `cap`.
    OneShot,
    /// One resident chat; history is the accumulator (scan), and `goal`
    /// is the opening turn declaring how to handle every bundle.
    Chat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formula {
    pub feed: Feed,
    pub goal: Option<String>,
    pub bundle: BundleShape,
    /// Backlog cap; 0 never drops. Defaults to 0 (lossless); a caller opts
    /// into dropping by naming a cap in the rules.
    pub coalesce: usize,
    /// Append a <references> block of the source session's file touches as
    /// of each bundle, so reference claims come from the store, not the model.
    pub references: bool,
    /// Caller-owned window SQL; when present it replaces the compiled
    /// bundlers entirely (see `Store::window_rows` for the row contract).
    pub window: Option<String>,
    /// The resident chat's context ceiling in tokens. When its latest input
    /// reaches this, the feed restarts it with a compacted resume. `0`
    /// disables; absent in the rules means `DEFAULT_COMPACT_TOKENS`.
    pub compact_tokens: usize,
}

/// How consecutive same-role turns bundle. `Pair` takes the single preceding
/// assistant turn; `Run` collapses each same-role run into one block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BundleShape {
    Pair,
    Run,
}

impl Formula {
    pub fn oneshot() -> Formula {
        Formula {
            feed: Feed::OneShot,
            goal: None,
            bundle: BundleShape::Pair,
            coalesce: 0,
            references: false,
            window: None,
            compact_tokens: DEFAULT_COMPACT_TOKENS,
        }
    }

    /// Load from a json rules file.
    pub fn load(path: &Path) -> Result<Formula> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read rules {}", path.display()))?;
        let file: FormulaFile = serde_json::from_str(&expand_env(&text))
            .with_context(|| format!("parse rules {}", path.display()))?;
        let feed = match file.feed.as_str() {
            "oneshot" => Feed::OneShot,
            "chat" => Feed::Chat,
            other => bail!(
                "unknown feed `{other}` in {}; expected oneshot or chat",
                path.display()
            ),
        };
        let bundle = match file.bundle.as_deref() {
            None | Some("pair") => BundleShape::Pair,
            Some("run") => BundleShape::Run,
            Some(other) => bail!(
                "unknown bundle `{other}` in {}; expected pair or run",
                path.display()
            ),
        };
        Ok(Formula {
            feed,
            goal: file.goal,
            bundle,
            coalesce: file.coalesce.unwrap_or(0),
            references: file.references.unwrap_or(false),
            window: file.window,
            compact_tokens: file.compact_tokens.unwrap_or(DEFAULT_COMPACT_TOKENS),
        })
    }
}

#[derive(Deserialize)]
struct FormulaFile {
    feed: String,
    goal: Option<String>,
    bundle: Option<String>,
    coalesce: Option<usize>,
    references: Option<bool>,
    window: Option<String>,
    compact_tokens: Option<usize>,
}

/// The rewrite surface: one enum value per feed, chosen once at boot from the
/// formula. `rewrite` is the whole contract.
enum Rewriter {
    OneShot {
        adapter: &'static dyn Harness,
        model: String,
        timeout: Duration,
    },
    Chat {
        adapter: &'static dyn Harness,
        spec: ChannelSpec,
        channel: Box<dyn LaneChannel>,
        /// The next goal turn to send; taken once, then set again on compact.
        pending_goal: Option<String>,
        compact_tokens: usize,
    },
}

/// The goal a compacted resident receives: it re-reads the artifact it owns
/// (which already carries the folded history) and keeps going.
pub(crate) const COMPACT_RESUME_GOAL: &str =
    "Your context was compacted. Read seq.d2 first (it holds your folded history), then continue.";

/// Turn-done budget for the chat feed: each bundle blocks until the model's
/// reply completes, then the next bundle goes out.
const CHAT_TURN_TIMEOUT: Duration = Duration::from_secs(600);
const CHAT_POLL: Duration = Duration::from_secs(5);

impl Rewriter {
    fn open(formula: &Formula, adapter: &'static dyn Harness, args: &Args) -> Result<Rewriter> {
        match formula.feed {
            Feed::OneShot => Ok(Rewriter::OneShot {
                adapter,
                model: args.model.clone(),
                timeout: ONESHOT_TIMEOUT,
            }),
            Feed::Chat => {
                let spec = ChannelSpec {
                    model: Some(args.model.clone()),
                    cwd: args.state_dir.clone(),
                    resume: None,
                    lane: None,
                };
                let channel = adapter
                    .open_channel(&spec)
                    .context("open the resident chat")?;
                Ok(Rewriter::Chat {
                    adapter,
                    spec,
                    channel,
                    pending_goal: formula.goal.clone(),
                    compact_tokens: formula.compact_tokens,
                })
            }
        }
    }

    fn rewrite(&mut self, store: &crate::Store, msg: &str) -> Result<String> {
        match self {
            Rewriter::OneShot {
                adapter,
                model,
                timeout,
            } => one_shot_bounded(*adapter, msg, model, *timeout),
            Rewriter::Chat {
                adapter,
                spec,
                channel,
                pending_goal,
                compact_tokens,
            } => {
                let lane: &mut dyn LaneChannel = channel.as_mut();
                if let Some(goal) = pending_goal.take() {
                    lane.start_turn(&goal).context("send the goal turn")?;
                    wait_done(lane)?;
                }
                lane.start_turn(msg).context("send the bundle")?;
                wait_done(lane)?;
                // Context ceiling: when the resident's input has grown to the
                // limit, restart it so the long session cannot exhaust its
                // window. The artifact (seq.d2) already carries the history.
                if *compact_tokens > 0 {
                    let session = lane.conversation_id();
                    let over = session
                        .as_deref()
                        .and_then(|id| context_tokens(store, id))
                        .unwrap_or(0)
                        >= *compact_tokens as i64;
                    if over {
                        lane.close().context("close the resident chat")?;
                        *channel = adapter.open_channel(spec)?;
                        *pending_goal = Some(COMPACT_RESUME_GOAL.to_owned());
                    }
                }
                // The resident chat's own turns are the output; sync ingests
                // them, so there is no reply text to return.
                Ok(String::new())
            }
        }
    }
}

/// The resident chat's current context size in tokens: its latest turn's fresh
/// input plus the cached prior context, read from the store the sync ingests.
pub(crate) fn context_tokens(store: &crate::Store, session: &str) -> Option<i64> {
    const SQL: &str = "SELECT input_tokens + cache_read_tokens AS ctx FROM agent_usage
         JOIN dict_session s ON s.id = agent_usage.session_id
         WHERE s.value = ?1 ORDER BY ts DESC LIMIT 1";
    match store
        .connection()
        .query_row(SQL, rusqlite::params![session], |row| row.get::<_, i64>(0))
    {
        Ok(ctx) => Some(ctx),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(err) => {
            tracing::warn!(session, %err, "resident context_tokens query failed");
            None
        }
    }
}

/// Block until the running turn completes (the model's reply finished), so the
/// feed fires the next bundle only after the fold has consumed the last one.
pub(crate) fn wait_done(channel: &mut dyn LaneChannel) -> Result<()> {
    let deadline = Instant::now() + CHAT_TURN_TIMEOUT;
    while Instant::now() < deadline {
        match channel.next_event(CHAT_POLL)? {
            Some(TurnEvent::Done { .. }) => return Ok(()),
            Some(TurnEvent::Flaked { detail }) => {
                channel.interrupt()?;
                bail!("resident chat turn flaked: {detail}");
            }
            Some(TurnEvent::Failed { detail }) => {
                bail!("resident chat turn failed: {detail}");
            }
            Some(TurnEvent::Started) | None => continue,
        }
    }
    bail!(
        "resident chat turn exceeded {}s",
        CHAT_TURN_TIMEOUT.as_secs()
    )
}

/// Strip the double encoding a stored `said` can carry: a leading or trailing
/// `"` that is framing, not text.
pub fn trim_double_encoded(said: &str) -> &str {
    let said = said.strip_prefix('"').unwrap_or(said);
    said.strip_suffix('"').unwrap_or(said)
}

/// Bundle ordered rows into pairs; the mapper's own template prompts (user
/// turns opening with `mode: `) are dropped so the loop never maps itself.
pub fn bundle_pairs(rows: &[TurnRow]) -> Vec<Pair> {
    let mut pairs = Vec::new();
    let mut ai_session = String::new();
    let mut last_ai = String::new();
    for row in rows {
        let said = trim_double_encoded(&row.said).to_owned();
        if row.session != ai_session {
            // A fresh session starts with no assistant history.
            ai_session = row.session.clone();
            last_ai.clear();
        }
        if row.role == "assistant" {
            last_ai = said;
        } else if row.role == "user" && !said.starts_with("mode: ") {
            pairs.push(Pair {
                session: row.session.clone(),
                turn: row.turn,
                ts: row.ts,
                ai_text: last_ai.clone(),
                user_text: said,
            });
        }
    }
    pairs
}

/// Consecutive same-role rows collapse: each user run since the model last
/// spoke is one bundle; each assistant run is one `ai_text` block.
pub fn bundle_runs(rows: &[TurnRow]) -> Vec<Pair> {
    fn push_pair(
        session: &str,
        ai_text: &str,
        pending: &mut Option<(i64, i64, String)>,
        pairs: &mut Vec<Pair>,
    ) {
        if let Some((turn, ts, text)) = pending.take() {
            pairs.push(Pair {
                session: session.to_owned(),
                turn,
                ts,
                ai_text: ai_text.to_owned(),
                user_text: text,
            });
        }
    }
    let mut pairs = Vec::new();
    let mut session = String::new();
    let mut ai_text = String::new();
    let mut pending: Option<(i64, i64, String)> = None;
    for row in rows {
        let said = trim_double_encoded(&row.said).to_owned();
        if row.session != session {
            push_pair(&session, &ai_text, &mut pending, &mut pairs);
            session = row.session.clone();
            ai_text.clear();
        }
        if row.role == "assistant" {
            if pending.is_some() {
                push_pair(&session, &ai_text, &mut pending, &mut pairs);
                ai_text.clear();
            }
            if !ai_text.is_empty() {
                ai_text.push_str("\n\n");
            }
            ai_text.push_str(&said);
        } else if row.role == "user" && !said.starts_with("mode: ") {
            match &mut pending {
                Some((turn, ts, text)) => {
                    text.push_str("\n\n");
                    text.push_str(&said);
                    *turn = row.turn;
                    *ts = row.ts;
                }
                None => pending = Some((row.turn, row.ts, said)),
            }
        }
    }
    push_pair(&session, &ai_text, &mut pending, &mut pairs);
    pairs
}

/// The newest assistant turn in `pair`'s session strictly before its ts, or
/// `None` when the session had none (its first turn; nothing to rewrite).
fn last_assistant_before(store: &crate::Store, pair: &Pair) -> Option<String> {
    let query = TurnQuery {
        session: Some(pair.session.clone()),
        role: Some("assistant".to_owned()),
        until: Some(pair.ts as u64),
        ..Default::default()
    };
    let rows = store.turn_rows(&query).ok()?;
    rows.iter()
        .filter(|row| row.ts < pair.ts)
        .max_by_key(|row| row.ts)
        .map(|row| trim_double_encoded(&row.said).to_owned())
}

/// Past the cap, only the newest pair survives; a cap of 0 never drops.
pub fn coalesce_with_cap(mut pairs: Vec<Pair>, cap: usize) -> Vec<Pair> {
    if cap > 0 && pairs.len() > cap {
        pairs.split_off(pairs.len() - 1)
    } else {
        pairs
    }
}

/// Substitute `{{mode}}`, `{{ai_text}}`, `{{user_text}}` in the template.
/// Unknown keys stay verbatim so a typo is visible in the sent prompt.
pub fn render_template(template: &str, mode: &str, ai: &str, user: &str) -> String {
    template
        .replace("{{mode}}", mode)
        .replace("{{ai_text}}", ai)
        .replace("{{user_text}}", user)
}

/// Expand `${NAME}` and `${NAME:-default}` (shell defaulting) in the
/// template and rules files before use; unset+defaultless is empty, like sh.
pub fn expand_env(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str("${");
            out.push_str(after);
            return out;
        };
        let name = &after[..end];
        let value = match name.split_once(":-") {
            Some((var, default)) => std::env::var(var).unwrap_or_else(|_| default.to_owned()),
            None => std::env::var(name).unwrap_or_default(),
        };
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Whitespace-collapsed form for the fixed-point test.
pub fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The resident loop.
pub fn run(args: Args) -> Result<()> {
    // Read-only: this loop never writes the store, and a read-only connection
    // never fights the resident `db sync` writer for the write lock.
    let store_path = match &args.store_path {
        Some(path) => path.clone(),
        None => crate::ident::Store::default_path().context("resolve the default boop store")?,
    };
    let store =
        crate::ident::Store::open_readonly(store_path).context("open boop store read-only")?;
    // Leak the registry so the adapter is 'static: the oneshot feed bounds a
    // model pass on its own thread. The resident never returns, so the leak is
    // one small fixed allocation.
    let registry: &'static Registry = Box::leak(Box::new(Registry::discover()));
    let harness = crate::lane::harness_for_model(&args.model)?
        .with_context(|| format!("model `{}` names no harness", args.model))?;
    let adapter = registry
        .by_id(&harness)
        .with_context(|| format!("no adapter registered for harness `{harness}`"))?;
    std::fs::create_dir_all(&args.state_dir)
        .with_context(|| format!("create {}", args.state_dir.display()))?;
    let template = args.template.clone();
    let mut cursor = seed_cursor(&args, &store)?;
    let mut done = load_done(&args.state_dir)?;
    let mut rewriter =
        Rewriter::open(&args.formula, adapter, &args).context("open the rewriter")?;
    loop {
        match poll_once(
            &store,
            &mut rewriter,
            &args,
            template.as_deref(),
            cursor,
            &mut done,
        ) {
            Ok(next) => cursor = next,
            // A transient store error (a locked read, a stalled query) kills
            // no resident; the next tick retries from the same cursor.
            Err(error) => {
                eprintln!("concatmap: tick failed at cursor {cursor}, retrying: {error:#}")
            }
        }
        std::thread::sleep(args.poll);
    }
}

/// One tick: query new pairs, coalesce, process serially, advance the cursor.
fn poll_once(
    store: &crate::Store,
    rewriter: &mut Rewriter,
    args: &Args,
    template: Option<&str>,
    cursor: i64,
    done: &mut BTreeSet<(String, i64)>,
) -> Result<i64> {
    let mut max_seen = cursor;
    // One job per bundle: either the caller's window SQL owns the partition,
    // or the compiled bundlers do.
    let jobs: Vec<Job> = match &args.formula.window {
        Some(sql) => {
            let session_id = store
                .session_id_lookup(args.session.as_deref().unwrap_or(""))
                .context("look up the session id for the window SQL")?;
            let window = store
                .window_rows(sql, args.session.as_deref(), session_id, cursor)
                .context("run the window SQL")?;
            for row in &window {
                if row.ts > max_seen {
                    max_seen = row.ts;
                }
            }
            window
                .into_iter()
                // Empty `said` islands (tool-call placeholders) map to nothing.
                .filter(|row| !row.text.trim().is_empty())
                .filter(|row| !done.contains(&(args.session.clone().unwrap_or_default(), row.id)))
                .map(|row| Job::Window {
                    session: args.session.clone().unwrap_or_default(),
                    row,
                })
                .collect()
        }
        None => {
            let query = TurnQuery {
                since: Some(cursor.max(0) as u64),
                session: args.session.clone(),
                ..Default::default()
            };
            let rows = store.turn_rows(&query).context("query new turns")?;
            for row in &rows {
                if row.ts > max_seen {
                    max_seen = row.ts;
                }
            }
            let bundled = match args.formula.bundle {
                BundleShape::Pair => bundle_pairs(&rows),
                BundleShape::Run => bundle_runs(&rows),
            };
            bundled
                .into_iter()
                .filter(|pair| !done.contains(&(pair.session.clone(), pair.turn)))
                .filter_map(|mut pair| {
                    // A windowed query can miss the assistant turn that sits before
                    // the cursor; pull it from the store before giving up on the pair.
                    if pair.ai_text.trim().is_empty() {
                        pair.ai_text = last_assistant_before(store, &pair)?;
                    }
                    Some(pair)
                })
                .map(Job::Pair)
                .collect()
        }
    };
    let jobs = coalesce_jobs(jobs, args.formula.coalesce);
    for job in &jobs {
        let key = job.key();
        // A marker planted by a human or a prior hang skips the job before any
        // attempt; the stat is the second test after the in-memory set.
        if done.contains(&key) {
            continue;
        }
        if marker_planted(&args.state_dir, &key.0, key.1) {
            done.insert(key);
            continue;
        }
        // A failed rewrite retries, then drops the bundle; it never drops the
        // resident.
        for attempt in 1..=REWRITE_ATTEMPTS {
            // A marker planted mid-flight (e.g. while another bundle hung)
            // skips the bundle before the next attempt re-sends it.
            if done.contains(&key) {
                break;
            }
            if marker_planted(&args.state_dir, &key.0, key.1) {
                done.insert(key.clone());
                break;
            }
            match process_job(job, store, rewriter, args, template, done) {
                Ok(()) => break,
                Err(error) if error.downcast_ref::<OneshotTimeout>().is_some() => {
                    // A hang is not transient: mark it failed and move on
                    // rather than burn the retry ladder on a stuck child.
                    eprintln!(
                        "concatmap: rewrite timed out for {}-{}: {error:#}",
                        key.0, key.1
                    );
                    let _ = write_done_marker(args, &key.0, key.1, done);
                    break;
                }
                Err(error) if attempt < REWRITE_ATTEMPTS => {
                    eprintln!(
                        "concatmap: rewrite failed for {}-{} (attempt {attempt}/{}), retrying in {}s: {error:#}",
                        key.0, key.1, REWRITE_ATTEMPTS, REWRITE_BACKOFF.as_secs()
                    );
                    std::thread::sleep(REWRITE_BACKOFF);
                }
                Err(error) => {
                    eprintln!(
                        "concatmap: rewrite failed for {}-{}: {error:#}",
                        key.0, key.1
                    );
                    // A permanently-failed bundle is marked so a poisoned
                    // window cannot re-hang the resident on a later tick.
                    let _ = write_done_marker(args, &key.0, key.1, done);
                }
            }
        }
    }
    if max_seen > cursor {
        std::fs::write(args.state_dir.join("cursor"), max_seen.to_string())
            .context("write cursor")?;
    }
    Ok(max_seen)
}

/// One bundle to map, whichever side produced it.
enum Job {
    Window {
        session: String,
        row: crate::query::WindowRow,
    },
    Pair(Pair),
}

impl Job {
    /// The done-marker key and the out-file stem.
    fn key(&self) -> (String, i64) {
        match self {
            Job::Window { session, row } => (session.clone(), row.id),
            Job::Pair(pair) => (pair.session.clone(), pair.turn),
        }
    }
}

/// Past the cap, only the newest job survives; a cap of 0 never drops.
fn coalesce_jobs(mut jobs: Vec<Job>, cap: usize) -> Vec<Job> {
    if cap > 0 && jobs.len() > cap {
        jobs.split_off(jobs.len() - 1)
    } else {
        jobs
    }
}

/// First run seeds at the store's newest ts so only post-launch pairs map.
fn load_or_seed_cursor(state_dir: &Path, store: &crate::Store) -> Result<i64> {
    let path = state_dir.join("cursor");
    if let Ok(text) = std::fs::read_to_string(&path) {
        return text
            .trim()
            .parse()
            .with_context(|| format!("parse {}", path.display()));
    }
    let query = TurnQuery::default();
    let rows = store.turn_rows(&query).context("seed cursor")?;
    let max_ts = rows.iter().map(|row| row.ts).max().unwrap_or(0);
    std::fs::write(&path, max_ts.to_string()).context("write seed cursor")?;
    Ok(max_ts)
}

/// The starting cursor: `--from-start` is 0, `--cursor N` is N, otherwise the
/// persisted cursor (seeding at the newest ts on first run).
fn seed_cursor(args: &Args, store: &crate::Store) -> Result<i64> {
    if args.from_start {
        return Ok(0);
    }
    if let Some(cursor) = args.cursor {
        return Ok(cursor);
    }
    load_or_seed_cursor(&args.state_dir, store)
}

/// True when a (session, id) done marker exists on disk, planted either by
/// this process or by a human while the resident is mid-flight.
fn marker_planted(state_dir: &Path, session: &str, id: i64) -> bool {
    state_dir
        .join("done")
        .join(format!("{session}-{id}"))
        .exists()
}

/// Done markers survive restarts: one empty file per processed (session, turn).
fn load_done(state_dir: &Path) -> Result<BTreeSet<(String, i64)>> {
    let dir = state_dir.join("done");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let mut done = BTreeSet::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        if let Some((session, turn)) = name.rsplit_once('-') {
            if let Ok(turn) = turn.parse() {
                done.insert((session.to_owned(), turn));
            }
        }
    }
    Ok(done)
}

fn process_job(
    job: &Job,
    store: &crate::Store,
    rewriter: &mut Rewriter,
    args: &Args,
    template: Option<&str>,
    done: &mut BTreeSet<(String, i64)>,
) -> Result<()> {
    let (session, id, msg) = match job {
        // The window SQL already built the message; it ships verbatim.
        Job::Window { session, row } => (session.clone(), row.id, row.text.clone()),
        Job::Pair(pair) => {
            let template = template.context("compiled bundling needs --template")?;
            let mode = args
                .mode
                .as_deref()
                .context("compiled bundling needs --mode")?;
            let mut msg = render_template(template, mode, &pair.ai_text, &pair.user_text);
            if args.formula.references {
                msg.push_str("\n\n<references>\n");
                let query = crate::query::FactQuery {
                    session: Some(pair.session.clone()),
                    until: Some(pair.ts as u64),
                    ..Default::default()
                };
                if let Ok(rows) = store.touch_rows(&query) {
                    // One line per path, the newest verb and turn seen up to now.
                    let mut seen: BTreeMap<&str, (String, i64)> = BTreeMap::new();
                    for row in &rows {
                        seen.insert(&row.path, (row.verb.clone(), row.turn));
                    }
                    for (path, (verb, turn)) in seen {
                        msg.push_str(&format!("{path}  {verb}  turn {turn}\n"));
                    }
                }
                msg.push_str("</references>");
            }
            (pair.session.clone(), pair.turn, msg)
        }
    };
    // The mapper's own turns are the output; sync ingests them. The feed just
    // delivers the bundle and records the marker.
    rewriter.rewrite(store, &msg)?;
    write_done_marker(args, &session, id, done)
}

/// Persist a (session, turn) marker and record it in the live set. A marker is
/// the skip signal: both a mapped bundle and a permanently-failed bundle write
/// one, so neither is retried on a later tick.
fn write_done_marker(
    args: &Args,
    session: &str,
    id: i64,
    done: &mut BTreeSet<(String, i64)>,
) -> Result<()> {
    let dir = args.state_dir.join("done");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    std::fs::write(dir.join(format!("{session}-{id}")), b"").context("write done marker")?;
    done.insert((session.to_owned(), id));
    Ok(())
}

/// A hung oneshot must not hang the resident; the same bound as the chat feed.
const ONESHOT_TIMEOUT: Duration = Duration::from_secs(600);

/// A oneshot pass that exceeded its bound. `poll_once` treats this as fatal for
/// the bundle: it is marked and skipped, not retried (a hang does not heal).
#[derive(Debug)]
struct OneshotTimeout {
    model: String,
    secs: u64,
}

impl std::fmt::Display for OneshotTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "one-shot pass on model {} exceeded {}s",
            self.model, self.secs
        )
    }
}

impl std::error::Error for OneshotTimeout {}

/// One model call per window, bounded. A wedged adapter returns control at
/// `timeout`; the adapter owns killing its own child (opencode's 600s guard),
/// this bound is the caller's backstop.
fn one_shot_bounded(
    adapter: &'static dyn Harness,
    msg: &str,
    model: &str,
    timeout: Duration,
) -> Result<String> {
    let spec = OneShotSpec {
        model: Some(model.to_owned()),
        prompt: msg.to_owned(),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(adapter.one_shot(&spec));
    });
    match rx.recv_timeout(timeout) {
        Ok(result) => result.with_context(|| format!("one-shot pass on model {model}")),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            Err(anyhow::Error::new(OneshotTimeout {
                model: model.to_owned(),
                secs: timeout.as_secs(),
            }))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            bail!("one-shot pass on model {model} panicked before replying")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{Delivery, TurnEvent};
    use crate::harness::{Harness, OneShotSpec, ReadChunk, SessionRef};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    fn row(session: &str, turn: i64, ts: i64, role: &str, said: &str) -> TurnRow {
        TurnRow {
            session: session.into(),
            harness: "claude".into(),
            turn,
            ts,
            role: role.into(),
            said: said.into(),
        }
    }

    #[test]
    fn trims_double_encoding() {
        assert_eq!(trim_double_encoded("\"hello\""), "hello");
        assert_eq!(trim_double_encoded("hello"), "hello");
    }

    #[test]
    fn bundles_user_then_assistant_into_one_pair() {
        let rows = vec![
            row("s", 1, 10, "user", "rewrite this"),
            row("s", 2, 11, "assistant", "done"),
            row("s", 3, 12, "user", "next"),
        ];
        let pairs = bundle_pairs(&rows);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].user_text, "rewrite this");
        assert_eq!(pairs[0].ai_text, "");
        assert_eq!(pairs[1].ai_text, "done");
        assert_eq!(pairs[1].user_text, "next");
    }

    #[test]
    fn mapper_template_prompts_never_bundle() {
        let rows = vec![row("m", 1, 10, "user", "mode: tighten\n\nrewrite")];
        assert!(bundle_pairs(&rows).is_empty());
    }

    #[test]
    fn coalesce_keeps_only_the_newest_past_the_cap() {
        let pairs: Vec<Pair> = (0..5)
            .map(|turn| Pair {
                session: "s".into(),
                turn,
                ts: turn,
                ai_text: String::new(),
                user_text: String::new(),
            })
            .collect();
        let kept = coalesce_with_cap(pairs, 4);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].turn, 4);
    }

    #[test]
    fn coalesce_below_the_cap_keeps_everything() {
        let pairs: Vec<Pair> = (0..4)
            .map(|turn| Pair {
                session: "s".into(),
                turn,
                ts: turn,
                ai_text: String::new(),
                user_text: String::new(),
            })
            .collect();
        assert_eq!(coalesce_with_cap(pairs, 4).len(), 4);
        let uncapped: Vec<Pair> = (0..2)
            .map(|turn| Pair {
                session: "s".into(),
                turn,
                ts: turn,
                ai_text: String::new(),
                user_text: String::new(),
            })
            .collect();
        assert_eq!(coalesce_with_cap(uncapped, 0).len(), 2);
    }

    #[test]
    fn render_fills_all_three_keys() {
        let template = "mode: {{mode}}\n<ai>{{ai_text}}</ai>\n<user>{{user_text}}</user>";
        let rendered = render_template(template, "tighten", "a", "u");
        assert_eq!(rendered, "mode: tighten\n<ai>a</ai>\n<user>u</user>");
    }

    #[test]
    fn normalize_collapses_all_whitespace() {
        assert_eq!(normalize("a \n\t b  c"), "a b c");
    }

    #[test]
    fn env_expansion_defaults_like_the_shell() {
        std::env::set_var("CM_EXPAND_TEST", "set");
        assert_eq!(expand_env("x ${CM_EXPAND_TEST} y"), "x set y");
        assert_eq!(expand_env("${CM_EXPAND_TEST:-d}"), "set");
        assert_eq!(expand_env("${CM_EXPAND_UNSET:-dflt}"), "dflt");
        assert_eq!(expand_env("${CM_EXPAND_UNSET}"), "");
        assert_eq!(
            expand_env("literal ${unterminated"),
            "literal ${unterminated"
        );
    }

    #[test]
    fn formula_loads_the_chat_feed_and_goal() {
        let path = std::env::temp_dir().join(format!("cm_formula_{}.json", std::process::id()));
        std::fs::write(&path, r#"{"feed": "chat", "goal": "tighten each bundle"}"#).unwrap();
        let formula = Formula::load(&path).unwrap();
        assert_eq!(formula.feed, Feed::Chat);
        assert_eq!(formula.goal.as_deref(), Some("tighten each bundle"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn formula_rejects_an_unknown_feed() {
        let path = std::env::temp_dir().join(format!("cm_formula_bad_{}.json", std::process::id()));
        std::fs::write(&path, r#"{"feed": "teleport"}"#).unwrap();
        assert!(Formula::load(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn formula_without_rules_is_oneshot() {
        assert_eq!(Formula::oneshot().feed, Feed::OneShot);
        assert_eq!(Formula::oneshot().bundle, BundleShape::Pair);
        assert_eq!(Formula::oneshot().coalesce, 0);
    }

    #[test]
    fn run_bundling_collapses_each_same_role_run() {
        let rows = vec![
            row("s", 1, 10, "assistant", "first reply"),
            row("s", 2, 11, "assistant", "second reply"),
            row("s", 3, 12, "user", "wait"),
            row("s", 4, 13, "user", "actually do this"),
            row("s", 5, 14, "assistant", "done"),
            row("s", 6, 15, "user", "next"),
        ];
        let pairs = bundle_runs(&rows);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].ai_text, "first reply\n\nsecond reply");
        assert_eq!(pairs[0].user_text, "wait\n\nactually do this");
        assert_eq!(pairs[0].turn, 4);
        assert_eq!(pairs[1].ai_text, "done");
        assert_eq!(pairs[1].user_text, "next");
    }

    #[test]
    fn run_bundling_skips_mapper_prompts_whole() {
        let rows = vec![row("m", 1, 10, "user", "mode: tighten\n\nrewrite")];
        assert!(bundle_runs(&rows).is_empty());
    }

    #[test]
    fn formula_reads_the_references_flag() {
        let path = std::env::temp_dir().join(format!("cm_formula_ref_{}.json", std::process::id()));
        std::fs::write(
            &path,
            r#"{"feed": "chat", "goal": "g", "bundle": "run", "coalesce": 0, "references": true}"#,
        )
        .unwrap();
        let formula = Formula::load(&path).unwrap();
        assert!(formula.references);
        assert_eq!(formula.coalesce, 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn formula_reads_the_window_sql() {
        let path = std::env::temp_dir().join(format!("cm_formula_win_{}.json", std::process::id()));
        std::fs::write(
            &path,
            r#"{"feed": "chat", "goal": "g", "window": "SELECT 1 AS id, 'x' AS text"}"#,
        )
        .unwrap();
        let formula = Formula::load(&path).unwrap();
        assert_eq!(
            formula.window.as_deref(),
            Some("SELECT 1 AS id, 'x' AS text")
        );
        assert_eq!(Formula::oneshot().window, None);
        let _ = std::fs::remove_file(&path);
    }

    fn store() -> (crate::Store, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "boop_cm_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_file(&path);
        (crate::Store::open(path.clone()).unwrap(), path)
    }

    // RECEIPT: unfixed tree returned `Err(near "Reilly": syntax error (1))`,
    // swallowed by `.ok()` into `None`, for this exact session id.
    #[test]
    fn context_tokens_handles_a_quote_in_the_session_id() {
        let (store, path) = store();
        let session = "O'Reilly";
        let session_id = store.intern_public("dict_session", session).unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_usage
                   (session_id, turn, ts, request_ref, model_id, input_tokens, cache_read_tokens)
                 VALUES (?1, 1, 1, 1, 1, 40, 60)",
                rusqlite::params![session_id],
            )
            .unwrap();
        assert_eq!(context_tokens(&store, session), Some(100));
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "boop_cm_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A bare session ref; enough for `project_discovered_session` to plant the
    /// `agent_session` row `turn_rows` joins against.
    fn session_ref(session: &str) -> SessionRef {
        SessionRef {
            harness: "fake",
            session_id: session.to_owned(),
            nickname: session.to_owned(),
            path: PathBuf::new(),
            cwd: None,
            git_branch: None,
            modified_ms: 0,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: None,
        }
    }

    /// A fake harness: counts `one_shot` calls, returns a canned reply, and
    /// can hang its first call to exercise the oneshot timeout.
    struct FakeHarness {
        calls: AtomicUsize,
        reply: &'static str,
        hang_first: bool,
        plant_done: Option<(PathBuf, String, i64)>,
    }

    impl Harness for FakeHarness {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn sessions(&self) -> Result<Vec<SessionRef>> {
            Ok(Vec::new())
        }
        fn read_from(&self, _: &SessionRef, _: u64) -> Result<ReadChunk> {
            Ok(ReadChunk {
                events: Vec::new(),
                next_offset: 0,
                reset: false,
                skipped: 0,
            })
        }
        fn one_shot(&self, _: &OneShotSpec) -> Result<String> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                if let Some((dir, session, id)) = &self.plant_done {
                    std::fs::create_dir_all(dir).unwrap();
                    std::fs::write(dir.join(format!("{session}-{id}")), b"").unwrap();
                }
            }
            if self.hang_first && n == 0 {
                std::thread::sleep(Duration::from_secs(3600));
            }
            Ok(self.reply.to_owned())
        }
    }

    /// A fake resident chat: records every `start_turn` text and writes one
    /// assistant reply to the store per turn, simulating the model + sync.
    struct RecordingChannel {
        db: PathBuf,
        session: String,
        next_ts: i64,
        turns: Arc<Mutex<Vec<String>>>,
    }

    impl LaneChannel for RecordingChannel {
        fn conversation_id(&self) -> Option<String> {
            Some(self.session.clone())
        }
        fn start_turn(&mut self, text: &str) -> Result<()> {
            self.turns.lock().unwrap().push(text.to_owned());
            let store = crate::Store::open(self.db.clone())?;
            store.write_turn(
                &self.session,
                self.next_ts as u64,
                self.next_ts as u64,
                "assistant",
                "reply",
            )?;
            self.next_ts += 1;
            Ok(())
        }
        fn steer(&mut self, _: &str) -> Result<Delivery> {
            Ok(Delivery::MidTurn)
        }
        fn next_event(&mut self, _: Duration) -> Result<Option<TurnEvent>> {
            Ok(Some(TurnEvent::ok("done")))
        }
        fn close(&mut self) -> Result<()> {
            Ok(())
        }
    }

    /// A window SQL partitioning same-role runs (gaps-and-islands) into
    /// `id` + `ts` + `text`, honouring `:session` / `:cursor`.
    const WINDOW_SQL: &str = "WITH marked AS (
        SELECT t.turn, t.ts, r.value AS role, t.said,
               ROW_NUMBER() OVER (ORDER BY t.ts, t.turn)
             - ROW_NUMBER() OVER (PARTITION BY r.value ORDER BY t.ts, t.turn) AS island
        FROM agent_turn t JOIN dict_role r ON r.id = t.role_id
        JOIN dict_session s ON s.id = t.session_id
        WHERE s.value = :session AND t.ts > :cursor)
        SELECT max(turn) AS id, max(ts) AS ts, group_concat(said, char(10)) AS text
        FROM marked GROUP BY role, island ORDER BY min(ts)";

    fn window_args(state_dir: PathBuf) -> Args {
        Args {
            template: None,
            mode: None,
            model: "fake-model".into(),
            state_dir,
            store_path: None,
            poll: Duration::ZERO,
            from_start: true,
            cursor: None,
            formula: Formula {
                feed: Feed::OneShot,
                goal: None,
                bundle: BundleShape::Pair,
                coalesce: 0,
                references: false,
                window: Some(WINDOW_SQL.to_owned()),
                compact_tokens: 0,
            },
            session: Some("ses".into()),
        }
    }

    #[test]
    fn oneshot_window_maps_each_window_exactly_once() {
        let (store, db_path) = store();
        store.write_turn("ses", 1, 10, "assistant", "a1").unwrap();
        store.write_turn("ses", 2, 11, "assistant", "a2").unwrap();
        store.write_turn("ses", 3, 12, "user", "u1").unwrap();
        store.write_turn("ses", 4, 13, "assistant", "a3").unwrap();
        let harness: &'static FakeHarness = Box::leak(Box::new(FakeHarness {
            calls: AtomicUsize::new(0),
            reply: "rewritten",
            hang_first: false,
            plant_done: None,
        }));
        let args = window_args(temp_dir("oneshot_state"));
        let mut rewriter = Rewriter::OneShot {
            adapter: harness,
            model: "fake-model".into(),
            timeout: Duration::from_secs(5),
        };
        let mut done = BTreeSet::new();
        // Three windows (assistant run, user run, assistant run); each is one
        // model call, and the cursor lands on the newest ts.
        let next = poll_once(&store, &mut rewriter, &args, None, 0, &mut done).unwrap();
        assert_eq!(next, 13);
        assert_eq!(harness.calls.load(Ordering::SeqCst), 3);
        assert_eq!(done.len(), 3);
        // Replaying from the advanced cursor yields nothing new.
        let again = poll_once(&store, &mut rewriter, &args, None, next, &mut done).unwrap();
        assert_eq!(again, 13);
        assert_eq!(harness.calls.load(Ordering::SeqCst), 3);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn poisoned_bundle_times_out_and_the_next_window_still_processes() {
        let (store, db_path) = store();
        store.write_turn("ses", 1, 10, "assistant", "a1").unwrap();
        store.write_turn("ses", 2, 11, "user", "u1").unwrap();
        let harness: &'static FakeHarness = Box::leak(Box::new(FakeHarness {
            calls: AtomicUsize::new(0),
            reply: "rewritten",
            hang_first: true,
            plant_done: None,
        }));
        let args = window_args(temp_dir("poison_state"));
        let mut rewriter = Rewriter::OneShot {
            adapter: harness,
            model: "fake-model".into(),
            timeout: Duration::from_millis(100),
        };
        let mut done = BTreeSet::new();
        poll_once(&store, &mut rewriter, &args, None, 0, &mut done).unwrap();
        // The hung first window is marked and skipped; the second still mapped.
        assert_eq!(harness.calls.load(Ordering::SeqCst), 2);
        assert_eq!(done.len(), 2);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn a_planted_marker_skips_the_bundle_mid_flight() {
        let (store, db_path) = store();
        store.write_turn("ses", 1, 10, "assistant", "a1").unwrap();
        store.write_turn("ses", 2, 11, "user", "u1").unwrap();
        let state_dir = temp_dir("plant_state");
        // The first one_shot call plants a done marker for the second window
        // (id 2) before returning, as a human would mid-flight.
        let harness: &'static FakeHarness = Box::leak(Box::new(FakeHarness {
            calls: AtomicUsize::new(0),
            reply: "rewritten",
            hang_first: false,
            plant_done: Some((state_dir.join("done"), "ses".to_owned(), 2)),
        }));
        let args = window_args(state_dir);
        let mut rewriter = Rewriter::OneShot {
            adapter: harness,
            model: "fake-model".into(),
            timeout: Duration::from_secs(5),
        };
        let mut done = BTreeSet::new();
        poll_once(&store, &mut rewriter, &args, None, 0, &mut done).unwrap();
        // The first window mapped, then the planted marker skipped the second.
        assert_eq!(harness.calls.load(Ordering::SeqCst), 1);
        assert_eq!(done.len(), 2);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn from_start_and_explicit_cursor_beat_the_persisted_seed() {
        let (store, db_path) = store();
        let state_dir = temp_dir("seed_state");
        std::fs::write(state_dir.join("cursor"), "42").unwrap();
        let mut args = window_args(state_dir);
        args.from_start = false;

        let mut from_start = args.clone();
        from_start.from_start = true;
        assert_eq!(seed_cursor(&from_start, &store).unwrap(), 0);

        let mut explicit = args.clone();
        explicit.cursor = Some(7);
        assert_eq!(seed_cursor(&explicit, &store).unwrap(), 7);

        assert_eq!(seed_cursor(&args, &store).unwrap(), 42);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn chat_feed_carries_state_across_two_windows() {
        let (store, db_path) = store();
        store.project_discovered_session(&session_ref("s")).unwrap();
        let adapter: &'static FakeHarness = Box::leak(Box::new(FakeHarness {
            calls: AtomicUsize::new(0),
            reply: "",
            hang_first: false,
            plant_done: None,
        }));
        let spec = ChannelSpec {
            model: Some("fake-model".into()),
            cwd: temp_dir("chat_cwd"),
            resume: None,
            lane: None,
        };
        let turns = Arc::new(Mutex::new(Vec::new()));
        let mut rewriter = Rewriter::Chat {
            adapter,
            spec,
            channel: Box::new(RecordingChannel {
                db: db_path.clone(),
                session: "s".into(),
                next_ts: 1,
                turns: turns.clone(),
            }),
            pending_goal: Some("goal turn".into()),
            compact_tokens: 0,
        };
        assert_eq!(rewriter.rewrite(&store, "bundle one").unwrap(), "");
        assert_eq!(rewriter.rewrite(&store, "bundle two").unwrap(), "");
        // The goal opens the resident once; both bundles accumulate on it.
        let recorded = turns.lock().unwrap();
        assert_eq!(
            *recorded,
            vec![
                "goal turn".to_owned(),
                "bundle one".to_owned(),
                "bundle two".to_owned()
            ]
        );
        let _ = std::fs::remove_file(&db_path);
    }

    /// The chat feed consumes the turn lifecycle as events: it fires a bundle,
    /// waits for `TurnDone`, then fires the next. The channel is the subject and
    /// the feed `concatMap`s over it (one inner pipeline per bundle, serial).
    ///
    ///   start_turn(goal)   -> Started -> Done
    ///   start_turn(bundle) -> Started -> Done
    ///   start_turn(bundle) -> Started -> Done
    #[test]
    fn the_chat_feed_waits_for_turn_done_before_the_next_bundle() {
        struct Subject {
            delivered: Arc<Mutex<Vec<String>>>,
            events: Arc<AtomicUsize>,
            started: bool,
        }
        impl LaneChannel for Subject {
            fn conversation_id(&self) -> Option<String> {
                Some("s".into())
            }
            fn start_turn(&mut self, text: &str) -> Result<()> {
                self.delivered.lock().unwrap().push(text.to_owned());
                self.started = false;
                Ok(())
            }
            fn steer(&mut self, _: &str) -> Result<Delivery> {
                Ok(Delivery::MidTurn)
            }
            fn next_event(&mut self, _: Duration) -> Result<Option<TurnEvent>> {
                self.events.fetch_add(1, Ordering::SeqCst);
                if !self.started {
                    self.started = true;
                    Ok(Some(TurnEvent::Started))
                } else {
                    Ok(Some(TurnEvent::ok("done")))
                }
            }
            fn close(&mut self) -> Result<()> {
                Ok(())
            }
        }

        let (store, db_path) = store();
        let adapter: &'static FakeHarness = Box::leak(Box::new(FakeHarness {
            calls: AtomicUsize::new(0),
            reply: "",
            hang_first: false,
            plant_done: None,
        }));
        let spec = ChannelSpec {
            model: Some("fake-model".into()),
            cwd: temp_dir("subject_cwd"),
            resume: None,
            lane: None,
        };
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let events = Arc::new(AtomicUsize::new(0));
        let mut rewriter = Rewriter::Chat {
            adapter,
            spec,
            channel: Box::new(Subject {
                delivered: delivered.clone(),
                events: events.clone(),
                started: false,
            }),
            pending_goal: Some("goal turn".into()),
            compact_tokens: 0,
        };

        rewriter.rewrite(&store, "bundle one").unwrap();
        rewriter.rewrite(&store, "bundle two").unwrap();

        let texts: Vec<String> = delivered.lock().unwrap().clone();
        assert_eq!(texts, ["goal turn", "bundle one", "bundle two"]);
        // Three turns (goal + two bundles), each consumed as Started then Done:
        // the feed polled the subject for turn-done, never slept a fixed gap.
        assert_eq!(events.load(Ordering::SeqCst), 6);
        let _ = std::fs::remove_file(&db_path);
    }
}
