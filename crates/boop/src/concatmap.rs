//! The concatMap refinement loop: read new (assistant, user) contact pairs
//! from the store and pipe each through a one-shot model pass to fixed point.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::channel::{ChannelSpec, LaneChannel};
use crate::harness::{Harness, OneShotSpec};
use crate::ident::TurnQuery;
use crate::registry::Registry;
use crate::rows::TurnRow;

/// The queue cap; past this, only the newest pair survives.
const QUEUE_CAP: usize = 4;

/// A failed rewrite is retried this many times, then the pair is dropped.
const REWRITE_ATTEMPTS: u32 = 3;
const REWRITE_BACKOFF: Duration = Duration::from_secs(10);

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
pub struct Args {
    pub template: Option<String>,
    pub mode: Option<String>,
    pub model: String,
    pub state_dir: PathBuf,
    pub out_dir: PathBuf,
    pub poll: Duration,
    pub cap: u32,
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
    /// Backlog cap; 0 never drops. Defaults to `QUEUE_CAP`.
    pub coalesce: usize,
    /// Append a <references> block of the source session's file touches as
    /// of each bundle, so reference claims come from the store, not the model.
    pub references: bool,
    /// Caller-owned window SQL; when present it replaces the compiled
    /// bundlers entirely (see `Store::window_rows` for the row contract).
    pub window: Option<String>,
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
            coalesce: QUEUE_CAP,
            references: false,
            window: None,
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
            other => bail!("unknown feed `{other}` in {}; expected oneshot or chat", path.display()),
        };
        let bundle = match file.bundle.as_deref() {
            None | Some("pair") => BundleShape::Pair,
            Some("run") => BundleShape::Run,
            Some(other) => bail!("unknown bundle `{other}` in {}; expected pair or run", path.display()),
        };
        Ok(Formula {
            feed,
            goal: file.goal,
            bundle,
            coalesce: file.coalesce.unwrap_or(QUEUE_CAP),
            references: file.references.unwrap_or(false),
            window: file.window,
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
}

/// The rewrite surface: one enum value per feed, chosen once at boot from the
/// formula. `rewrite` is the whole contract.
enum Rewriter<'a> {
    OneShot {
        adapter: &'a dyn Harness,
        model: String,
        cap: u32,
    },
    Chat {
        adapter: &'a dyn Harness,
        channel: Box<dyn LaneChannel>,
        cwd: PathBuf,
        goal: Option<String>,
    },
}

/// Turn-end and reply-capture budgets for the chat feed.
const CHAT_TURN_TIMEOUT: Duration = Duration::from_secs(600);
const CHAT_REPLY_TIMEOUT: Duration = Duration::from_secs(120);
const CHAT_POLL: Duration = Duration::from_secs(5);

impl<'a> Rewriter<'a> {
    fn open(formula: &Formula, adapter: &'a dyn Harness, args: &Args) -> Result<Rewriter<'a>> {
        match formula.feed {
            Feed::OneShot => Ok(Rewriter::OneShot {
                adapter,
                model: args.model.clone(),
                cap: args.cap,
            }),
            Feed::Chat => {
                let spec = ChannelSpec {
                    model: Some(args.model.clone()),
                    cwd: args.state_dir.clone(),
                    resume: None,
                };
                let channel = adapter
                    .open_channel(&spec)
                    .context("open the resident chat")?;
                Ok(Rewriter::Chat {
                    adapter,
                    channel,
                    cwd: args.state_dir.clone(),
                    goal: formula.goal.clone(),
                })
            }
        }
    }

    fn rewrite(&mut self, store: &crate::Store, msg: &str) -> Result<String> {
        match self {
            Rewriter::OneShot { adapter, model, cap } => {
                passes_until_fixed(*adapter, msg, model, *cap)
            }
            Rewriter::Chat { adapter, channel, cwd, goal } => {
                let channel = channel.as_mut();
                if let Some(goal) = goal.take() {
                    channel.start_turn(&goal).context("send the goal turn")?;
                    wait_turn(channel)?;
                }
                let session = mapper_session(*adapter, channel, cwd);
                let marker = session
                    .as_deref()
                    .and_then(|s| newest_assistant_ts(store, s))
                    .unwrap_or(0);
                channel.start_turn(msg).context("send the bundle")?;
                wait_turn(channel)?;
                let session = session
                    .context("the resident chat never resolved a harness session id")?;
                wait_reply_text(store, &session, marker)
            }
        }
    }
}

/// Block until the in-flight turn ends or the budget dies.
fn wait_turn(channel: &mut dyn LaneChannel) -> Result<()> {
    let deadline = Instant::now() + CHAT_TURN_TIMEOUT;
    while Instant::now() < deadline {
        match channel.poll_turn(CHAT_POLL)? {
            Some(end) if end.ok => return Ok(()),
            Some(end) => bail!("resident chat turn failed: {}", end.detail),
            None => continue,
        }
    }
    bail!("resident chat turn exceeded {}s", CHAT_TURN_TIMEOUT.as_secs())
}

/// The chat's harness session id once it exists; until the harness resolves
/// one, fall back to the newest session whose cwd is the pipe's own.
fn mapper_session(adapter: &dyn Harness, channel: &dyn LaneChannel, cwd: &Path) -> Option<String> {
    if let Some(id) = channel.conversation_id() {
        return Some(id);
    }
    // opencode canonicalizes directories (/tmp -> /private/tmp); compare the
    // canonical spelling or the fallback never matches.
    let cwd = std::fs::canonicalize(cwd)
        .unwrap_or_else(|_| cwd.to_owned())
        .display()
        .to_string();
    adapter
        .sessions()
        .ok()?
        .into_iter()
        .filter(|session| session.cwd.as_deref() == Some(cwd.as_str()))
        .max_by_key(|session| session.modified_ms)
        .map(|session| session.session_id)
}

fn newest_assistant_ts(store: &crate::Store, session: &str) -> Option<i64> {
    let query = TurnQuery {
        session: Some(session.to_owned()),
        role: Some("assistant".to_owned()),
        ..Default::default()
    };
    let rows = store.turn_rows(&query).ok()?;
    rows.iter().map(|row| row.ts).max()
}

/// The rewrite is the mapper conversation's newest assistant turn past
/// `marker`; sync ingests it, so poll the store until it lands.
fn wait_reply_text(store: &crate::Store, session: &str, marker: i64) -> Result<String> {
    let deadline = Instant::now() + CHAT_REPLY_TIMEOUT;
    while Instant::now() < deadline {
        let query = TurnQuery {
            session: Some(session.to_owned()),
            role: Some("assistant".to_owned()),
            ..Default::default()
        };
        if let Some(row) = store
            .turn_rows(&query)
            .ok()
            .and_then(|rows| rows.into_iter().filter(|row| row.ts > marker).max_by_key(|row| row.ts))
        {
            let text = trim_double_encoded(&row.said).to_owned();
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    bail!("reply from {session} never reached the store past {}", marker)
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
    let store = crate::ident::Store::open_readonly(crate::ident::Store::default_path()?)
        .context("open boop store read-only")?;
    let registry = Registry::discover();
    let harness = crate::lane::harness_for_model(&args.model)?
        .with_context(|| format!("model `{}` names no harness", args.model))?;
    let adapter = registry
        .by_id(&harness)
        .with_context(|| format!("no adapter registered for harness `{harness}`"))?;
    std::fs::create_dir_all(&args.state_dir)
        .with_context(|| format!("create {}", args.state_dir.display()))?;
    std::fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("create {}", args.out_dir.display()))?;
    let template = args.template.clone();
    let mut cursor = load_or_seed_cursor(&args.state_dir, &store)?;
    let mut done = load_done(&args.state_dir)?;
    let mut rewriter = Rewriter::open(&args.formula, adapter, &args).context("open the rewriter")?;
    loop {
        match poll_once(&store, &mut rewriter, &args, template.as_deref(), cursor, &mut done) {
            Ok(next) => cursor = next,
            // A transient store error (a locked read, a stalled query) kills
            // no resident; the next tick retries from the same cursor.
            Err(error) => eprintln!("concatmap: tick failed at cursor {cursor}, retrying: {error:#}"),
        }
        std::thread::sleep(args.poll);
    }
}

/// One tick: query new pairs, coalesce, process serially, advance the cursor.
fn poll_once(
    store: &crate::Store,
    rewriter: &mut Rewriter<'_>,
    args: &Args,
    template: Option<&str>,
    cursor: i64,
    done: &mut BTreeSet<(String, i64)>,
) -> Result<i64> {
    let query = TurnQuery {
        since: Some(cursor.max(0) as u64),
        session: args.session.clone(),
        ..Default::default()
    };
    let rows = store.turn_rows(&query).context("query new turns")?;
    let mut max_seen = cursor;
    for row in &rows {
        if row.ts > max_seen {
            max_seen = row.ts;
        }
    }
    // One job per bundle: either the caller's window SQL owns the partition,
    // or the compiled bundlers do.
    let jobs: Vec<Job> = match &args.formula.window {
        Some(sql) => {
            let session_id = store
                .session_id_lookup(args.session.as_deref().unwrap_or(""))
                .context("look up the session id for the window SQL")?;
            let mut window = store
                .window_rows(sql, args.session.as_deref(), session_id, cursor)
                .context("run the window SQL")?;
            if args.formula.coalesce > 0 && window.len() > args.formula.coalesce {
                window = window.split_off(window.len() - 1);
            }
            window
                .into_iter()
                .filter(|row| !done.contains(&(args.session.clone().unwrap_or_default(), row.id)))
                .map(|row| Job::Window {
                    session: args.session.clone().unwrap_or_default(),
                    row,
                })
                .collect()
        }
        None => {
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
                .map(|pair| Job::Pair(pair))
                .collect()
        }
    };
    let jobs = coalesce_jobs(jobs, args.formula.coalesce);
    for job in &jobs {
        // A failed rewrite retries, then drops the bundle; it never drops the resident.
        for attempt in 1..=REWRITE_ATTEMPTS {
            match process_job(job, store, rewriter, args, template, done) {
                Ok(()) => break,
                Err(error) if attempt < REWRITE_ATTEMPTS => {
                    eprintln!(
                        "concatmap: rewrite failed for {}-{} (attempt {attempt}/{}), retrying in {}s: {error:#}",
                        job.key().0, job.key().1, REWRITE_ATTEMPTS, REWRITE_BACKOFF.as_secs()
                    );
                    std::thread::sleep(REWRITE_BACKOFF);
                }
                Err(error) => eprintln!(
                    "concatmap: rewrite failed for {}-{}: {error:#}",
                    job.key().0, job.key().1
                ),
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
        return text.trim().parse().with_context(|| format!("parse {}", path.display()));
    }
    let query = TurnQuery::default();
    let rows = store.turn_rows(&query).context("seed cursor")?;
    let max_ts = rows.iter().map(|row| row.ts).max().unwrap_or(0);
    std::fs::write(&path, max_ts.to_string()).context("write seed cursor")?;
    Ok(max_ts)
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
    rewriter: &mut Rewriter<'_>,
    args: &Args,
    template: Option<&str>,
    done: &mut BTreeSet<(String, i64)>,
) -> Result<()> {
    let (session, id, msg) = match job {
        // The window SQL already built the message; it ships verbatim.
        Job::Window { session, row } => (session.clone(), row.id, row.text.clone()),
        Job::Pair(pair) => {
            let template = template.context("compiled bundling needs --template")?;
            let mode = args.mode.as_deref().context("compiled bundling needs --mode")?;
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
    let out_dir = args.out_dir.join(session.chars().take(8).collect::<String>());
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("create {}", out_dir.display()))?;
    let out_path = out_dir.join(format!("{}.md", id));
    let text = rewriter.rewrite(store, &msg)?;
    std::fs::write(&out_path, text)
        .with_context(|| format!("write {}", out_path.display()))?;
    std::fs::write(args.state_dir.join("done").join(format!("{}-{}", session, id)), b"")
        .context("write done marker")?;
    done.insert((session, id));
    Ok(())
}

/// One-shot passes until the normalized output repeats or the cap hits. The
/// harness adapter owns the command spelling; this loop names no binary.
fn passes_until_fixed(adapter: &dyn Harness, msg: &str, model: &str, cap: u32) -> Result<String> {
    let mut prev: Option<String> = None;
    let mut last = String::new();
    for _ in 0..cap.max(1) {
        let spec = OneShotSpec {
            model: Some(model.to_owned()),
            prompt: msg.to_owned(),
        };
        let text = adapter
            .one_shot(&spec)
            .with_context(|| format!("one-shot pass on model {model}"))?;
        if prev.as_deref() == Some(normalize(&text).as_str()) {
            return Ok(text);
        }
        prev = Some(normalize(&text));
        last = text;
    }
    Ok(last)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let kept = coalesce_with_cap(pairs, QUEUE_CAP);
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
        assert_eq!(coalesce_with_cap(pairs, QUEUE_CAP).len(), 4);
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
        assert_eq!(expand_env("literal ${unterminated"), "literal ${unterminated");
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
        assert_eq!(Formula::oneshot().coalesce, QUEUE_CAP);
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
}
