//! The concatMap refinement loop: read new (assistant, user) contact pairs
//! from the store and pipe each through a one-shot model pass to fixed point.

use std::collections::BTreeSet;
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
    pub template: PathBuf,
    pub mode: String,
    pub model: String,
    pub state_dir: PathBuf,
    pub out_dir: PathBuf,
    pub poll: Duration,
    pub cap: u32,
    pub formula: Formula,
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
}

impl Formula {
    pub fn oneshot() -> Formula {
        Formula { feed: Feed::OneShot, goal: None }
    }

    /// Load from a json rules file: `{"feed": "chat", "goal": "..."}`.
    pub fn load(path: &Path) -> Result<Formula> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read rules {}", path.display()))?;
        let file: FormulaFile = serde_json::from_str(&text)
            .with_context(|| format!("parse rules {}", path.display()))?;
        let feed = match file.feed.as_str() {
            "oneshot" => Feed::OneShot,
            "chat" => Feed::Chat,
            other => bail!("unknown feed `{other}` in {}; expected oneshot or chat", path.display()),
        };
        Ok(Formula { feed, goal: file.goal })
    }
}

#[derive(Deserialize)]
struct FormulaFile {
    feed: String,
    goal: Option<String>,
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
    if channel.conversation_id_kind() == "harness_session" {
        if let Some(id) = channel.conversation_id() {
            return Some(id);
        }
    }
    let cwd = cwd.display().to_string();
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

/// Past the cap, only the newest pair survives.
pub fn coalesce(mut pairs: Vec<Pair>) -> Vec<Pair> {
    if pairs.len() > QUEUE_CAP {
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

/// Whitespace-collapsed form for the fixed-point test.
pub fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The resident loop.
pub fn run(args: Args) -> Result<()> {
    let store = crate::open_default().context("open boop store")?;
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
    let template = std::fs::read_to_string(&args.template)
        .with_context(|| format!("read {}", args.template.display()))?;
    let mut cursor = load_or_seed_cursor(&args.state_dir, &store)?;
    let mut done = load_done(&args.state_dir)?;
    let mut rewriter = Rewriter::open(&args.formula, adapter, &args).context("open the rewriter")?;
    loop {
        cursor = poll_once(&store, &mut rewriter, &args, &template, cursor, &mut done)?;
        std::thread::sleep(args.poll);
    }
}

/// One tick: query new pairs, coalesce, process serially, advance the cursor.
fn poll_once(
    store: &crate::Store,
    rewriter: &mut Rewriter<'_>,
    args: &Args,
    template: &str,
    cursor: i64,
    done: &mut BTreeSet<(String, i64)>,
) -> Result<i64> {
    let query = TurnQuery {
        since: Some(cursor.max(0) as u64),
        ..Default::default()
    };
    let rows = store.turn_rows(&query).context("query new turns")?;
    let mut max_seen = cursor;
    for row in &rows {
        if row.ts > max_seen {
            max_seen = row.ts;
        }
    }
    let fresh: Vec<Pair> = bundle_pairs(&rows)
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
        .collect();
    for pair in coalesce(fresh) {
        // A failed rewrite drops the pair, never the resident.
        if let Err(error) = process_pair(&pair, store, rewriter, args, template, done) {
            eprintln!("concatmap: rewrite failed for {}-{}: {error:#}", pair.session, pair.turn);
        }
    }
    if max_seen > cursor {
        std::fs::write(args.state_dir.join("cursor"), max_seen.to_string())
            .context("write cursor")?;
    }
    Ok(max_seen)
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

fn process_pair(
    pair: &Pair,
    store: &crate::Store,
    rewriter: &mut Rewriter<'_>,
    args: &Args,
    template: &str,
    done: &mut BTreeSet<(String, i64)>,
) -> Result<()> {
    let msg = render_template(template, &args.mode, &pair.ai_text, &pair.user_text);
    let out_dir = args.out_dir.join(pair.session.chars().take(8).collect::<String>());
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("create {}", out_dir.display()))?;
    let out_path = out_dir.join(format!("{}.md", pair.turn));
    let text = rewriter.rewrite(store, &msg)?;
    std::fs::write(&out_path, text)
        .with_context(|| format!("write {}", out_path.display()))?;
    std::fs::write(args.state_dir.join("done").join(format!("{}-{}", pair.session, pair.turn)), b"")
        .context("write done marker")?;
    done.insert((pair.session.clone(), pair.turn));
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
        let kept = coalesce(pairs);
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
        assert_eq!(coalesce(pairs).len(), 4);
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
        assert_eq!(Formula::oneshot(), Formula { feed: Feed::OneShot, goal: None });
    }
}
