//! Cached review notifications.
//!
//! One shared consumer tails the ghcache `change_log` out of its read-only
//! SQLite database, correlates each change to the Boop lane that owns the
//! repo+branch, and turns it into a pending review request. A push is a
//! checkpoint-review request; a PR is final-review intent. A burst of pushes
//! coalesces to the newest observed head with both a quiet window and a hard
//! maximum wait, and a force-push/rebase whose proposed content is identical
//! against the same base context reuses the previous review instead of
//! enqueueing a duplicate.
//!
//! The cache is read-only. Durable state (source cursor, lane<->repo mapping,
//! pending requests, what was already notified, and the review generation)
//! lives in `review.db` beside the mailbox, so a supervisor restart resumes
//! without replaying a notification.
//!
//! The cache client dependency cannot be declared in this workspace while
//! `ghcache-client` links `libsqlite3-sys` 0.30 and Boop's store links 0.38
//! (Cargo refuses two packages with the same `links` value). This module reads
//! the documented ghcache schema with rusqlite; [`CacheReader`] is the one
//! seam a feature-gated client would replace. See the task report.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use tracing::{debug, info, warn};

use boop_store::bus::{self, MessageKind};
use boop_store::Store;

/// How often the shared consumer reads the cache. One loop for every lane and
/// subscriber; never one busy poll per lane.
const POLL: Duration = Duration::from_millis(500);
/// How long the consumer waits before retrying a cache that is absent, locked,
/// or has no path. The state is explicit, not a silent fallback to `gh`.
const MISSING_CACHE_BACKOFF: Duration = Duration::from_secs(30);
/// Largest change_log batch read in one tick.
const BATCH: i64 = 500;

const CURSOR_SOURCE: &str = "ghcache/change_log";

const REVIEW_QUIET_ENV: &str = "BOOP_REVIEW_QUIET_SECS";
const REVIEW_MAX_WAIT_ENV: &str = "BOOP_REVIEW_MAX_WAIT_SECS";
const DEFAULT_REVIEW_QUIET: Duration = Duration::from_secs(3);
const DEFAULT_REVIEW_MAX_WAIT: Duration = Duration::from_secs(30);

/// Coalescing policy: quiet window per burst and a hard ceiling that a
/// continuous stream of pushes cannot extend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub quiet_ms: u64,
    pub max_wait_ms: u64,
}

impl Policy {
    pub fn from_env() -> Policy {
        Policy {
            quiet_ms: parse_secs(
                std::env::var(REVIEW_QUIET_ENV).ok().as_deref(),
                DEFAULT_REVIEW_QUIET,
            ),
            max_wait_ms: parse_secs(
                std::env::var(REVIEW_MAX_WAIT_ENV).ok().as_deref(),
                DEFAULT_REVIEW_MAX_WAIT,
            ),
        }
    }
}

fn parse_secs(raw: Option<&str>, fallback: Duration) -> u64 {
    raw.and_then(|value| value.parse::<u64>().ok())
        .map(|secs| secs.saturating_mul(1000))
        .unwrap_or(fallback.as_millis() as u64)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Whether a change is a checkpoint (push) or final-review (PR) intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Checkpoint,
    Final,
}

impl Intent {
    fn as_str(self) -> &'static str {
        match self {
            Intent::Checkpoint => "checkpoint",
            Intent::Final => "final",
        }
    }

    fn from_str(value: &str) -> Intent {
        if value == "final" {
            Intent::Final
        } else {
            Intent::Checkpoint
        }
    }
}

/// One immutable review snapshot: the exact head and base a CI/final check
/// must validate, plus the content fingerprint and base context used only to
/// decide review reuse. A SHA identifies a head revision, not the request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Generation {
    pub head: String,
    pub base: String,
    /// merge-base(base, head); `None` when git could not establish it.
    pub base_context: Option<String>,
    /// Hash of the raw diff and binary patch against `base_context`; `None`
    /// when git could not read the change, which is never equivalent.
    pub fingerprint: Option<String>,
}

impl Generation {
    /// Two generations reuse one review only when both the proposed content
    /// and the relevant base context are identical. Git patch-id alone is not
    /// proof of semantic equivalence, so the fingerprint carries raw modes,
    /// rename/delete status and binary content.
    pub fn equivalent(&self, other: &Generation) -> bool {
        match (
            &self.fingerprint,
            &other.fingerprint,
            &self.base_context,
            &other.base_context,
        ) {
            (Some(a), Some(b), Some(ac), Some(bc)) => a == b && ac == bc,
            _ => false,
        }
    }
}

/// One observed change correlated to a lane, before coalescing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub lane: String,
    pub key: String,
    pub repo_slug: String,
    pub branch: String,
    pub generation: Generation,
    pub intent: Intent,
    /// The PR url (final) or `None` (checkpoint).
    pub url: Option<String>,
    pub title: Option<String>,
    pub observed_ms: u64,
}

/// A coalesced review request, persisted across restarts.
#[derive(Clone, Debug)]
pub struct Pending {
    pub observation: Observation,
    pub first_seen_ms: u64,
    pub quiet_until_ms: u64,
    pub max_until_ms: u64,
    pub pr_notified: bool,
    pub last_generation: Option<Generation>,
    pub last_intent: Option<Intent>,
}

/// What one scheduler tick decides for a pending request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tick {
    /// Still inside the quiet window and the hard ceiling.
    Defer,
    /// Emit one notice for the newest observed generation.
    Notify(Box<Observation>),
    /// The newest generation is content-equivalent to the last notice against
    /// the same base; drop the request without notifying.
    Reuse,
}

impl Policy {
    /// Fold one observation into the lane/PR slot. A new generation resets the
    /// quiet deadline; the maximum wait is set once so a continuous push storm
    /// cannot starve the review.
    pub fn observe(&self, slot: &mut Option<Pending>, observation: Observation) {
        let now = observation.observed_ms;
        match slot {
            None => {
                *slot = Some(Pending {
                    observation,
                    first_seen_ms: now,
                    quiet_until_ms: now.saturating_add(self.quiet_ms),
                    max_until_ms: now.saturating_add(self.max_wait_ms),
                    pr_notified: false,
                    last_generation: None,
                    last_intent: None,
                });
            }
            Some(pending) => {
                pending.quiet_until_ms = now.saturating_add(self.quiet_ms);
                let upgraded = pending.observation.intent == Intent::Checkpoint
                    && observation.intent == Intent::Final;
                if upgraded {
                    pending.observation.intent = Intent::Final;
                }
                if pending.observation.url.is_none() {
                    pending.observation.url = observation.url.clone();
                }
                if pending.observation.title.is_none() {
                    pending.observation.title = observation.title.clone();
                }
                if observation.generation != pending.observation.generation || upgraded {
                    pending.observation.generation = observation.generation;
                }
            }
        }
    }

    /// Evaluate one slot at `now`. A decision other than `Defer` clears it.
    pub fn tick(&self, slot: &mut Option<Pending>, now: u64) -> Tick {
        let Some(pending) = slot.as_ref() else {
            return Tick::Defer;
        };
        if now < pending.quiet_until_ms && now < pending.max_until_ms {
            return Tick::Defer;
        }
        let desired = &pending.observation;
        if let (Some(last), Some(last_intent)) = (&pending.last_generation, pending.last_intent) {
            let intent_upgrade =
                last_intent == Intent::Checkpoint && desired.intent == Intent::Final;
            if !intent_upgrade && last.equivalent(&desired.generation) {
                *slot = None;
                return Tick::Reuse;
            }
        }
        let observation = desired.clone();
        *slot = None;
        Tick::Notify(Box::new(observation))
    }
}

/// One running review. Only a finalization whose generation equals the desired
/// generation may mark the request reviewed; a result for a superseded head is
/// ignored so the newest head is never labelled reviewed by an old result.
#[derive(Clone, Debug)]
pub struct ReviewRun {
    pub key: String,
    pub desired_generation: u64,
    pub reviewed_generation: Option<u64>,
}

impl ReviewRun {
    /// Returns true when this result finalizes the current desired generation.
    pub fn finalize(&mut self, generation: u64) -> bool {
        if generation != self.desired_generation {
            return false;
        }
        self.reviewed_generation = Some(generation);
        true
    }
}

// ---------------------------------------------------------------------------
// Cache boundary
// ---------------------------------------------------------------------------

/// One `change_log` row. Mirrors `ghcache_client::ChangeEvent` field for field.
#[derive(Clone, Debug, PartialEq)]
pub struct CacheEvent {
    pub id: i64,
    pub entity_type: String,
    pub entity_id: i64,
    pub event: String,
    pub repo_slug: Option<String>,
    pub payload: Value,
    pub occurred_at: String,
}

/// The row set this module reads. `ghcache-client` would implement this once
/// its sqlx dependency is feature-gated out of the build graph.
pub struct CacheReader {
    connection: Connection,
}

impl CacheReader {
    pub fn open(path: &Path) -> Result<CacheReader> {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("open ghcache db read-only at {}", path.display()))?;
        connection.busy_timeout(Duration::from_millis(500)).ok();
        Ok(CacheReader { connection })
    }

    /// New change rows after `last_id`, oldest first, bounded by `limit`.
    pub fn changes_since(&self, last_id: i64, limit: i64) -> Result<Vec<CacheEvent>> {
        let mut statement = self.connection.prepare(
            "SELECT id, entity_type, entity_id, event, repo_slug, payload_json, occurred_at
             FROM change_log WHERE id > ?1 ORDER BY id LIMIT ?2",
        )?;
        let rows = statement.query_map(params![last_id, limit], |row| {
            let payload: Option<String> = row.get(5)?;
            Ok(CacheEvent {
                id: row.get(0)?,
                entity_type: row.get(1)?,
                entity_id: row.get(2)?,
                event: row.get(3)?,
                repo_slug: row.get(4)?,
                payload: payload
                    .as_deref()
                    .and_then(|raw| serde_json::from_str(raw).ok())
                    .unwrap_or(Value::Null),
                occurred_at: row.get(6)?,
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// The branch row a `branch` change names.
    fn branch(&self, id: i64) -> Result<Option<BranchRow>> {
        self.connection
            .query_row(
                "SELECT r.owner || '/' || r.name, b.name, b.sha
                 FROM branch b JOIN repo r ON r.id = b.repo_id WHERE b.id = ?1",
                params![id],
                |row| {
                    Ok(BranchRow {
                        repo_slug: row.get(0)?,
                        name: row.get(1)?,
                        sha: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// The pull request row a `pull_request` change names.
    fn pull_request(&self, id: i64) -> Result<Option<PullRow>> {
        self.connection
            .query_row(
                "SELECT r.owner || '/' || r.name, pr.number, pr.head_ref, pr.head_sha,
                        pr.base_ref, pr.state, pr.title
                 FROM pull_request pr JOIN repo r ON r.id = pr.repo_id WHERE pr.id = ?1",
                params![id],
                |row| {
                    Ok(PullRow {
                        repo_slug: row.get(0)?,
                        number: row.get(1)?,
                        head_ref: row.get(2)?,
                        head_sha: row.get(3)?,
                        base_ref: row.get(4)?,
                        state: row.get(5)?,
                        title: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }
}

struct BranchRow {
    repo_slug: String,
    name: String,
    sha: Option<String>,
}

struct PullRow {
    repo_slug: String,
    number: i64,
    head_ref: Option<String>,
    head_sha: Option<String>,
    base_ref: Option<String>,
    state: String,
    title: String,
}

/// A lane and the cache identity it owns. `repo_slug`/`branch` are filled the
/// first time the cache reports the lane's worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneRepo {
    pub lane: String,
    pub cwd: String,
    pub repo_slug: String,
    pub branch: String,
    pub head: String,
    /// The base sha the spawn branched from; the comparison base for a
    /// checkpoint before a PR exists. Empty when unknown.
    pub base: String,
}

/// One correlated change ready to become a review request.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub lane: String,
    pub key: String,
    pub repo_slug: String,
    pub branch: String,
    pub pr_number: Option<i64>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub head: String,
    pub base: String,
    pub intent: Intent,
}

/// `owner/name` from a git remote URL, in the forms ghcache records.
pub fn slug_from_origin(origin: &str) -> Option<String> {
    let trimmed = origin.trim().trim_end_matches(".git");
    if let Some(rest) = trimmed.strip_prefix("git@") {
        // git@github.com:owner/name
        let (_, path) = rest.split_once(':')?;
        return normalize_slug(path);
    }
    if let Some((_, rest)) = trimmed.split_once("://") {
        // https://host/owner/name or ssh://git@host/owner/name
        let mut parts = rest.splitn(2, '/');
        let _host = parts.next()?;
        let path = parts.next()?;
        return normalize_slug(path);
    }
    None
}

fn normalize_slug(path: &str) -> Option<String> {
    let path = path.trim_start_matches('/');
    let mut parts = path.splitn(2, '/');
    let owner = parts.next().filter(|p| !p.is_empty())?;
    let name = parts.next().filter(|p| !p.is_empty())?;
    let name = name.split('/').next().unwrap_or(name);
    Some(format!("{owner}/{name}"))
}

/// Correlate one cache event to a lane. A `worktree` event teaches the
/// lane<->repo mapping and produces no review; `branch` and `pull_request`
/// changes correlate by repo+branch. An event for a repo/branch no lane owns
/// yields nothing, so unrelated existing PRs are never announced.
pub fn resolve_event(
    reader: &CacheReader,
    event: &CacheEvent,
    lanes: &[LaneRepo],
) -> Result<Option<Resolved>> {
    match event.entity_type.as_str() {
        "worktree" => Ok(None),
        "branch" => {
            let Some(branch) = reader.branch(event.entity_id)? else {
                return Ok(None);
            };
            let Some(sha) = branch.sha.filter(|sha| !sha.is_empty()) else {
                return Ok(None);
            };
            let Some(lane) = lane_for(lanes, &branch.repo_slug, &branch.name) else {
                return Ok(None);
            };
            let base = if lane.base.is_empty() {
                "HEAD".to_owned()
            } else {
                lane.base.clone()
            };
            Ok(Some(Resolved {
                lane: lane.lane.clone(),
                key: format!("{}:{}:{}", lane.lane, branch.repo_slug, branch.name),
                repo_slug: branch.repo_slug,
                branch: branch.name,
                pr_number: None,
                title: None,
                url: None,
                head: sha,
                base,
                intent: Intent::Checkpoint,
            }))
        }
        "pull_request" => {
            let Some(pull) = reader.pull_request(event.entity_id)? else {
                return Ok(None);
            };
            if pull.state != "open" {
                return Ok(None);
            }
            let head = pull.head_sha.filter(|sha| !sha.is_empty())?;
            let branch = pull.head_ref.clone().unwrap_or_default();
            if branch.is_empty() {
                return Ok(None);
            }
            let Some(lane) = lane_for(lanes, &pull.repo_slug, &branch) else {
                return Ok(None);
            };
            let url = format!("https://github.com/{}/pull/{}", pull.repo_slug, pull.number);
            Ok(Some(Resolved {
                lane: lane.lane.clone(),
                key: format!("{}:{}:{}", lane.lane, pull.repo_slug, branch),
                repo_slug: pull.repo_slug,
                branch,
                pr_number: Some(pull.number),
                title: Some(pull.title),
                url: Some(url),
                head,
                base: pull.base_ref.unwrap_or_else(|| "HEAD".to_owned()),
                intent: Intent::Final,
            }))
        }
        _ => Ok(None),
    }
}

fn lane_for<'a>(lanes: &'a [LaneRepo], repo_slug: &str, branch: &str) -> Option<&'a LaneRepo> {
    lanes
        .iter()
        .find(|lane| lane.repo_slug == repo_slug && lane.branch == branch)
}

/// Record the worktree identity the cache reported for a lane, by path.
pub fn learn_worktree(state: &ReviewState, event: &CacheEvent, lanes: &[LaneRepo]) -> Result<()> {
    let worktree = event
        .payload
        .get("worktree")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if worktree.is_empty() {
        return Ok(());
    }
    let origin = event
        .payload
        .get("origin")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let branch = event
        .payload
        .get("branch")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let head = event
        .payload
        .get("head")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(slug) = slug_from_origin(origin) else {
        return Ok(());
    };
    for lane in lanes {
        if same_path(&lane.cwd, worktree) {
            state.set_lane_repo(&lane.lane, &slug, branch, head)?;
        }
    }
    Ok(())
}

fn same_path(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let canonical = |value: &str| std::fs::canonicalize(value).ok();
    match (canonical(a), canonical(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Content fingerprint
// ---------------------------------------------------------------------------

/// Read the proposed content of `head` against `base` from the lane's own
/// checkout. Returns the generation with `fingerprint`/`base_context` set when
/// git could read both; `None` fields mean equivalence can never be claimed.
pub fn fingerprint(cwd: &Path, base: &str, head: &str) -> Generation {
    let base_context = git(cwd, &["merge-base", base, head]);
    let raw = git(cwd, &["diff", "--raw", "-M", base, head]);
    let patch = git(cwd, &["diff", "--binary", "-M", base, head]);
    let fingerprint = match (base_context.as_deref(), raw.as_deref(), patch.as_deref()) {
        (Some(context), Some(raw), Some(patch)) => {
            let mut hasher = DefaultHasher::new();
            context.hash(&mut hasher);
            raw.hash(&mut hasher);
            patch.hash(&mut hasher);
            Some(format!("{:016x}", hasher.finish()))
        }
        _ => None,
    };
    Generation {
        head: head.to_owned(),
        base: base.to_owned(),
        base_context,
        fingerprint,
    }
}

fn git(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

// ---------------------------------------------------------------------------
// Durable state
// ---------------------------------------------------------------------------

const STATE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS source_cursor (
  source TEXT PRIMARY KEY,
  last_change_id INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS seen_event (
  source TEXT NOT NULL,
  event_id INTEGER NOT NULL,
  PRIMARY KEY (source, event_id)
);
CREATE TABLE IF NOT EXISTS lane_repo (
  lane TEXT PRIMARY KEY,
  cwd TEXT NOT NULL,
  repo_slug TEXT NOT NULL DEFAULT '',
  branch TEXT NOT NULL DEFAULT '',
  head TEXT NOT NULL DEFAULT '',
  base TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS review_request (
  key TEXT PRIMARY KEY,
  lane TEXT NOT NULL,
  repo_slug TEXT NOT NULL,
  branch TEXT NOT NULL,
  pr_number INTEGER,
  title TEXT,
  url TEXT,
  first_seen_ms INTEGER NOT NULL,
  quiet_until_ms INTEGER NOT NULL,
  max_until_ms INTEGER NOT NULL,
  head TEXT NOT NULL,
  base TEXT NOT NULL,
  base_context TEXT,
  fingerprint TEXT,
  intent TEXT NOT NULL,
  last_head TEXT,
  last_base TEXT,
  last_base_context TEXT,
  last_fingerprint TEXT,
  last_intent TEXT,
  pr_notified INTEGER NOT NULL DEFAULT 0,
  review_generation INTEGER NOT NULL DEFAULT 0,
  desired_generation INTEGER NOT NULL DEFAULT 0,
  reviewed_generation INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS review_claim (
  key TEXT NOT NULL,
  generation TEXT NOT NULL,
  intent TEXT NOT NULL,
  at_ms INTEGER NOT NULL,
  PRIMARY KEY (key, generation, intent)
);
";

/// The review database beside the mailbox. A separate file keeps the adapter's
/// state out of the transcript/store schema while still persisting across
/// restarts.
pub struct ReviewState {
    path: PathBuf,
}

impl ReviewState {
    pub fn beside_mail_dir(mail_dir: &Path) -> Result<ReviewState> {
        let state = ReviewState {
            path: mail_dir.join("review.db"),
        };
        state.init()?;
        Ok(state)
    }

    fn connection(&self) -> Result<Connection> {
        let connection = Connection::open(&self.path)
            .with_context(|| format!("open review.db at {}", self.path.display()))?;
        connection.busy_timeout(Duration::from_secs(5)).ok();
        Ok(connection)
    }

    fn init(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        self.connection()?
            .execute_batch(STATE_SCHEMA)
            .context("initialise review.db schema")?;
        Ok(())
    }

    // -- source cursor / replay dedup --------------------------------------

    pub fn cursor(&self) -> Result<i64> {
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT last_change_id FROM source_cursor WHERE source = ?1",
                params![CURSOR_SOURCE],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    pub fn set_cursor(&self, id: i64) -> Result<()> {
        self.connection()?.execute(
            "INSERT INTO source_cursor (source, last_change_id) VALUES (?1, ?2)
             ON CONFLICT(source) DO UPDATE SET last_change_id = excluded.last_change_id",
            params![CURSOR_SOURCE, id],
        )?;
        Ok(())
    }

    pub fn seen_event(&self, event_id: i64) -> Result<bool> {
        let connection = self.connection()?;
        let seen: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM seen_event WHERE source = ?1 AND event_id = ?2)",
            params![CURSOR_SOURCE, event_id],
            |row| row.get(0),
        )?;
        Ok(seen)
    }

    pub fn mark_event(&self, event_id: i64) -> Result<()> {
        self.connection()?.execute(
            "INSERT OR IGNORE INTO seen_event (source, event_id) VALUES (?1, ?2)",
            params![CURSOR_SOURCE, event_id],
        )?;
        Ok(())
    }

    // -- lane <-> repo -----------------------------------------------------

    pub fn register_lane(&self, lane: &str, cwd: &str) -> Result<()> {
        self.connection()?.execute(
            "INSERT INTO lane_repo (lane, cwd) VALUES (?1, ?2)
             ON CONFLICT(lane) DO UPDATE SET cwd = excluded.cwd",
            params![lane, cwd],
        )?;
        Ok(())
    }

    pub fn set_lane_repo(
        &self,
        lane: &str,
        repo_slug: &str,
        branch: &str,
        head: &str,
    ) -> Result<()> {
        self.connection()?.execute(
            "UPDATE lane_repo SET repo_slug = ?2, branch = ?3, head = ?4 WHERE lane = ?1",
            params![lane, repo_slug, branch, head],
        )?;
        Ok(())
    }

    pub fn set_lane_base(&self, lane: &str, base: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE lane_repo SET base = ?2 WHERE lane = ?1",
            params![lane, base],
        )?;
        Ok(())
    }

    pub fn lanes(&self) -> Result<Vec<LaneRepo>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT lane, cwd, repo_slug, branch, head, base FROM lane_repo ORDER BY lane",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(LaneRepo {
                lane: row.get(0)?,
                cwd: row.get(1)?,
                repo_slug: row.get(2)?,
                branch: row.get(3)?,
                head: row.get(4)?,
                base: row.get(5)?,
            })
        })?;
        let mut lanes = Vec::new();
        for row in rows {
            lanes.push(row?);
        }
        Ok(lanes)
    }

    // -- pending requests --------------------------------------------------

    pub fn load_pending(&self) -> Result<Vec<Pending>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT key, lane, repo_slug, branch, pr_number, title, url,
                    first_seen_ms, quiet_until_ms, max_until_ms, head, base,
                    base_context, fingerprint, intent, last_head, last_base,
                    last_base_context, last_fingerprint, last_intent, pr_notified
             FROM review_request",
        )?;
        let rows = statement.query_map([], |row| {
            let generation = Generation {
                head: row.get(10)?,
                base: row.get(11)?,
                base_context: row.get(12)?,
                fingerprint: row.get(13)?,
            };
            let last_generation = match row.get::<_, Option<String>>(15)? {
                Some(head) => Some(Generation {
                    head,
                    base: row.get::<_, Option<String>>(16)?.unwrap_or_default(),
                    base_context: row.get(17)?,
                    fingerprint: row.get(18)?,
                }),
                None => None,
            };
            let observation = Observation {
                lane: row.get(1)?,
                key: row.get(0)?,
                repo_slug: row.get(2)?,
                branch: row.get(3)?,
                generation,
                intent: Intent::from_str(&row.get::<_, String>(14)?),
                url: row.get(6)?,
                title: row.get(5)?,
                observed_ms: row.get(7)?,
            };
            Ok(Pending {
                observation,
                first_seen_ms: row.get(7)?,
                quiet_until_ms: row.get(8)?,
                max_until_ms: row.get(9)?,
                pr_notified: row.get::<_, i64>(20)? != 0,
                last_generation,
                last_intent: row
                    .get::<_, Option<String>>(19)?
                    .map(|value| Intent::from_str(&value)),
            })
        })?;
        let mut pending = Vec::new();
        for row in rows {
            pending.push(row?);
        }
        Ok(pending)
    }

    pub fn save_pending(&self, pending: &Pending) -> Result<()> {
        let observation = &pending.observation;
        let last = pending.last_generation.as_ref();
        self.connection()?.execute(
            "INSERT INTO review_request
               (key, lane, repo_slug, branch, pr_number, title, url,
                first_seen_ms, quiet_until_ms, max_until_ms, head, base,
                base_context, fingerprint, intent, last_head, last_base,
                last_base_context, last_fingerprint, last_intent, pr_notified)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                     ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
             ON CONFLICT(key) DO UPDATE SET
               lane = excluded.lane, repo_slug = excluded.repo_slug,
               branch = excluded.branch, pr_number = excluded.pr_number,
               title = excluded.title, url = excluded.url,
               first_seen_ms = excluded.first_seen_ms,
               quiet_until_ms = excluded.quiet_until_ms,
               max_until_ms = excluded.max_until_ms, head = excluded.head,
               base = excluded.base, base_context = excluded.base_context,
               fingerprint = excluded.fingerprint, intent = excluded.intent,
               last_head = excluded.last_head, last_base = excluded.last_base,
               last_base_context = excluded.last_base_context,
               last_fingerprint = excluded.last_fingerprint,
               last_intent = excluded.last_intent, pr_notified = excluded.pr_notified",
            params![
                observation.key,
                observation.lane,
                observation.repo_slug,
                observation.branch,
                observation.generation.head,
                observation.title,
                observation.url,
                pending.first_seen_ms as i64,
                pending.quiet_until_ms as i64,
                pending.max_until_ms as i64,
                observation.generation.head,
                observation.generation.base,
                observation.generation.base_context,
                observation.generation.fingerprint,
                observation.intent.as_str(),
                last.map(|g| g.head.clone()),
                last.map(|g| g.base.clone()),
                last.and_then(|g| g.base_context.clone()),
                last.and_then(|g| g.fingerprint.clone()),
                pending.last_intent.map(Intent::as_str),
                pending.pr_notified as i64,
            ],
        )?;
        Ok(())
    }

    pub fn remove_pending(&self, key: &str) -> Result<()> {
        self.connection()?
            .execute("DELETE FROM review_request WHERE key = ?1", params![key])?;
        Ok(())
    }

    // -- notification claims ----------------------------------------------

    /// Claim one `(key, generation, intent)` notice. `true` means this caller
    /// appends the rows; a replay claims nothing.
    pub fn claim_notice(&self, key: &str, generation: &str, intent: Intent) -> Result<bool> {
        let inserted = self.connection()?.execute(
            "INSERT OR IGNORE INTO review_claim (key, generation, intent, at_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![key, generation, intent.as_str(), now_ms() as i64],
        )?;
        Ok(inserted == 1)
    }

    // -- review generations ------------------------------------------------

    /// Advance the desired generation for a key. Returns the new generation
    /// number. A running review keeps its `reviewed_generation`.
    pub fn advance_generation(&self, key: &str) -> Result<u64> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE review_request
             SET desired_generation = desired_generation + 1,
                 review_generation = review_generation + 1
             WHERE key = ?1",
            params![key],
        )?;
        Ok(connection
            .query_row(
                "SELECT desired_generation FROM review_request WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// Record a finalization only when it matches the desired generation.
    pub fn finalize_review(&self, key: &str, generation: u64) -> Result<bool> {
        let changed = self.connection()?.execute(
            "UPDATE review_request SET reviewed_generation = ?2
             WHERE key = ?1 AND desired_generation = ?2",
            params![key, generation as i64],
        )?;
        Ok(changed > 0)
    }

    pub fn reviewed_generation(&self, key: &str) -> Result<u64> {
        Ok(self
            .connection()?
            .query_row(
                "SELECT reviewed_generation FROM review_request WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }
}

// ---------------------------------------------------------------------------
// Notification emission
// ---------------------------------------------------------------------------

/// Append a review-update notice to every subscriber. Distinct from
/// `Store::notify_pr`, whose URL claim is a creation notice only and must not
/// be reused as a generic update key.
pub fn notify_review_update(
    store: &Store,
    lane: &str,
    url: Option<&str>,
    body_head: &str,
) -> Result<Vec<bus::Message>> {
    let mut rows = Vec::new();
    let body = match url {
        Some(url) => format!("pr update {lane} {url} head={body_head}\n review: gh pr diff {url}"),
        None => format!("push update {lane} head={body_head}\n review: git show {body_head}"),
    };
    for subscriber in bus::lane_subscribers(store, lane) {
        let row = bus::Message {
            id: bus::mint_id(),
            from: lane.to_owned(),
            to: subscriber,
            from_timestamp: bus::now_iso(),
            to_timestamp: None,
            kind: MessageKind::Pr,
            reply_to: None,
            body: body.clone(),
            r#ref: None,
            rc: None,
            detail: None,
        };
        bus::append_message(store, "bus", &row, "review update")?;
        rows.push(row);
    }
    Ok(rows)
}

fn deliver(store: &Store, row: &bus::Message) {
    let Ok(routes) = bus::routes_in(store) else {
        return;
    };
    let registry = boop_harness::registry::Registry::discover();
    match crate::deliver::deliver_hail(&registry, store, &routes, row) {
        Ok(_) => debug!(message_id = row.id, "review notice delivered"),
        Err(error) => warn!(message_id = row.id, error = %error, "review notice delivery failed"),
    }
}

// ---------------------------------------------------------------------------
// Shared consumer
// ---------------------------------------------------------------------------

fn consumers() -> &'static Mutex<HashSet<PathBuf>> {
    static CONSUMERS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    CONSUMERS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Register a lane's worktree and make sure the one shared consumer for this
/// mailbox is running. Safe to call on every supervisor start.
pub fn observe_lane(mail_dir: &Path, lane: &str, cwd: &Path) {
    if let Ok(state) = ReviewState::beside_mail_dir(mail_dir) {
        if let Err(error) = state.register_lane(lane, &cwd.to_string_lossy()) {
            warn!(lane, error = %error, "review lane registration failed");
            return;
        }
        // Fill the repo+branch now when the checkout can name them; the cache's
        // worktree change remains the authority and overwrites this later.
        if let Some((slug, branch, head)) = probe_lane_repo(cwd) {
            let _ = state.set_lane_repo(lane, &slug, &branch, &head);
        }
        // The spawn's base sha is the comparison base for a checkpoint before a
        // PR exists. The route holds it.
        if let Ok(store) = bus::open_store(mail_dir) {
            if let Ok(routes) = bus::routes_in(&store) {
                if let Some(base) = routes.get(lane).and_then(|route| route.base_sha.clone()) {
                    let _ = state.set_lane_base(lane, &base);
                }
            }
        }
    }
    ensure_consumer(mail_dir);
}

fn ensure_consumer(mail_dir: &Path) {
    // No cache is configured: hold in the explicit missing-cache state rather
    // than starting a thread that polls nothing.
    if cache_db_path().is_none() {
        debug!("GHCACHE_DB unset; cached review notices hold");
        return;
    }
    let mut started = consumers().lock().expect("review consumer set");
    if !started.insert(mail_dir.to_path_buf()) {
        return;
    }
    let dir = mail_dir.to_path_buf();
    std::thread::Builder::new()
        .name("boop-review".to_owned())
        .spawn(move || consumer_loop(dir))
        .map(|_| info!(dir = %mail_dir.display(), "review consumer started"))
        .unwrap_or_else(|error| {
            warn!(error = %error, "review consumer start failed");
        });
}

/// Where the ghcache database is. `GHCACHE_DB` is required so a supervisor
/// never reads a database the user did not point at; the documented
/// `ghcache db-path` value exports this. Absence is an explicit bounded state,
/// never a `gh` fallback.
fn cache_db_path() -> Option<PathBuf> {
    std::env::var_os("GHCACHE_DB")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn consumer_loop(mail_dir: PathBuf) {
    let state = match ReviewState::beside_mail_dir(&mail_dir) {
        Ok(state) => state,
        Err(error) => {
            warn!(error = %error, "review state open failed; consumer stopping");
            return;
        }
    };
    let mut slots: std::collections::HashMap<String, Option<Pending>> =
        std::collections::HashMap::new();
    let mut missing_logged = false;
    loop {
        let cache_path = cache_db_path();
        let reader = match cache_path {
            Some(path) if path.exists() => match CacheReader::open(&path) {
                Ok(reader) => {
                    missing_logged = false;
                    Some(reader)
                }
                Err(error) => {
                    if !missing_logged {
                        warn!(error = %error, "ghcache unreadable; review notices hold");
                        missing_logged = true;
                    }
                    None
                }
            },
            _ => {
                if !missing_logged {
                    info!("no ghcache database; review notices hold");
                    missing_logged = true;
                }
                None
            }
        };
        let Some(reader) = reader else {
            std::thread::sleep(MISSING_CACHE_BACKOFF);
            continue;
        };

        if let Err(error) = pump(&reader, &state, &mail_dir, &mut slots) {
            debug!(error = %error, "review pump pass failed");
        }
        std::thread::sleep(POLL);
    }
}

/// One pass: read the new change rows, learn lane worktrees, observe review
/// requests, then tick the scheduler and emit at most one notice per slot.
fn pump(
    reader: &CacheReader,
    state: &ReviewState,
    mail_dir: &Path,
    slots: &mut std::collections::HashMap<String, Option<Pending>>,
) -> Result<()> {
    let policy = Policy::from_env();
    // Recover pending requests a previous process left before this pass. A
    // request already in memory keeps its earliest deadlines.
    for pending in state.load_pending()? {
        slots
            .entry(pending.observation.key.clone())
            .or_insert(Some(pending));
    }
    let cursor = state.cursor()?;
    let events = reader.changes_since(cursor, BATCH)?;
    let mut lanes = state.lanes()?;
    let mut highest = cursor;
    for event in &events {
        highest = highest.max(event.id);
        if state.seen_event(event.id)? {
            continue;
        }
        if event.entity_type == "worktree" {
            learn_worktree(state, event, &lanes)?;
            lanes = state.lanes()?;
        } else if let Some(resolved) = resolve_event(reader, event, &lanes)? {
            let cwd = lane_cwd(&lanes, &resolved.lane);
            let generation = fingerprint(Path::new(&cwd), &resolved.base, &resolved.head);
            let observation = Observation {
                lane: resolved.lane.clone(),
                key: resolved.key.clone(),
                repo_slug: resolved.repo_slug.clone(),
                branch: resolved.branch.clone(),
                generation,
                intent: resolved.intent,
                url: resolved.url.clone(),
                title: resolved.title.clone(),
                observed_ms: now_ms(),
            };
            let slot = slots.entry(resolved.key.clone()).or_insert(None);
            policy.observe(slot, observation);
            if let Some(pending) = slot.as_ref() {
                state.save_pending(pending)?;
            }
        }
        state.mark_event(event.id)?;
    }
    if highest > cursor {
        state.set_cursor(highest)?;
    }

    // Tick every slot, including ones reloaded by a previous restart.
    let now = now_ms();
    for (key, slot) in slots.iter_mut() {
        match policy.tick(slot, now) {
            Tick::Defer => {}
            Tick::Reuse => {
                state.remove_pending(key)?;
            }
            Tick::Notify(observation) => {
                if let Err(error) = emit(state, mail_dir, &observation) {
                    warn!(key = %observation.key, error = %error, "review notice emit failed");
                }
                state.remove_pending(key)?;
            }
        }
    }
    Ok(())
}

fn lane_cwd(lanes: &[LaneRepo], lane: &str) -> String {
    lanes
        .iter()
        .find(|entry| entry.lane == lane)
        .map(|entry| entry.cwd.clone())
        .unwrap_or_default()
}

fn emit(state: &ReviewState, mail_dir: &Path, observation: &Observation) -> Result<()> {
    let store = bus::open_store(mail_dir)?;
    let token = observation
        .generation
        .fingerprint
        .clone()
        .unwrap_or_else(|| observation.generation.head.clone());
    let url = observation.url.as_deref();
    // The PR-creation notice is claimed once per key under a sentinel token.
    // Every later head update uses the review-update path, so the URL claim
    // cannot suppress a genuinely new head on the same PR.
    let creation = observation.intent == Intent::Final && url.is_some();
    if creation && state.claim_notice(&observation.key, "pr-created", Intent::Final)? {
        let rows = store.notify_pr(
            &observation.lane,
            url.expect("final intent carries a url"),
            observation.title.as_deref(),
        )?;
        for row in &rows {
            deliver(&store, row);
        }
    } else if state.claim_notice(&observation.key, &token, observation.intent)? {
        let rows =
            notify_review_update(&store, &observation.lane, url, &observation.generation.head)?;
        for row in &rows {
            deliver(&store, row);
        }
    }
    Ok(())
}

/// Resolve a lane's repo+branch from its own checkout. Used when the cache
/// reported no worktree event yet.
pub fn probe_lane_repo(cwd: &Path) -> Option<(String, String, String)> {
    let branch = git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let origin = git(cwd, &["config", "--get", "remote.origin.url"])?;
    let head = git(cwd, &["rev-parse", "HEAD"])?;
    let slug = slug_from_origin(&origin)?;
    Some((slug, branch, head))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generation(head: &str, fingerprint: &str, base_context: &str) -> Generation {
        Generation {
            head: head.to_owned(),
            base: "main".to_owned(),
            base_context: Some(base_context.to_owned()),
            fingerprint: Some(fingerprint.to_owned()),
        }
    }

    fn observation(
        head: &str,
        fingerprint: &str,
        base_context: &str,
        intent: Intent,
        at: u64,
    ) -> Observation {
        Observation {
            lane: "mine".to_owned(),
            key: "mine:o/n:feature".to_owned(),
            repo_slug: "o/n".to_owned(),
            branch: "feature".to_owned(),
            generation: generation(head, fingerprint, base_context),
            intent,
            url: None,
            title: None,
            observed_ms: at,
        }
    }

    fn policy() -> Policy {
        Policy {
            quiet_ms: 1000,
            max_wait_ms: 5000,
        }
    }

    #[test]
    fn scheduler_coalesces_a_push_burst_to_the_newest_head() {
        let policy = policy();
        let mut slot = None;
        policy.observe(
            &mut slot,
            observation("a", "f1", "ctx", Intent::Checkpoint, 0),
        );
        policy.observe(
            &mut slot,
            observation("b", "f2", "ctx", Intent::Checkpoint, 200),
        );
        policy.observe(
            &mut slot,
            observation("c", "f3", "ctx", Intent::Checkpoint, 400),
        );
        assert_eq!(policy.tick(&mut slot, 900), Tick::Defer);
        match policy.tick(&mut slot, 1400) {
            Tick::Notify(observation) => assert_eq!(observation.generation.head, "c"),
            other => panic!("expected one notify for the newest head, got {other:?}"),
        }
        assert!(slot.is_none());
    }

    #[test]
    fn max_wait_bounds_a_continuous_push_stream() {
        let policy = policy();
        let mut slot = None;
        for ofs in (0..10_000).step_by(400) {
            policy.observe(
                &mut slot,
                observation("h", &format!("f{ofs}"), "ctx", Intent::Checkpoint, ofs),
            );
            let tick = policy.tick(&mut slot, ofs);
            if ofs < 5000 {
                assert_eq!(tick, Tick::Defer, "quiet window must not expire early");
            } else {
                assert!(matches!(tick, Tick::Notify(_)), "max wait must fire");
                break;
            }
        }
    }

    #[test]
    fn equivalent_generation_reuses_the_review() {
        let policy = policy();
        let mut slot = None;
        policy.observe(
            &mut slot,
            observation("a", "same", "ctx", Intent::Checkpoint, 0),
        );
        let mut pending = slot.take();
        let first = policy.tick(&mut pending, 1100);
        assert!(matches!(first, Tick::Notify(_)));

        // Record the notice, then a rebase lands an equivalent content change.
        let mut slot = pending;
        policy.observe(
            &mut slot,
            observation("a", "same", "ctx", Intent::Checkpoint, 2000),
        );
        // The notifier is responsible for recording last_notified after a tick.
        if let Some(pending) = slot.as_mut() {
            pending.last_generation = Some(generation("a", "same", "ctx"));
            pending.last_intent = Some(Intent::Checkpoint);
        }
        assert_eq!(policy.tick(&mut slot, 3100), Tick::Reuse);
    }

    #[test]
    fn base_movement_requires_a_new_review() {
        let policy = policy();
        let mut slot = None;
        policy.observe(
            &mut slot,
            observation("a", "same", "ctx-old", Intent::Checkpoint, 0),
        );
        policy.tick(&mut slot, 1100);
        policy.observe(
            &mut slot,
            observation("a", "same", "ctx-new", Intent::Checkpoint, 2000),
        );
        if let Some(pending) = slot.as_mut() {
            pending.last_generation = Some(generation("a", "same", "ctx-old"));
            pending.last_intent = Some(Intent::Checkpoint);
        }
        assert!(matches!(policy.tick(&mut slot, 3100), Tick::Notify(_)));
    }

    #[test]
    fn changed_conflict_resolution_requires_a_new_review() {
        let policy = policy();
        let mut slot = None;
        policy.observe(
            &mut slot,
            observation("a", "before", "ctx", Intent::Checkpoint, 0),
        );
        policy.tick(&mut slot, 1100);
        policy.observe(
            &mut slot,
            observation("a", "after", "ctx", Intent::Checkpoint, 2000),
        );
        if let Some(pending) = slot.as_mut() {
            pending.last_generation = Some(generation("a", "before", "ctx"));
            pending.last_intent = Some(Intent::Checkpoint);
        }
        assert!(matches!(policy.tick(&mut slot, 3100), Tick::Notify(_)));
    }

    #[test]
    fn pr_after_checkpoint_upgrades_final_intent_once() {
        let policy = policy();
        let mut slot = None;
        policy.observe(
            &mut slot,
            observation("a", "same", "ctx", Intent::Checkpoint, 0),
        );
        policy.tick(&mut slot, 1100);
        policy.observe(&mut slot, {
            let mut obs = observation("a", "same", "ctx", Intent::Final, 2000);
            obs.url = Some("https://github.com/o/n/pull/1".to_owned());
            obs
        });
        if let Some(pending) = slot.as_mut() {
            pending.last_generation = Some(generation("a", "same", "ctx"));
            pending.last_intent = Some(Intent::Checkpoint);
        }
        match policy.tick(&mut slot, 3100) {
            Tick::Notify(observation) => assert_eq!(observation.intent, Intent::Final),
            other => panic!("final intent must upgrade the checkpoint, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_source_event_does_not_renotify() {
        // A second, identical observation after the notice is equivalent
        // against an unchanged base and is dropped.
        let policy = policy();
        let mut slot = None;
        policy.observe(&mut slot, observation("a", "same", "ctx", Intent::Final, 0));
        if let Some(pending) = slot.as_mut() {
            pending.last_generation = Some(generation("a", "same", "ctx"));
            pending.last_intent = Some(Intent::Final);
        }
        assert_eq!(policy.tick(&mut slot, 1100), Tick::Reuse);
    }

    #[test]
    fn stale_finalization_does_not_mark_the_newest_head_reviewed() {
        let mut run = ReviewRun {
            key: "mine:o/n:1".to_owned(),
            desired_generation: 2,
            reviewed_generation: None,
        };
        assert!(!run.finalize(1));
        assert_eq!(run.reviewed_generation, None);
        assert!(run.finalize(2));
        assert_eq!(run.reviewed_generation, Some(2));
    }

    #[test]
    fn unrelated_repo_or_branch_is_not_correlated() {
        let lanes = vec![LaneRepo {
            lane: "mine".to_owned(),
            cwd: "/tmp/mine".to_owned(),
            repo_slug: "o/n".to_owned(),
            branch: "feature".to_owned(),
            head: String::new(),
            base: String::new(),
        }];
        assert!(lane_for(&lanes, "o/other", "feature").is_none());
        assert!(lane_for(&lanes, "o/n", "main").is_none());
        assert_eq!(lane_for(&lanes, "o/n", "feature").unwrap().lane, "mine");
    }

    #[test]
    fn origin_urls_parse_to_a_slug() {
        assert_eq!(
            slug_from_origin("git@github.com:o/n.git").as_deref(),
            Some("o/n")
        );
        assert_eq!(
            slug_from_origin("https://github.com/o/n.git").as_deref(),
            Some("o/n")
        );
        assert_eq!(
            slug_from_origin("ssh://git@github.com/o/n").as_deref(),
            Some("o/n")
        );
        assert!(slug_from_origin("not-a-url").is_none());
    }

    // -- real git ----------------------------------------------------------

    fn scratch_repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "boop-review-{}-{}-{name}",
            std::process::id(),
            now_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
        ] {
            let status = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(&args)
                .status()
                .unwrap();
            assert!(status.success());
        }
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        let run = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        };
        run(&["add", "-A"]);
        run(&["commit", "-qm", "seed"]);
        dir
    }

    fn head(cwd: &Path) -> String {
        git(cwd, &["rev-parse", "HEAD"]).unwrap()
    }

    #[test]
    fn rewrite_with_identical_content_is_equivalent() {
        let dir = scratch_repo("identical");
        let base = head(&dir);
        std::fs::write(dir.join("b.txt"), "b\n").unwrap();
        let run = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        };
        run(&["add", "-A"]);
        run(&["commit", "-qm", "change"]);
        let first = fingerprint(&dir, &base, &head(&dir));
        // Amend the message only: new SHA, identical tree and diff.
        run(&["commit", "--amend", "-qm", "change reworded"]);
        let second = fingerprint(&dir, &base, &head(&dir));
        assert_ne!(first.head, second.head);
        assert!(first.fingerprint.is_some());
        assert!(
            first.equivalent(&second),
            "same content, same base reuses review"
        );
    }

    #[test]
    fn changed_content_fingerprint_differs() {
        let dir = scratch_repo("changed");
        let base = head(&dir);
        let run = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        };
        std::fs::write(dir.join("b.txt"), "b\n").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-qm", "change"]);
        let first = fingerprint(&dir, &base, &head(&dir));
        std::fs::write(dir.join("b.txt"), "different\n").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "--amend", "-qm", "change amended"]);
        let second = fingerprint(&dir, &base, &head(&dir));
        assert!(!first.equivalent(&second));
    }

    // -- real sqlite + store ----------------------------------------------

    const GHCACHE_TEST_SCHEMA: &str = "
CREATE TABLE repo (id INTEGER PRIMARY KEY, owner TEXT, name TEXT);
CREATE TABLE branch (id INTEGER PRIMARY KEY, repo_id INTEGER, name TEXT, sha TEXT);
CREATE TABLE pull_request (id INTEGER PRIMARY KEY, repo_id INTEGER, number INTEGER,
  state TEXT, title TEXT, head_ref TEXT, head_sha TEXT, base_ref TEXT);
CREATE TABLE change_log (id INTEGER PRIMARY KEY AUTOINCREMENT, entity_type TEXT,
  entity_id INTEGER, event TEXT, repo_slug TEXT, payload_json TEXT, occurred_at TEXT);
";

    fn scratch_cache() -> (PathBuf, Connection) {
        let path = std::env::temp_dir().join(format!(
            "boop-review-cache-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = std::fs::remove_file(&path);
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(GHCACHE_TEST_SCHEMA).unwrap();
        (path, connection)
    }

    #[test]
    fn cache_event_correlates_only_the_owning_lane() {
        let (path, connection) = scratch_cache();
        connection
            .execute(
                "INSERT INTO repo (id, owner, name) VALUES (1, 'o', 'n')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO branch (id, repo_id, name, sha) VALUES (7, 1, 'feature', 'abc123')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO pull_request (id, repo_id, number, state, title, head_ref, head_sha, base_ref)
                 VALUES (9, 1, 42, 'open', 'My PR', 'feature', 'abc123', 'main')",
                [],
            )
            .unwrap();
        let reader = CacheReader::open(&path).unwrap();
        let lanes = vec![LaneRepo {
            lane: "mine".to_owned(),
            cwd: "/tmp/mine".to_owned(),
            repo_slug: "o/n".to_owned(),
            branch: "feature".to_owned(),
            head: String::new(),
            base: String::new(),
        }];
        let push = CacheEvent {
            id: 1,
            entity_type: "branch".to_owned(),
            entity_id: 7,
            event: "updated".to_owned(),
            repo_slug: Some("o/n".to_owned()),
            payload: Value::Null,
            occurred_at: String::new(),
        };
        let resolved = resolve_event(&reader, &push, &lanes).unwrap().unwrap();
        assert_eq!(resolved.intent, Intent::Checkpoint);
        assert_eq!(resolved.head, "abc123");

        let pr = CacheEvent {
            id: 2,
            entity_type: "pull_request".to_owned(),
            entity_id: 9,
            event: "inserted".to_owned(),
            repo_slug: Some("o/n".to_owned()),
            payload: Value::Null,
            occurred_at: String::new(),
        };
        let resolved = resolve_event(&reader, &pr, &lanes).unwrap().unwrap();
        assert_eq!(resolved.intent, Intent::Final);
        assert_eq!(
            resolved.url.as_deref(),
            Some("https://github.com/o/n/pull/42")
        );

        let unrelated = vec![LaneRepo {
            lane: "other".to_owned(),
            cwd: "/tmp/other".to_owned(),
            repo_slug: "o/other".to_owned(),
            branch: "feature".to_owned(),
            head: String::new(),
            base: String::new(),
        }];
        assert!(resolve_event(&reader, &pr, &unrelated).unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cursor_and_notice_claims_survive_reopen() {
        let dir = std::env::temp_dir().join(format!(
            "boop-review-state-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        {
            let state = ReviewState::beside_mail_dir(&dir).unwrap();
            state.set_cursor(99).unwrap();
            assert!(state.claim_notice("k", "head1", Intent::Final).unwrap());
        }
        let reopened = ReviewState::beside_mail_dir(&dir).unwrap();
        assert_eq!(reopened.cursor().unwrap(), 99);
        assert!(!reopened.claim_notice("k", "head1", Intent::Final).unwrap());
        assert!(reopened.claim_notice("k", "head2", Intent::Final).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replay_marks_events_seen_across_reopen() {
        let dir = std::env::temp_dir().join(format!(
            "boop-review-replay-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        {
            let state = ReviewState::beside_mail_dir(&dir).unwrap();
            assert!(!state.seen_event(5).unwrap());
            state.mark_event(5).unwrap();
        }
        let reopened = ReviewState::beside_mail_dir(&dir).unwrap();
        assert!(reopened.seen_event(5).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pending_round_trips_through_review_db() {
        let dir = std::env::temp_dir().join(format!(
            "boop-review-pending-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let state = ReviewState::beside_mail_dir(&dir).unwrap();
        let observation = observation("head1", "fp", "ctx", Intent::Final, 10);
        let pending = Pending {
            observation,
            first_seen_ms: 10,
            quiet_until_ms: 1010,
            max_until_ms: 5010,
            pr_notified: false,
            last_generation: Some(generation("prev", "old", "ctx")),
            last_intent: Some(Intent::Checkpoint),
        };
        state.save_pending(&pending).unwrap();
        let loaded = state.load_pending().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].observation.key, "mine:o/n:feature");
        assert_eq!(loaded[0].observation.repo_slug, "o/n");
        assert_eq!(
            loaded[0].observation.generation.fingerprint.as_deref(),
            Some("fp")
        );
        assert_eq!(loaded[0].max_until_ms, 5010);
        assert_eq!(loaded[0].last_generation.as_ref().unwrap().head, "prev");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pump_notifies_the_parent_from_a_real_cache_change() {
        // Real scratch git checkout, real boop store, real ghcache-shaped
        // SQLite. One branch change must reach the lane's registered parent.
        let root = std::env::temp_dir().join(format!(
            "boop-review-pump-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let repo = scratch_repo("pump");
        let git_run = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        };
        git_run(&["checkout", "-qb", "feature"]);
        let base_sha = head(&repo);
        std::fs::write(repo.join("c.txt"), "c\n").unwrap();
        git_run(&["add", "-A"]);
        git_run(&["commit", "-qm", "feature work"]);
        let head_sha = head(&repo);

        let mail_dir = root.join("mail");
        std::fs::create_dir_all(&mail_dir).unwrap();
        let store = bus::open_store(&mail_dir).unwrap();
        bus::write_route(
            &mail_dir,
            "mine",
            &bus::Route {
                kind: bus::RouteKind::Lane,
                harness: None,
                tmux: None,
                cwd: Some(repo.display().to_string()),
                model: None,
                mode: None,
                session_id: None,
                source_path: None,
                parent: Some("parent".to_owned()),
                goal: None,
                registered_at: None,
                base_sha: None,
                worktree_dir: None,
                app_server_socket: None,
            },
        )
        .unwrap();

        let state = ReviewState::beside_mail_dir(&mail_dir).unwrap();
        state
            .register_lane("mine", &repo.display().to_string())
            .unwrap();
        state
            .set_lane_repo("mine", "o/n", "feature", &head_sha)
            .unwrap();
        state.set_lane_base("mine", &base_sha).unwrap();

        let (cache_path, cache) = scratch_cache();
        cache
            .execute(
                "INSERT INTO repo (id, owner, name) VALUES (1, 'o', 'n')",
                [],
            )
            .unwrap();
        cache
            .execute(
                "INSERT INTO branch (id, repo_id, name, sha) VALUES (7, 1, 'feature', ?1)",
                params![head_sha],
            )
            .unwrap();
        cache
            .execute(
                "INSERT INTO change_log (entity_type, entity_id, event, repo_slug)
                 VALUES ('branch', 7, 'updated', 'o/n')",
                [],
            )
            .unwrap();
        let reader = CacheReader::open(&cache_path).unwrap();

        let mut slots = std::collections::HashMap::new();
        pump(&reader, &state, &mail_dir, &mut slots).unwrap();
        // Open the quiet window so the same pass can emit.
        let key = "mine:o/n:feature".to_owned();
        if let Some(Some(pending)) = slots.get_mut(&key) {
            pending.quiet_until_ms = 0;
            pending.max_until_ms = 0;
        }
        pump(&reader, &state, &mail_dir, &mut slots).unwrap();

        let rows = bus::messages_in(&store).unwrap();
        assert!(
            rows.iter()
                .any(|row| row.kind == MessageKind::Pr && row.to == "parent"),
            "expected one review notice to the parent, got {} rows",
            rows.len()
        );

        // A replay of the same change must not append a second notice.
        let before = bus::messages_in(&store).unwrap().len();
        state.set_cursor(0).unwrap();
        let mut replay_slots = std::collections::HashMap::new();
        pump(&reader, &state, &mail_dir, &mut replay_slots).unwrap();
        assert_eq!(bus::messages_in(&store).unwrap().len(), before);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn changed_content_key_fingerprint_rejects_patch_id_only_equality() {
        // Fingerprints include raw modes and rename/delete status, so two
        // changes that share a patch-id-shaped body over different modes are
        // not equivalent.
        let a = generation("h1", "fp", "ctx");
        let b = generation("h2", "fp-other", "ctx");
        assert!(!a.equivalent(&b));
        assert!(!a.equivalent(&Generation {
            head: "h3".to_owned(),
            base: "main".to_owned(),
            base_context: Some("ctx".to_owned()),
            fingerprint: None,
        }));
    }
}
