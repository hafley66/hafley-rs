//! Layer 3: the relational conversation store at `~/.agent/boop.db`.
//!
//! The base tables of QUERY-SURFACE.md land here, dictionary-encoded per the
//! sql-relational-design law: every JOINED or GROUPED column (session id, cwd,
//! branch, path, role, program, url, skill, pr, harness) is an INTEGER id into
//! a dict table; only payload text (`said`, `argline`, `nickname`) stays TEXT
//! because it is never a key. `boop sync` projects transcripts into these
//! tables; `boop follow` is the same projection in a poll loop.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::session::{Ingested, KnownSession, KnownSessions, SessionRef};

/// Every SQLite connection waits for a contending reader or the one WAL writer
/// for the same bounded interval.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The relational store.
pub struct Store {
    connection: Connection,
}

/// Bumped whenever stored rows mean something different. 8 = agent_pr keyed
/// (session_id, turn, pr_url_id): a turn with two PRs keeps two rows.
/// 10 = agent_favorite, user-pinned markdown bodies.
/// 11 = bounded, lane-addressable supervisor/channel trace events.
/// 13 = per-session attributes and the mood rows mail renders through.
/// 14 = the door a live session answers on, and one delivery row per hail.
pub const SCHEMA_VERSION: i64 = 14;
pub const TRACE_EVENT_RETENTION_LIMIT: u64 = 10_000;
const TRACE_EVENT_QUERY_LIMIT: u64 = 1_000;

/// The current-state `agent_live` row for one session, dict ids joined back
/// to TEXT: the pane it holds, its last status, and the door it answers on.
pub struct LiveRow {
    pub session: String,
    pub pid: Option<i64>,
    pub tmux_pane: Option<String>,
    pub status: Option<String>,
    pub door_kind: Option<String>,
    pub door_addr: Option<String>,
}

/// The attribute key a session's mood is stored under.
pub const MOOD_ATTR_KEY: &str = "mood";
/// The mood every session falls back to, and the template used when the store
/// holds no row for it.
pub const DEFAULT_MOOD: &str = "plain";
pub const DEFAULT_MOOD_TEMPLATE: &str = "[boop {id} from {from}] {body}";
/// An ancestry walk longer than this is a cycle in the lane tree.
const MOOD_ANCESTRY_LIMIT: i64 = 64;

/// The moods a store is born with. A row edited afterwards is never rewritten:
/// seeding is `INSERT OR IGNORE`.
const MOOD_SEEDS: [(&str, &str); 3] = [
    (DEFAULT_MOOD, DEFAULT_MOOD_TEMPLATE),
    (
        "unga",
        "unga: lists/tables/mermaid only, no prose\n{from} -> {id}\n{body}",
    ),
    ("board", "| {from} | {id} | {body} |"),
];

/// A row returned to the CLI, joined back from ids to the TEXT the query
/// surface exposes.
pub type Row = serde_json::Value;

/// One sync pass's row accounting. `dropped` can only rise when two writers
/// race on one ordinal, so a non-zero value is a defect signal, not a stat.
#[derive(Default, Clone, Copy, Debug)]
pub struct SyncStat {
    pub written: u64,
    pub dropped: u64,
    pub usage_written: u64,
    pub usage_updated: u64,
}

/// One completed native child whose parent currently has a registered route.
/// The two outbox lifecycle edges are kept in `agent_edge`, next to the
/// completion fact they advance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeChildCompletion {
    pub parent_session: String,
    pub child_session: String,
    pub completed_at_ms: u64,
    pub mailed: bool,
}

impl SyncStat {
    pub fn add(&mut self, other: SyncStat) {
        self.written += other.written;
        self.dropped += other.dropped;
        self.usage_written += other.usage_written;
        self.usage_updated += other.usage_updated;
    }
}

/// One LLM response's token counts. `input_tokens` EXCLUDES the cached buckets
/// (the opposite of the OTEL convention), so the five columns sum to the total.
pub struct UsageRow<'a> {
    pub ts: u64,
    pub message_id: &'a str,
    pub request_id: &'a str,
    pub model: &'a str,
    pub service_tier: Option<&'a str>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_create_5m_tokens: i64,
    pub cache_create_1h_tokens: i64,
    pub cache_read_tokens: i64,
    pub is_sidechain: bool,
    pub cost_usd_recorded: Option<f64>,
}

/// The per-transcript walk: the last ordinal handed out plus its accounting.
struct Walk {
    turn: u64,
    stat: SyncStat,
}

impl Walk {
    fn record(&mut self, inserted: usize) {
        if inserted == 0 {
            self.stat.dropped += 1;
        } else {
            self.stat.written += 1;
        }
    }
}

/// The shared turn-query filter set.
#[derive(Default, Clone)]
pub struct TurnQuery {
    pub harness: Option<String>,
    pub session: Option<String>,
    pub role: Option<String>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub turn_from: Option<u64>,
    pub turn_to: Option<u64>,
    pub path: Option<String>,
    pub limit: Option<u64>,
}

/// One lane spawn's purpose, as the store records it.
#[derive(Clone, Debug, Default)]
pub struct LaneSpawn {
    pub lane: String,
    pub trace: Option<String>,
    pub harness: Option<String>,
    pub branch: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub parent: Option<String>,
    pub goal: Option<String>,
    pub brief_path: Option<String>,
    pub brief_body: Option<String>,
    pub ts: u64,
}

/// The mood one session's mail renders through. `set_by` names the session
/// whose attribute row decided it, `None` when the default applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveMood {
    pub name: String,
    pub template: String,
    pub set_by: Option<String>,
}

impl EffectiveMood {
    fn default_mood() -> Self {
        EffectiveMood {
            name: DEFAULT_MOOD.to_owned(),
            template: DEFAULT_MOOD_TEMPLATE.to_owned(),
            set_by: None,
        }
    }

    /// The line `boop me` prints.
    pub fn line(&self) -> String {
        match &self.set_by {
            Some(setter) => format!("mood: {} (set by {setter})", self.name),
            None => format!("mood: {} (set by default)", self.name),
        }
    }
}

/// One durable supervisor/channel observation. `event_key` is minted by the
/// producer from trace/lane, a supervisor-run identity, and a monotonic
/// sequence. Timestamps are payload fields and never identity fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceEvent {
    pub event_key: String,
    pub lane: String,
    pub trace: Option<String>,
    pub session: Option<String>,
    pub kind: String,
    pub from_lane: Option<String>,
    pub to_lane: Option<String>,
    pub started_ts: Option<u64>,
    pub finished_ts: Option<u64>,
    pub delivery_state: Option<String>,
    pub classification: Option<String>,
    pub detail: String,
    pub created_ts: u64,
}

/// A joined trace event returned by the lane query. Endpoint identities are
/// returned as strings so callers never need dictionary table knowledge.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct TraceEventRow {
    pub event_key: String,
    pub lane: String,
    pub trace: Option<String>,
    pub session: Option<String>,
    pub kind: String,
    pub from_lane: Option<String>,
    pub to_lane: Option<String>,
    pub started_ts: Option<u64>,
    pub finished_ts: Option<u64>,
    pub delivery_state: Option<String>,
    pub classification: Option<String>,
    pub detail: String,
    pub created_ts: u64,
}

/// One `kind=error` trace event, flattened for the `boop debug` report.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct TraceErrorRow {
    pub lane: String,
    pub created_ts: u64,
    pub detail: String,
}

/// FNV-1a over the body, hex. The digest only has to separate distinct briefs
/// in one local store; nothing trusts it against an adversary.
fn markdown_digest(body: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in body.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}:{}", body.len())
}

/// Union-find root with path compression.
fn find(parents: &mut BTreeMap<i64, i64>, node: i64) -> i64 {
    let mut root = node;
    while let Some(next) = parents.get(&root).copied() {
        if next == root {
            break;
        }
        root = next;
    }
    let mut walk = node;
    while let Some(next) = parents.get(&walk).copied() {
        if next == walk {
            break;
        }
        parents.insert(walk, root);
        walk = next;
    }
    root
}

fn bounded_diagnostic(detail: &str) -> String {
    let mut clean = detail.to_owned();
    for marker in [
        "api_key=",
        "password=",
        "secret=",
        "token=",
        "Bearer ",
        "sk-",
    ] {
        let mut from = 0;
        while let Some(relative) = clean[from..].find(marker) {
            let start = from + relative;
            let value_start = start + marker.len();
            let value_end = clean[value_start..]
                .find(|character: char| {
                    character.is_whitespace() || matches!(character, ',' | '"' | '\'')
                })
                .map(|offset| value_start + offset)
                .unwrap_or(clean.len());
            clean.replace_range(value_start..value_end, "[redacted]");
            from = value_start + "[redacted]".len();
        }
    }
    if clean.len() <= 512 {
        return clean;
    }
    let mut end = 512;
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    clean.truncate(end);
    clean
}

fn configure_connection(connection: &Connection, path: &std::path::Path) -> Result<()> {
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .with_context(|| format!("set busy_timeout on {}", path.display()))?;
    // synchronous is connection-local. WAL itself is set only by initialization
    // or migration below, never by a current-schema hot-path open.
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .with_context(|| format!("set WAL synchronous mode on {}", path.display()))?;
    Ok(())
}

fn schema_version_of(connection: &Connection) -> Result<i64> {
    Ok(connection.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

fn enable_wal(connection: &Connection, path: &std::path::Path) -> Result<()> {
    let deadline = std::time::Instant::now() + BUSY_TIMEOUT;
    loop {
        match connection.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(rusqlite::Error::SqliteFailure(code, _))
                if matches!(
                    code.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                ) && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => {
                return Err(error).with_context(|| format!("enable WAL on {}", path.display()));
            }
        }
    }
}

impl Store {
    pub fn open(path: PathBuf) -> Result<Self> {
        let connection = Connection::open(&path)
            .with_context(|| format!("open boop.db at {}", path.display()))?;
        configure_connection(&connection, &path)?;
        let store = Store { connection };
        if store.schema_version()? < SCHEMA_VERSION {
            store.initialise_or_migrate(&path)?;
        }
        Ok(store)
    }

    /// Open the store read-only for the raw-SQL surface. No schema projection
    /// runs here: a read-only connection cannot write, so it must not appear
    /// to migrate a stale store.
    pub fn open_readonly(path: PathBuf) -> Result<Self> {
        let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("open boop.db read-only at {}", path.display()))?;
        configure_connection(&connection, &path)?;
        Ok(Store { connection })
    }

    fn stamp_version(&self) -> Result<()> {
        self.connection
            .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        schema_version_of(&self.connection)
    }

    /// True when the rows on disk predate this schema version. Reading them is
    /// safe; appending dense ordinals beside sparse ones is not.
    pub fn is_stale(&self) -> Result<bool> {
        if self.schema_version()? >= SCHEMA_VERSION {
            return Ok(false);
        }
        let turns: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM agent_turn", [], |row| row.get(0))?;
        Ok(turns > 0)
    }

    /// Change persistent journal mode and migrate only when this open observed
    /// an uninitialized or stale database. `BEGIN IMMEDIATE` serializes the
    /// check and migration; a process that waited behind another migrator sees
    /// the stamped version and commits without replaying DDL.
    fn initialise_or_migrate(&self, path: &std::path::Path) -> Result<()> {
        // SQLite does not apply busy_timeout while changing journal mode, so
        // concurrent first opens retry this one initialization-only pragma.
        enable_wal(&self.connection, path)?;
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            if self.schema_version()? >= SCHEMA_VERSION {
                return Ok(());
            }
            self.connection
                .execute_batch(SCHEMA)
                .with_context(|| format!("initialise boop.db schema at {}", path.display()))?;
            self.seed_moods()?;
            if self.schema_version()? == 0 {
                self.stamp_version()?;
                return Ok(());
            }
            // Each step runs only from the version it leaves, so a store that
            // already migrated 6->7 never re-runs those ALTERs.
            if self.schema_version()? < 7 {
                self.connection.execute_batch(
                    "CREATE TABLE IF NOT EXISTS dict_record
                       (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
                     ALTER TABLE sync_cursor ADD COLUMN record_id_id INTEGER;
                     ALTER TABLE sync_cursor ADD COLUMN turn INTEGER NOT NULL DEFAULT 0;
                     ALTER TABLE sync_cursor ADD COLUMN timestamp INTEGER NOT NULL DEFAULT 0;
                     PRAGMA user_version = 7;",
                )?;
            }
            if self.schema_version()? < 8 {
                self.connection.execute_batch(
                    "CREATE TABLE agent_pr_new (
                   session_id INTEGER NOT NULL,
                   turn INTEGER NOT NULL,
                   pr_url_id INTEGER NOT NULL,
                   PRIMARY KEY (session_id, turn, pr_url_id)
                 ) WITHOUT ROWID;
                 INSERT INTO agent_pr_new (session_id, turn, pr_url_id)
                   SELECT session_id, turn, pr_url_id FROM agent_pr;
                 DROP TABLE agent_pr;
                 ALTER TABLE agent_pr_new RENAME TO agent_pr;
                 PRAGMA user_version = 8;",
                )?;
            }
            if self.schema_version()? < 9 {
                self.backfill_traces()?;
            }
            if self.schema_version()? < 11 {
                self.connection.execute_batch(
                    "CREATE TABLE IF NOT EXISTS agent_trace_event (
                       event_id INTEGER PRIMARY KEY,
                       event_key TEXT NOT NULL UNIQUE,
                       lane_id INTEGER NOT NULL REFERENCES dict_session(id),
                       trace_id INTEGER REFERENCES dict_trace(id),
                       session_id INTEGER REFERENCES dict_session(id),
                       from_lane_id INTEGER REFERENCES dict_session(id),
                       to_lane_id INTEGER REFERENCES dict_session(id),
                       kind_id INTEGER NOT NULL REFERENCES dict_trace_kind(id),
                       started_ts INTEGER,
                       finished_ts INTEGER,
                       delivery_state_id INTEGER REFERENCES dict_trace_delivery(id),
                       classification_id INTEGER REFERENCES dict_trace_classification(id),
                       detail TEXT NOT NULL DEFAULT '',
                       created_ts INTEGER NOT NULL
                     );
                     CREATE INDEX IF NOT EXISTS idx_trace_event_lane_time
                       ON agent_trace_event(lane_id, created_ts, event_id);
                     CREATE INDEX IF NOT EXISTS idx_trace_event_trace_time
                       ON agent_trace_event(trace_id, created_ts, event_id);
                     PRAGMA user_version = 11;",
                )?;
            }
            if self.schema_version()? < 12 {
                self.connection.execute_batch(
                    "CREATE TABLE IF NOT EXISTS sync_root_stamp (
                       harness_id INTEGER NOT NULL,
                       root_path_id INTEGER NOT NULL,
                       mtime_ms INTEGER NOT NULL,
                       PRIMARY KEY (harness_id, root_path_id)
                     ) WITHOUT ROWID;
                     ",
                )?;
                let has_modified_ms = self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info('sync_cursor') WHERE name = 'modified_ms')",
                    [],
                    |row| row.get::<_, bool>(0),
                )?;
                if !has_modified_ms {
                    self.connection.execute_batch(
                        "ALTER TABLE sync_cursor ADD COLUMN modified_ms INTEGER NOT NULL DEFAULT 0;",
                    )?;
                }
                self.connection.execute_batch("PRAGMA user_version = 12;")?;
            }
            if self.schema_version()? < 14 {
                // agent_delivery arrives with SCHEMA above; an older
                // agent_live needs the two columns added.
                for column in ["door_kind", "door_addr"] {
                    let present = self.connection.query_row(
                        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('agent_live') WHERE name = ?1)",
                        params![column],
                        |row| row.get::<_, bool>(0),
                    )?;
                    if !present {
                        self.connection.execute_batch(&format!(
                            "ALTER TABLE agent_live ADD COLUMN {column} TEXT;"
                        ))?;
                    }
                }
                self.connection.execute_batch("PRAGMA user_version = 14;")?;
            }
            self.stamp_version()?;
            Ok(())
        })();
        match result {
            Ok(()) => self.connection.execute_batch("COMMIT")?,
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                return Err(error);
            }
        }
        Ok(())
    }

    /// Drop every table, recreate the schema, stamp the version; the caller
    /// re-syncs from byte 0. Favorites alone cross the drop by value.
    pub fn rebuild(&self) -> Result<()> {
        let mut favorites: Vec<(String, String, String, i64, i64)> = Vec::new();
        {
            let mut statement = self.connection.prepare(
                "SELECT m.body, f.note, f.source, f.created_ts, m.first_ts
                   FROM agent_favorite f
                   JOIN markdown_cache m ON m.markdown_id = f.markdown_id
                  ORDER BY f.favorite_id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?;
            for row in rows {
                favorites.push(row?);
            }
        }
        let mut names = Vec::new();
        {
            let mut statement = self.connection.prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                names.push(row?);
            }
        }
        for name in &names {
            self.connection
                .execute_batch(&format!("DROP TABLE IF EXISTS \"{name}\""))?;
        }
        self.connection.execute_batch(SCHEMA)?;
        self.seed_moods()?;
        self.stamp_version()?;
        for (body, note, source, created_ts, first_ts) in favorites {
            let markdown_id = self.intern_markdown(&body, first_ts as u64)?;
            self.connection.execute(
                "INSERT INTO agent_favorite (markdown_id, note, source, created_ts)
                 VALUES (?1, ?2, ?3, ?4)",
                params![markdown_id, note, source, created_ts],
            )?;
        }
        self.connection.execute_batch("VACUUM")?;
        Ok(())
    }

    pub fn default_path() -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("BOOP_DB").filter(|path| !path.is_empty()) {
            return Ok(PathBuf::from(path));
        }
        let home = dirs::home_dir().context("resolve home directory")?;
        Ok(home.join(".agent").join("boop.db"))
    }

    /// Wrap the next writes in one transaction so a batch of per-fact INSERTs
    /// commits once instead of once per row (rusqlite autocommits otherwise).
    pub fn begin(&self) -> Result<()> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        Ok(())
    }

    pub fn commit(&self) -> Result<()> {
        self.connection.execute_batch("COMMIT")?;
        Ok(())
    }

    pub fn rollback(&self) -> Result<()> {
        self.connection.execute_batch("ROLLBACK")?;
        Ok(())
    }

    fn intern(&self, table: &str, value: &str) -> Result<i64> {
        let insert = format!("INSERT OR IGNORE INTO {table} (id, value) VALUES (NULL, ?1)");
        self.connection.execute(&insert, params![value])?;
        let select = format!("SELECT id FROM {table} WHERE value = ?1");
        Ok(self
            .connection
            .query_row(&select, params![value], |row| row.get(0))?)
    }

    fn session_id(&self, session: &str) -> Result<i64> {
        self.intern("dict_session", session)
    }

    fn upsert_session_row(
        &self,
        session: &str,
        harness: &str,
        nickname: &str,
        cwd: Option<&str>,
        branch: Option<&str>,
        started_ts: u64,
    ) -> Result<()> {
        let sid = self.session_id(session)?;
        let harness_id = self.intern("dict_harness", harness)?;
        let cwd_id = cwd.map(|c| self.intern("dict_cwd", c)).transpose()?;
        let branch_id = branch.map(|b| self.intern("dict_branch", b)).transpose()?;
        self.connection.execute(
            "INSERT INTO agent_session (session_id, harness_id, nickname, cwd_id, branch_id, started_ts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(session_id) DO UPDATE SET harness_id=excluded.harness_id,
               nickname=excluded.nickname, cwd_id=excluded.cwd_id,
               branch_id=excluded.branch_id,
               started_ts=MIN(agent_session.started_ts, excluded.started_ts)",
            params![sid, harness_id, nickname, cwd_id, branch_id, started_ts as i64],
        )?;
        Ok(())
    }

    /// Returns rows inserted: 0 means the ordinal was already taken, which the
    /// caller counts as a defect rather than swallowing.
    fn add_turn(&self, session: &str, turn: u64, ts: u64, role: &str, said: &str) -> Result<usize> {
        let sid = self.session_id(session)?;
        let role_id = self.intern("dict_role", role)?;
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO agent_turn (session_id, turn, ts, role_id, said)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![sid, turn as i64, ts as i64, role_id, said],
        )?)
    }

    /// Intern the composite natural key of one LLM call. Returns the surrogate
    /// id and whether this call was seen for the first time.
    fn intern_request(&self, message_id: &str, request_id: &str) -> Result<(i64, bool)> {
        let inserted = self.connection.execute(
            "INSERT OR IGNORE INTO dict_request (message_id, request_id) VALUES (?1, ?2)",
            params![message_id, request_id],
        )?;
        let id: i64 = self.connection.query_row(
            "SELECT id FROM dict_request WHERE message_id = ?1 AND request_id = ?2",
            params![message_id, request_id],
            |row| row.get(0),
        )?;
        Ok((id, inserted > 0))
    }

    /// A repeat of a stored call updates counts only when `output_tokens` did
    /// not go backwards; the transcript writes snapshots first, final last.
    fn add_usage(
        &self,
        session: &str,
        turn: u64,
        request_ref: i64,
        is_new: bool,
        usage: &UsageRow,
    ) -> Result<bool> {
        let sid = self.session_id(session)?;
        let model_id = self.intern("dict_model", usage.model)?;
        let tier_id = usage
            .service_tier
            .map(|tier| self.intern("dict_service_tier", tier))
            .transpose()?;
        if is_new {
            self.connection.execute(
                "INSERT INTO agent_usage (session_id, turn, ts, request_ref, model_id,
                   service_tier_id, input_tokens, output_tokens, cache_create_5m_tokens,
                   cache_create_1h_tokens, cache_read_tokens, is_sidechain, cost_usd_recorded)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    sid,
                    turn as i64,
                    usage.ts as i64,
                    request_ref,
                    model_id,
                    tier_id,
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.cache_create_5m_tokens,
                    usage.cache_create_1h_tokens,
                    usage.cache_read_tokens,
                    i64::from(usage.is_sidechain),
                    usage.cost_usd_recorded,
                ],
            )?;
            return Ok(true);
        }
        let changed = self.connection.execute(
            "UPDATE agent_usage SET ts = ?2, input_tokens = ?3, output_tokens = ?4,
               cache_create_5m_tokens = ?5, cache_create_1h_tokens = ?6,
               cache_read_tokens = ?7, cost_usd_recorded = ?8
             WHERE request_ref = ?1 AND ?4 >= output_tokens",
            params![
                request_ref,
                usage.ts as i64,
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_create_5m_tokens,
                usage.cache_create_1h_tokens,
                usage.cache_read_tokens,
                usage.cost_usd_recorded,
            ],
        )?;
        Ok(changed > 0)
    }

    /// The highest ordinal already stored for a session; the next sync pass
    /// hands out `max + 1` so a growing transcript never renumbers.
    fn max_turn(&self, session: &str) -> Result<u64> {
        let sid = self.session_id(session)?;
        let max: i64 = self.connection.query_row(
            "SELECT COALESCE(MAX(turn), 0) FROM agent_turn WHERE session_id = ?1",
            params![sid],
            |row| row.get(0),
        )?;
        Ok(max as u64)
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Intern a natural key into a dictionary, for callers outside this module.
    pub fn intern_public(&self, table: &str, value: &str) -> Result<i64> {
        self.intern(table, value)
    }

    /// Run raw read-only SQL against this connection; return the column names
    /// (for a text header) and one JSON object per row. Errors surface SQLite's
    /// own message verbatim, never rewritten.
    pub fn passthrough(&self, sql: &str) -> Result<(Vec<String>, Vec<Row>)> {
        let mut statement = self.connection.prepare(sql)?;
        let names: Vec<String> = statement
            .column_names()
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut rows = statement.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let mut object = serde_json::Map::new();
            for (index, name) in names.iter().enumerate() {
                let value = match row.get_ref(index)? {
                    rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                    rusqlite::types::ValueRef::Integer(number) => serde_json::json!(number),
                    rusqlite::types::ValueRef::Real(number) => serde_json::json!(number),
                    rusqlite::types::ValueRef::Text(text) => {
                        serde_json::json!(String::from_utf8_lossy(text))
                    }
                    rusqlite::types::ValueRef::Blob(_) => serde_json::Value::Null,
                };
                object.insert(name.clone(), value);
            }
            out.push(serde_json::Value::Object(object));
        }
        Ok((names, out))
    }

    /// Run a SELECT and hand back one JSON object per row, keyed by column
    /// name, so a read surface never restates the column list twice.
    pub fn rows(&self, sql: &str, values: Vec<rusqlite::types::Value>) -> Result<Vec<Row>> {
        let mut statement = self.connection.prepare(sql)?;
        let names: Vec<String> = statement
            .column_names()
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut out = Vec::new();
        let iter = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            let mut object = serde_json::Map::new();
            for (index, name) in names.iter().enumerate() {
                let value = match row.get_ref(index)? {
                    rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                    rusqlite::types::ValueRef::Integer(number) => serde_json::json!(number),
                    rusqlite::types::ValueRef::Real(number) => serde_json::json!(number),
                    rusqlite::types::ValueRef::Text(text) => {
                        serde_json::json!(String::from_utf8_lossy(text))
                    }
                    rusqlite::types::ValueRef::Blob(_) => serde_json::Value::Null,
                };
                object.insert(name.clone(), value);
            }
            Ok(serde_json::Value::Object(object))
        })?;
        for row in iter {
            out.push(row?);
        }
        Ok(out)
    }

    /// Sessions whose ordinals are not dense. Empty is the invariant every
    /// sync preserves; the CLI prints the count as a sync receipt.
    pub fn sparse_sessions(&self) -> Result<Vec<(String, i64, i64)>> {
        let mut statement = self.connection.prepare(
            "SELECT dict_session.value, COUNT(*), MAX(agent_turn.turn)
             FROM agent_turn
             JOIN dict_session ON dict_session.id = agent_turn.session_id
             GROUP BY agent_turn.session_id
             HAVING COUNT(*) <> MAX(agent_turn.turn)",
        )?;
        let mut out = Vec::new();
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn add_touch(
        &self,
        session: &str,
        turn: u64,
        ts: u64,
        path: &str,
        verb: &str,
        raw_verb: &str,
    ) -> Result<()> {
        let sid = self.session_id(session)?;
        let path_id = self.intern("dict_path", path)?;
        // verb_id is the canonical lowercase spelling; raw_verb_id keeps the
        // harness's own casing on disk so a consumer never re-normalizes.
        let verb_id = self.intern("dict_verb", verb)?;
        let raw_verb_id = self.intern("dict_verb", raw_verb)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_touch
               (session_id, turn, ts, path_id, verb_id, raw_verb_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![sid, turn as i64, ts as i64, path_id, verb_id, raw_verb_id],
        )?;
        Ok(())
    }

    fn add_cmd(
        &self,
        session: &str,
        turn: u64,
        ts: u64,
        program: &str,
        argline: &str,
    ) -> Result<()> {
        let sid = self.session_id(session)?;
        let program_id = self.intern("dict_program", program)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_cmd (session_id, turn, ts, program_id, argline)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![sid, turn as i64, ts as i64, program_id, argline],
        )?;
        Ok(())
    }

    fn add_fetch(&self, session: &str, turn: u64, ts: u64, url: &str, domain: &str) -> Result<()> {
        let sid = self.session_id(session)?;
        let kind_id = self.intern("dict_netkind", "fetch")?;
        let url_id = self.intern("dict_url", url)?;
        let domain_id = self.intern("dict_domain", domain)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_fetch (session_id, turn, ts, kind_id, url_id, domain_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![sid, turn as i64, ts as i64, kind_id, url_id, domain_id],
        )?;
        Ok(())
    }

    fn add_search(&self, session: &str, turn: u64, ts: u64, query: &str) -> Result<()> {
        let sid = self.session_id(session)?;
        let kind_id = self.intern("dict_netkind", "search")?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_fetch (session_id, turn, ts, kind_id, query)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![sid, turn as i64, ts as i64, kind_id, query],
        )?;
        Ok(())
    }

    fn add_skill(&self, session: &str, turn: u64, skill: &str) -> Result<()> {
        let sid = self.session_id(session)?;
        let skill_id = self.intern("dict_skill", skill)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_skill (session_id, turn, skill_id) VALUES (?1, ?2, ?3)",
            params![sid, turn as i64, skill_id],
        )?;
        Ok(())
    }

    fn add_pr(&self, session: &str, turn: u64, pr_url: &str) -> Result<()> {
        let sid = self.session_id(session)?;
        let pr_id = self.intern("dict_pr", pr_url)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_pr (session_id, turn, pr_url_id) VALUES (?1, ?2, ?3)",
            params![sid, turn as i64, pr_id],
        )?;
        Ok(())
    }

    fn set_cursor(&self, session: &str, path: &str, offset: u64) -> Result<()> {
        let sid = self.session_id(session)?;
        let path_id = self.intern("dict_path", path)?;
        self.connection.execute(
            "INSERT INTO sync_cursor (session_id, path_id, offset) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id, path_id) DO UPDATE SET offset=excluded.offset",
            params![sid, path_id, offset as i64],
        )?;
        Ok(())
    }

    fn set_cursor_modified(
        &self,
        session: &str,
        path: &str,
        offset: u64,
        modified_ms: u64,
    ) -> Result<()> {
        let sid = self.session_id(session)?;
        let path_id = self.intern("dict_path", path)?;
        self.connection.execute(
            "INSERT INTO sync_cursor (session_id, path_id, offset, modified_ms) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(session_id, path_id) DO UPDATE SET offset=excluded.offset, modified_ms=excluded.modified_ms",
            params![sid, path_id, offset as i64, modified_ms as i64],
        )?;
        Ok(())
    }

    /// How many stored cursors still carry the v12 migration's `modified_ms = 0`.
    /// Zero means the per-candidate backfill has nothing to write.
    pub fn cursors_missing_modified(&self) -> Result<i64> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM sync_cursor WHERE modified_ms = 0",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn backfill_cursor_modified(
        &self,
        session: &str,
        path: &str,
        modified_ms: u64,
    ) -> Result<()> {
        let sid = self.session_id(session)?;
        let path_id = self.intern("dict_path", path)?;
        self.connection.execute(
            "UPDATE sync_cursor SET modified_ms = ?3
             WHERE session_id = ?1 AND path_id = ?2 AND modified_ms = 0",
            params![sid, path_id, modified_ms as i64],
        )?;
        Ok(())
    }

    fn get_cursor(&self, session: &str, path: &str) -> Result<u64> {
        let sid = self.session_id(session)?;
        let path_id = self.intern("dict_path", path)?;
        let offset: i64 = self
            .connection
            .query_row(
                "SELECT offset FROM sync_cursor WHERE session_id = ?1 AND path_id = ?2",
                params![sid, path_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(offset as u64)
    }

    /// The consumed byte offset for a transcript, 0 when never synced. The
    /// sync freshness gate compares it to `metadata.len()` to skip re-reads.
    pub fn cursor_offset(&self, session: &str, path: &str) -> Result<u64> {
        self.get_cursor(session, path)
    }

    /// Metadata for transcript paths already projected into this store. A
    /// harness uses it to construct candidates from a filesystem stat instead
    /// of reopening each historical transcript just to parse its first record.
    pub fn known_sessions(&self) -> Result<KnownSessions> {
        let mut statement = self.connection.prepare(
            "SELECT dp.value, ds.value, COALESCE(s.nickname, ds.value), cwd.value, branch.value, harness.value,
                    (SELECT parent.value
                       FROM agent_edge edge
                       JOIN dict_edekind kind ON kind.id = edge.edge_kind_id
                       JOIN dict_session parent ON parent.id = edge.parent_session_id
                      WHERE edge.child_session_id = sc.session_id AND kind.value = 'spawned'
                      LIMIT 1),
                    sc.offset, sc.modified_ms
             FROM sync_cursor sc
             JOIN dict_session ds ON ds.id = sc.session_id
             JOIN dict_path dp ON dp.id = sc.path_id
             JOIN agent_session s ON s.session_id = sc.session_id
             JOIN dict_harness harness ON harness.id = s.harness_id
             LEFT JOIN dict_cwd cwd ON cwd.id = s.cwd_id
             LEFT JOIN dict_branch branch ON branch.id = s.branch_id",
        )?;
        let mut out = KnownSessions::new();
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })?;
        for row in rows {
            let (path, session_id, nickname, cwd, git_branch, harness, parent, cursor, modified_ms) =
                row?;
            out.insert(
                PathBuf::from(path),
                KnownSession {
                    harness,
                    session_id,
                    nickname,
                    cwd,
                    git_branch,
                    parent,
                    cursor: cursor as u64,
                    modified_ms: modified_ms as u64,
                },
            );
        }
        Ok(out)
    }

    /// Project discovery metadata independently of transcript bytes. Harness
    /// discovery is authoritative for identity, cwd, activity, and native
    /// parent relation even when a transcript is empty or unchanged.
    pub fn project_discovered_session(&self, session: &SessionRef) -> Result<()> {
        self.upsert_session_row(
            &session.session_id,
            session.harness.as_str(),
            &session.nickname,
            session.cwd.as_deref(),
            session.git_branch.as_deref(),
            session.modified_ms,
        )?;
        if let Some(parent) = &session.parent {
            self.ensure_edge(parent, &session.session_id, "spawned")?;
        }
        Ok(())
    }

    /// Record a spawn edge between two known sessions, stamped at the current
    /// wall clock.
    pub fn add_edge(&self, parent: &str, child: &str, kind: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.add_edge_at(parent, child, kind, now)
    }

    /// Record an edge observation at an explicit timestamp. The first sighting
    /// sets `first_ts`; every repeat bumps `last_ts` and `n`, so one structural
    /// spawn stays one row while repeated communication across that edge is
    /// counted, not collapsed.
    pub fn add_edge_at(&self, parent: &str, child: &str, kind: &str, ts: u64) -> Result<()> {
        let parent_id = self.session_id(parent)?;
        let child_id = self.session_id(child)?;
        let kind_id = self.intern("dict_edekind", kind)?;
        self.connection.execute(
            "INSERT INTO agent_edge
               (parent_session_id, child_session_id, edge_kind_id, first_ts, last_ts, n)
             VALUES (?1, ?2, ?3, ?4, ?4, 1)
             ON CONFLICT(parent_session_id, child_session_id, edge_kind_id) DO UPDATE SET
               last_ts = excluded.last_ts,
               n = agent_edge.n + 1",
            params![parent_id, child_id, kind_id, ts as i64],
        )?;
        Ok(())
    }

    /// Record one lifecycle edge once. Returns whether this pass inserted the
    /// row, for callers that need a first-observation receipt.
    pub fn ensure_edge_at(&self, parent: &str, child: &str, kind: &str, ts: u64) -> Result<bool> {
        let parent_id = self.session_id(parent)?;
        let child_id = self.session_id(child)?;
        let kind_id = self.intern("dict_edekind", kind)?;
        let changed = self.connection.execute(
            "INSERT INTO agent_edge
               (parent_session_id, child_session_id, edge_kind_id, first_ts, last_ts, n)
             VALUES (?1, ?2, ?3, ?4, ?4, 1)
             ON CONFLICT(parent_session_id, child_session_id, edge_kind_id) DO NOTHING",
            params![parent_id, child_id, kind_id, ts as i64],
        )?;
        Ok(changed == 1)
    }

    fn ensure_edge(&self, parent: &str, child: &str, kind: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        let parent_id = self.session_id(parent)?;
        let child_id = self.session_id(child)?;
        let kind_id = self.intern("dict_edekind", kind)?;
        self.connection.execute(
            "INSERT INTO agent_edge
               (parent_session_id, child_session_id, edge_kind_id, first_ts, last_ts, n)
             VALUES (?1, ?2, ?3, ?4, ?4, 1)
             ON CONFLICT(parent_session_id, child_session_id, edge_kind_id) DO NOTHING",
            params![parent_id, child_id, kind_id, now as i64],
        )?;
        Ok(())
    }

    /// Store one markdown body once. The digest is the natural key; a second
    /// spawn of the same brief returns the id already there.
    pub fn intern_markdown(&self, body: &str, ts: u64) -> Result<i64> {
        let digest = markdown_digest(body);
        self.connection.execute(
            "INSERT OR IGNORE INTO markdown_cache (digest, body, bytes, first_ts)
             VALUES (?1, ?2, ?3, ?4)",
            params![digest, body, body.len() as i64, ts as i64],
        )?;
        Ok(self.connection.query_row(
            "SELECT markdown_id FROM markdown_cache WHERE digest = ?1",
            params![digest],
            |row| row.get(0),
        )?)
    }

    /// Pin one markdown body as a favorite. The body dedupes through
    /// markdown_cache; note and source ride on the favorite row itself.
    pub fn favorite_add(&self, body: &str, note: &str, source: &str, ts: u64) -> Result<i64> {
        let markdown_id = self.intern_markdown(body, ts)?;
        self.connection.execute(
            "INSERT INTO agent_favorite (markdown_id, note, source, created_ts)
             VALUES (?1, ?2, ?3, ?4)",
            params![markdown_id, note, source, ts as i64],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// The trace a session belongs to, by trace name.
    pub fn trace_of(&self, session: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT t.value FROM agent_trace_span s
                   JOIN dict_session d ON d.id = s.session_id
                   JOIN dict_trace t ON t.id = s.trace_id
                  WHERE d.value = ?1",
                params![session],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    /// Put `session` under `trace`, recording which rule decided it. The first
    /// span of a trace becomes its root. A session already attached stays put:
    /// re-attaching is how two arcs would silently merge.
    pub fn attach_trace(&self, session: &str, trace: &str, rule: &str, ts: u64) -> Result<()> {
        let session_id = self.session_id(session)?;
        let trace_id = self.intern("dict_trace", trace)?;
        let attach_id = self.intern("dict_attach", rule)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_trace (trace_id, root_session_id, started_ts)
             VALUES (?1, ?2, ?3)",
            params![trace_id, session_id, ts as i64],
        )?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_trace_span (session_id, trace_id, attach_id, attached_ts)
             VALUES (?1, ?2, ?3, ?4)",
            params![session_id, trace_id, attach_id, ts as i64],
        )?;
        Ok(())
    }

    /// Persist one supervisor/channel event and prune the global event log in
    /// the same transaction. Duplicate producer keys resolve to their
    /// existing internal row, making retries idempotent.
    pub fn record_trace_event(&self, event: &TraceEvent) -> Result<i64> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let lane_id = self.session_id(&event.lane)?;
            let trace_id = event
                .trace
                .as_deref()
                .map(|value| self.intern("dict_trace", value))
                .transpose()?;
            let session_id = event
                .session
                .as_deref()
                .map(|value| self.session_id(value))
                .transpose()?;
            let from_lane_id = event
                .from_lane
                .as_deref()
                .map(|value| self.session_id(value))
                .transpose()?;
            let to_lane_id = event
                .to_lane
                .as_deref()
                .map(|value| self.session_id(value))
                .transpose()?;
            let kind_id = self.intern("dict_trace_kind", &event.kind)?;
            let delivery_state_id = event
                .delivery_state
                .as_deref()
                .map(|value| self.intern("dict_trace_delivery", value))
                .transpose()?;
            let classification_id = event
                .classification
                .as_deref()
                .map(|value| self.intern("dict_trace_classification", value))
                .transpose()?;
            let detail = bounded_diagnostic(&event.detail);
            self.connection.execute(
                "INSERT OR IGNORE INTO agent_trace_event
                   (event_key, lane_id, trace_id, session_id, from_lane_id, to_lane_id,
                    kind_id, started_ts, finished_ts, delivery_state_id,
                    classification_id, detail, created_ts)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    event.event_key,
                    lane_id,
                    trace_id,
                    session_id,
                    from_lane_id,
                    to_lane_id,
                    kind_id,
                    event.started_ts.map(|value| value as i64),
                    event.finished_ts.map(|value| value as i64),
                    delivery_state_id,
                    classification_id,
                    detail,
                    event.created_ts as i64,
                ],
            )?;
            let event_id = self.connection.query_row(
                "SELECT event_id FROM agent_trace_event WHERE event_key = ?1",
                params![event.event_key],
                |row| row.get(0),
            )?;
            self.prune_trace_events_in_transaction(TRACE_EVENT_RETENTION_LIMIT)?;
            Ok(event_id)
        })();
        match result {
            Ok(event_id) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(event_id)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// Query events for one lane, joining all dictionary identities back to
    /// strings. A caller cannot request more than the bounded query window.
    pub fn query_trace_events(&self, lane: Option<&str>, limit: u64) -> Result<Vec<TraceEventRow>> {
        let limit = limit.min(TRACE_EVENT_QUERY_LIMIT);
        let mut statement = self.connection.prepare(
            "SELECT e.event_key, lane.value, trace.value, session.value,
                    kind.value, from_lane.value, to_lane.value, e.started_ts,
                    e.finished_ts, delivery.value, classification.value, e.detail,
                    e.created_ts
               FROM agent_trace_event e
               JOIN dict_session lane ON lane.id = e.lane_id
               LEFT JOIN dict_trace trace ON trace.id = e.trace_id
               LEFT JOIN dict_session session ON session.id = e.session_id
               LEFT JOIN dict_session from_lane ON from_lane.id = e.from_lane_id
               LEFT JOIN dict_session to_lane ON to_lane.id = e.to_lane_id
               JOIN dict_trace_kind kind ON kind.id = e.kind_id
               LEFT JOIN dict_trace_delivery delivery ON delivery.id = e.delivery_state_id
               LEFT JOIN dict_trace_classification classification
                 ON classification.id = e.classification_id
              WHERE (?1 IS NULL OR lane.value = ?1)
              ORDER BY e.created_ts, e.event_id
              LIMIT ?2",
        )?;
        let mut rows = statement.query(params![lane, limit as i64])?;
        let mut events = Vec::new();
        while let Some(row) = rows.next()? {
            events.push(TraceEventRow {
                event_key: row.get(0)?,
                lane: row.get(1)?,
                trace: row.get(2)?,
                session: row.get(3)?,
                kind: row.get(4)?,
                from_lane: row.get(5)?,
                to_lane: row.get(6)?,
                started_ts: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
                finished_ts: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
                delivery_state: row.get(9)?,
                classification: row.get(10)?,
                detail: row.get(11)?,
                created_ts: row.get::<_, i64>(12)? as u64,
            });
        }
        Ok(events)
    }

    /// The `kind=error` trace events at or after `since_ms`, oldest first. The
    /// canned read behind `boop debug`.
    pub fn error_events_since(
        &self,
        since_ms: u64,
        lane: Option<&str>,
    ) -> Result<Vec<TraceErrorRow>> {
        let mut statement = self.connection.prepare(
            "SELECT lane.value, e.created_ts, e.detail
               FROM agent_trace_event e
               JOIN dict_session lane ON lane.id = e.lane_id
               JOIN dict_trace_kind kind ON kind.id = e.kind_id
              WHERE kind.value = 'error'
                AND e.created_ts >= ?1
                AND (?2 IS NULL OR lane.value = ?2)
              ORDER BY e.created_ts, e.event_id
              LIMIT ?3",
        )?;
        let mut rows = statement.query(params![
            since_ms as i64,
            lane,
            TRACE_EVENT_QUERY_LIMIT as i64
        ])?;
        let mut events = Vec::new();
        while let Some(row) = rows.next()? {
            events.push(TraceErrorRow {
                lane: row.get(0)?,
                created_ts: row.get::<_, i64>(1)? as u64,
                detail: row.get(2)?,
            });
        }
        Ok(events)
    }

    /// Delete the oldest rows beyond `max_rows` in deterministic order.
    pub fn prune_trace_events(&self, max_rows: u64) -> Result<u64> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.prune_trace_events_in_transaction(max_rows);
        match result {
            Ok(deleted) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(deleted)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    fn prune_trace_events_in_transaction(&self, max_rows: u64) -> Result<u64> {
        Ok(self.connection.execute(
            "DELETE FROM agent_trace_event
              WHERE event_id IN (
                SELECT event_id FROM agent_trace_event
                 ORDER BY created_ts DESC, event_id DESC
                 LIMIT -1 OFFSET ?1
              )",
            params![max_rows as i64],
        )? as u64)
    }

    /// Every session under one trace, oldest attach first.
    pub fn trace_sessions(&self, trace: &str) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "SELECT d.value FROM agent_trace_span s
               JOIN dict_session d ON d.id = s.session_id
               JOIN dict_trace t ON t.id = s.trace_id
              WHERE t.value = ?1 ORDER BY s.attached_ts",
        )?;
        let rows = statement.query_map(params![trace], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Record what a lane was told to do: its goal, the brief path, and the
    /// brief bytes as of this spawn.
    #[allow(clippy::too_many_arguments)]
    pub fn record_lane_spawn(&self, spawn: &LaneSpawn) -> Result<i64> {
        let lane_id = self.session_id(&spawn.lane)?;
        let trace_id = match &spawn.trace {
            Some(trace) => Some(self.intern("dict_trace", trace)?),
            None => None,
        };
        let harness_id = match &spawn.harness {
            Some(value) => Some(self.intern("dict_harness", value)?),
            None => None,
        };
        let branch_id = match &spawn.branch {
            Some(value) => Some(self.intern("dict_branch", value)?),
            None => None,
        };
        let cwd_id = match &spawn.cwd {
            Some(value) => Some(self.intern("dict_cwd", value)?),
            None => None,
        };
        let model_id = match &spawn.model {
            Some(value) => Some(self.intern("dict_model", value)?),
            None => None,
        };
        let parent_id = match &spawn.parent {
            Some(value) => Some(self.session_id(value)?),
            None => None,
        };
        let brief_path_id = match &spawn.brief_path {
            Some(value) => Some(self.intern("dict_path", value)?),
            None => None,
        };
        let brief_markdown_id = match &spawn.brief_body {
            Some(body) => Some(self.intern_markdown(body, spawn.ts)?),
            None => None,
        };
        self.connection.execute(
            "INSERT INTO agent_lane
               (lane_id, trace_id, harness_id, branch_id, cwd_id, model_id,
                parent_lane_id, goal, brief_path_id, brief_markdown_id, spawned_ts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                lane_id,
                trace_id,
                harness_id,
                branch_id,
                cwd_id,
                model_id,
                parent_id,
                spawn.goal,
                brief_path_id,
                brief_markdown_id,
                spawn.ts as i64
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Write the moods a store is born with. Reentrant: an edited template
    /// survives every later open.
    fn seed_moods(&self) -> Result<()> {
        for (name, template) in MOOD_SEEDS {
            self.connection.execute(
                "INSERT OR IGNORE INTO dict_mood_name (id, value) VALUES (NULL, ?1)",
                params![name],
            )?;
            self.connection.execute(
                "INSERT OR IGNORE INTO mood (name_id, template)
                   SELECT id, ?2 FROM dict_mood_name WHERE value = ?1",
                params![name, template],
            )?;
        }
        Ok(())
    }

    /// Every mood a session may be set to, alphabetical.
    pub fn mood_names(&self) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "SELECT name.value FROM mood
               JOIN dict_mood_name name ON name.id = mood.name_id
              ORDER BY name.value",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut names = Vec::new();
        for row in rows {
            names.push(row?);
        }
        Ok(names)
    }

    /// One session attribute, last write wins.
    pub fn set_session_attr(&self, session: &str, key: &str, value: &str, ts: u64) -> Result<()> {
        let session_id = self.session_id(session)?;
        let key_id = self.intern("dict_attr_key", key)?;
        self.connection.execute(
            "INSERT INTO agent_session_attr (session_id, key_id, value, set_ts)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (session_id, key_id)
               DO UPDATE SET value = excluded.value, set_ts = excluded.set_ts",
            params![session_id, key_id, value, ts as i64],
        )?;
        Ok(())
    }

    /// Drop one session attribute. False when the session had none.
    pub fn clear_session_attr(&self, session: &str, key: &str) -> Result<bool> {
        let removed = self.connection.execute(
            "DELETE FROM agent_session_attr
              WHERE session_id = (SELECT id FROM dict_session WHERE value = ?1)
                AND key_id = (SELECT id FROM dict_attr_key WHERE value = ?2)",
            params![session, key],
        )?;
        Ok(removed > 0)
    }

    /// Set one session's mood. An unknown name names the known ones.
    pub fn set_session_mood(&self, session: &str, mood: &str, ts: u64) -> Result<()> {
        self.check_mood_name(mood)?;
        self.set_session_attr(session, MOOD_ATTR_KEY, mood, ts)
    }

    /// Error unless `mood` is a stored mood.
    pub fn check_mood_name(&self, mood: &str) -> Result<()> {
        let known = self.mood_names()?;
        if known.iter().any(|name| name == mood) {
            return Ok(());
        }
        anyhow::bail!("unknown mood {mood}; known moods: {}", known.join(", "))
    }

    /// The mood mail addressed to `session` renders through: the session's own
    /// attribute row, else the nearest ancestor's up the lane tree, else the
    /// default. One statement, whatever the depth.
    pub fn effective_mood(&self, session: &str) -> Result<EffectiveMood> {
        let resolved = self
            .connection
            .query_row(
                "WITH RECURSIVE ancestry(session_id, depth) AS (
                     SELECT id, 0 FROM dict_session WHERE value = ?1
                     UNION
                     SELECT lane.parent_lane_id, ancestry.depth + 1
                       FROM ancestry
                       JOIN agent_lane lane ON lane.lane_id = ancestry.session_id
                      WHERE lane.parent_lane_id IS NOT NULL
                        AND ancestry.depth < ?4
                 )
                 SELECT name, template, set_by FROM (
                     SELECT mood_name.value AS name,
                            mood.template AS template,
                            setter.value AS set_by,
                            ancestry.depth AS rank
                       FROM ancestry
                       JOIN agent_session_attr attr ON attr.session_id = ancestry.session_id
                       JOIN dict_attr_key attr_key
                         ON attr_key.id = attr.key_id AND attr_key.value = ?2
                       JOIN dict_mood_name mood_name ON mood_name.value = attr.value
                       JOIN mood ON mood.name_id = mood_name.id
                       JOIN dict_session setter ON setter.id = ancestry.session_id
                     UNION ALL
                     SELECT mood_name.value, mood.template, NULL, ?4 + 1
                       FROM mood
                       JOIN dict_mood_name mood_name ON mood_name.id = mood.name_id
                      WHERE mood_name.value = ?3
                 )
                 ORDER BY rank, set_by
                 LIMIT 1",
                params![session, MOOD_ATTR_KEY, DEFAULT_MOOD, MOOD_ANCESTRY_LIMIT],
                |row| {
                    Ok(EffectiveMood {
                        name: row.get(0)?,
                        template: row.get(1)?,
                        set_by: row.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(resolved.unwrap_or_else(EffectiveMood::default_mood))
    }

    /// Give every connected component of `spawned` edges one trace, named for
    /// its root session. A session with no edge is left unattached: adjacency
    /// in time is a guess, and a wrong attach merges two unrelated arcs.
    pub fn backfill_traces(&self) -> Result<usize> {
        let mut parents: BTreeMap<i64, i64> = BTreeMap::new();
        let mut statement = self.connection.prepare(
            "SELECT a.parent_session_id, a.child_session_id, a.first_ts FROM agent_edge a
               JOIN dict_edekind k ON k.id = a.edge_kind_id WHERE k.value = 'spawned'",
        )?;
        let mut edges = Vec::new();
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            edges.push(row?);
        }
        for (parent, child, _) in &edges {
            parents.entry(*parent).or_insert(*parent);
            parents.entry(*child).or_insert(*child);
        }
        for (parent, child, _) in &edges {
            let root = find(&mut parents, *parent);
            let other = find(&mut parents, *child);
            if root != other {
                parents.insert(other, root);
            }
        }
        let mut attached = 0usize;
        for (parent, child, ts) in &edges {
            for session in [parent, child] {
                let root = find(&mut parents, *session);
                let name = self.session_name(root)?;
                let trace_id = self.intern("dict_trace", &format!("trace-{name}"))?;
                let attach_id = self.intern("dict_attach", "backfill-spawned-edge")?;
                self.connection.execute(
                    "INSERT OR IGNORE INTO agent_trace (trace_id, root_session_id, started_ts)
                     VALUES (?1, ?2, ?3)",
                    params![trace_id, root, ts],
                )?;
                attached += self.connection.execute(
                    "INSERT OR IGNORE INTO agent_trace_span
                       (session_id, trace_id, attach_id, attached_ts)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![session, trace_id, attach_id, ts],
                )?;
            }
        }
        Ok(attached)
    }

    fn session_name(&self, id: i64) -> Result<String> {
        Ok(self.connection.query_row(
            "SELECT value FROM dict_session WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?)
    }

    /// Query spawn edges, joined back to the TEXT query surface. `session`
    /// filters to edges touching that session id; none means all edges.
    pub fn query_edges(&self, session: Option<&str>) -> Result<Vec<Row>> {
        let sql = "SELECT p.value AS parent, c.value AS child, e.value AS edge,
                          a.first_ts, a.last_ts, a.n
                   FROM agent_edge a
                   JOIN dict_session p ON p.id = a.parent_session_id
                   JOIN dict_session c ON c.id = a.child_session_id
                   JOIN dict_edekind e ON e.id = a.edge_kind_id
                   WHERE (?1 IS NULL OR c.value = ?1 OR p.value = ?1)
                   ORDER BY p.value, c.value";
        let mut statement = self.connection.prepare(sql)?;
        let value: Option<String> = session.map(str::to_owned);
        let mut rows = Vec::new();
        let iter = statement.query_map(params![value], |row| {
            Ok(serde_json::json!({
                "kind": "agent_edge",
                "parent": row.get::<_, String>(0)?,
                "child": row.get::<_, String>(1)?,
                "edge": row.get::<_, String>(2)?,
                "first_ts": row.get::<_, Option<i64>>(3)?,
                "last_ts": row.get::<_, Option<i64>>(4)?,
                "n": row.get::<_, i64>(5)?,
            }))
        })?;
        for row in iter {
            rows.push(row?);
        }
        Ok(rows)
    }

    /// Edges as typed rows, with temporal and count evidence.
    pub fn edge_rows(&self, session: Option<&str>) -> Result<Vec<crate::rows::EdgeRow>> {
        let sql = "SELECT p.value, c.value, e.value, a.first_ts, a.last_ts, a.n
                   FROM agent_edge a
                   JOIN dict_session p ON p.id = a.parent_session_id
                   JOIN dict_session c ON c.id = a.child_session_id
                   JOIN dict_edekind e ON e.id = a.edge_kind_id
                   WHERE (?1 IS NULL OR c.value = ?1 OR p.value = ?1)
                   ORDER BY p.value, c.value";
        let mut statement = self.connection.prepare(sql)?;
        let value: Option<String> = session.map(str::to_owned);
        let iter = statement.query_map(params![value], |row| {
            Ok(crate::rows::EdgeRow {
                parent: row.get(0)?,
                child: row.get(1)?,
                edge: row.get(2)?,
                first_ts: row.get(3)?,
                last_ts: row.get(4)?,
                n: row.get(5)?,
            })
        })?;
        Ok(iter.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Completed native children for registered parent sessions that have not
    /// reached native delivery. The route count bounds this lookup; it never
    /// scans mailbox history or unrelated completion edges.
    pub fn native_child_completion_outbox(
        &self,
        parent_sessions: &[String],
    ) -> Result<Vec<NativeChildCompletion>> {
        if parent_sessions.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat_n("?", parent_sessions.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT parent.value, child.value, completed.first_ts,
                    EXISTS(
                        SELECT 1 FROM agent_edge mailed
                        JOIN dict_edekind mailed_kind ON mailed_kind.id = mailed.edge_kind_id
                        WHERE mailed.parent_session_id = completed.parent_session_id
                          AND mailed.child_session_id = completed.child_session_id
                          AND mailed_kind.value = 'completion-mailed'
                    )
               FROM agent_edge completed
               JOIN dict_session parent ON parent.id = completed.parent_session_id
               JOIN dict_session child ON child.id = completed.child_session_id
               JOIN dict_edekind completed_kind ON completed_kind.id = completed.edge_kind_id
               WHERE completed_kind.value = 'completed'
                 AND parent.value IN ({placeholders})
                 AND NOT EXISTS(
                    SELECT 1 FROM agent_edge delivered
                    JOIN dict_edekind delivered_kind ON delivered_kind.id = delivered.edge_kind_id
                    WHERE delivered.parent_session_id = completed.parent_session_id
                      AND delivered.child_session_id = completed.child_session_id
                      AND delivered_kind.value = 'completion-delivered'
                 )
               ORDER BY parent.value, child.value"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows =
            statement.query_map(rusqlite::params_from_iter(parent_sessions.iter()), |row| {
                Ok(NativeChildCompletion {
                    parent_session: row.get(0)?,
                    child_session: row.get(1)?,
                    completed_at_ms: row.get::<_, i64>(2)? as u64,
                    mailed: row.get(3)?,
                })
            })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Record one liveness observation at `ts`. Maintains `agent_live` as the
    /// current-state cache and folds the interval into `agent_live_span`: a
    /// state change closes the open interval and opens a new one; a repeated
    /// identical observation extends and inserts nothing.
    pub fn record_status(
        &self,
        session: &str,
        ts: u64,
        status: &str,
        pid: Option<i64>,
        tmux_pane: Option<&str>,
    ) -> Result<()> {
        let sid = self.session_id(session)?;
        let status_id = self.intern("dict_status", status)?;
        let pane_id = match tmux_pane {
            Some(pane) => Some(self.intern("dict_pane", pane)?),
            None => None,
        };
        self.connection.execute(
            "INSERT INTO agent_live (session_id, pid, tmux_pane_id, status_id)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(session_id) DO UPDATE SET
               pid = excluded.pid,
               tmux_pane_id = excluded.tmux_pane_id,
               status_id = excluded.status_id",
            params![sid, pid, pane_id, status_id],
        )?;
        let open: Option<(i64, i64, Option<i64>, Option<i64>)> = self
            .connection
            .query_row(
                "SELECT from_ts, status_id, pid, tmux_pane_id FROM agent_live_span
                 WHERE session_id = ?1 AND to_ts IS NULL
                 ORDER BY from_ts DESC LIMIT 1",
                params![sid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let unchanged = open
            .as_ref()
            .map(|(_, st, pg, pn)| *st == status_id && *pg == pid && *pn == pane_id)
            .unwrap_or(false);
        if unchanged {
            return Ok(());
        }
        if let Some((from_ts, _, _, _)) = open {
            self.connection.execute(
                "UPDATE agent_live_span SET to_ts = ?2
                 WHERE session_id = ?1 AND from_ts = ?3",
                params![sid, ts as i64, from_ts],
            )?;
        }
        self.connection.execute(
            "INSERT INTO agent_live_span (session_id, from_ts, to_ts, status_id, pid, tmux_pane_id)
             VALUES (?1, ?2, NULL, ?3, ?4, ?5)",
            params![sid, ts as i64, status_id, pid, pane_id],
        )?;
        Ok(())
    }

    /// Record the door this session answers on, beside its liveness row. A
    /// session with no observation yet gets a row carrying the door alone.
    pub fn record_live_door(
        &self,
        session: &str,
        door_kind: &str,
        door_addr: Option<&str>,
    ) -> Result<()> {
        let sid = self.session_id(session)?;
        self.connection.execute(
            "INSERT INTO agent_live (session_id, door_kind, door_addr)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET
               door_kind = excluded.door_kind,
               door_addr = excluded.door_addr",
            params![sid, door_kind, door_addr],
        )?;
        Ok(())
    }

    /// The last liveness observation for one session, door included. `None`
    /// means this session has never been observed running.
    pub fn live_row(&self, session: &str) -> Result<Option<LiveRow>> {
        let row = self
            .connection
            .query_row(
                "SELECT dict_session.value, live.pid, pane.value, status.value,
                        live.door_kind, live.door_addr
                   FROM agent_live live
                   JOIN dict_session ON dict_session.id = live.session_id
                   LEFT JOIN dict_pane pane ON pane.id = live.tmux_pane_id
                   LEFT JOIN dict_status status ON status.id = live.status_id
                  WHERE dict_session.value = ?1",
                params![session],
                |row| {
                    Ok(LiveRow {
                        session: row.get(0)?,
                        pid: row.get(1)?,
                        tmux_pane: row.get(2)?,
                        status: row.get(3)?,
                        door_kind: row.get(4)?,
                        door_addr: row.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Record what one hail's door answered. Keyed on (message, route): a
    /// second delivery attempt of the same message overwrites its outcome.
    pub fn record_delivery(
        &self,
        message_id: &str,
        route: &str,
        harness: Option<crate::harness_id::HarnessId>,
        outcome: &str,
        detail: &str,
        at_ms: u64,
    ) -> Result<()> {
        let harness_id = harness
            .map(|id| self.intern("dict_harness", id.as_str()))
            .transpose()?;
        self.connection.execute(
            "INSERT INTO agent_delivery (message_id, route, harness_id, outcome, detail, at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(message_id, route) DO UPDATE SET
               harness_id = excluded.harness_id,
               outcome = excluded.outcome,
               detail = excluded.detail,
               at_ms = excluded.at_ms",
            params![message_id, route, harness_id, outcome, detail, at_ms as i64],
        )?;
        Ok(())
    }

    /// Every liveness interval for one session (or all when `session` is
    /// `None`), joined back to the TEXT status surface.
    pub fn live_span(&self, session: Option<&str>) -> Result<Vec<crate::rows::LiveSpanRow>> {
        let sql = "SELECT dict_session.value, d.value AS status,
                          s.from_ts, s.to_ts, s.pid, p.value AS tmux_pane
                   FROM agent_live_span s
                   JOIN dict_session ON dict_session.id = s.session_id
                   JOIN dict_status d ON d.id = s.status_id
                   LEFT JOIN dict_pane p ON p.id = s.tmux_pane_id
                   WHERE (?1 IS NULL OR dict_session.value = ?1)
                   ORDER BY dict_session.value, s.from_ts";
        let mut statement = self.connection.prepare(sql)?;
        let filter: Option<String> = session.map(str::to_owned);
        let iter = statement.query_map(params![filter], |row| {
            Ok(crate::rows::LiveSpanRow {
                session: row.get(0)?,
                status: row.get(1)?,
                from_ts: row.get(2)?,
                to_ts: row.get(3)?,
                pid: row.get(4)?,
                tmux_pane: row.get(5)?,
            })
        })?;
        let mut rows = Vec::new();
        for row in iter {
            rows.push(row?);
        }
        Ok(rows)
    }

    /// The interval active at a point in time, using the half-open rule
    /// `from_ts <= T AND (to_ts IS NULL OR to_ts > T)`.
    pub fn query_live_at(&self, at_ts: u64) -> Result<Vec<crate::rows::LiveSpanRow>> {
        let sql = "SELECT dict_session.value, d.value AS status, s.from_ts, s.to_ts, s.pid,
                          p.value AS tmux_pane
                   FROM agent_live_span s
                   JOIN dict_session ON dict_session.id = s.session_id
                   JOIN dict_status d ON d.id = s.status_id
                   LEFT JOIN dict_pane p ON p.id = s.tmux_pane_id
                   WHERE s.from_ts <= ?1 AND (s.to_ts IS NULL OR s.to_ts > ?1)
                   ORDER BY dict_session.value";
        let mut statement = self.connection.prepare(sql)?;
        let iter = statement.query_map(params![at_ts as i64], |row| {
            Ok(crate::rows::LiveSpanRow {
                session: row.get(0)?,
                status: row.get(1)?,
                from_ts: row.get(2)?,
                to_ts: row.get(3)?,
                pid: row.get(4)?,
                tmux_pane: row.get(5)?,
            })
        })?;
        let mut rows = Vec::new();
        for row in iter {
            rows.push(row?);
        }
        Ok(rows)
    }

    /// Table row counts, for receipts.
    pub fn counts(&self) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut out = serde_json::Map::new();
        for table in [
            "agent_session",
            "agent_turn",
            "agent_touch",
            "agent_cmd",
            "agent_fetch",
            "agent_skill",
            "agent_pr",
            "agent_usage",
        ] {
            let n: i64 =
                self.connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })?;
            out.insert(table.into(), serde_json::json!(n));
        }
        Ok(out)
    }

    pub fn db_bytes(&self) -> Result<u64> {
        let bytes: i64 = self
            .connection
            .pragma_query_value(None, "page_count", |row| row.get(0))?;
        let size: i64 = self
            .connection
            .pragma_query_value(None, "page_size", |row| row.get(0))?;
        Ok((bytes * size) as u64)
    }

    // ------------------------------------------------------------------
    // Queries (join ids back to the TEXT query surface)
    // ------------------------------------------------------------------

    /// Query turns with the shared filter set. Returns JSON rows with the TEXT
    /// query surface joined back out.
    pub fn query_turns(&self, query: &TurnQuery) -> Result<Vec<Row>> {
        let harness = query.harness.as_deref();
        let session = query.session.as_deref();
        let role = query.role.as_deref();
        let since = query.since;
        let until = query.until;
        let turn_from = query.turn_from;
        let turn_to = query.turn_to;
        let path = query.path.as_deref();
        let limit = query.limit;
        let mut sql = String::from(
            "SELECT s.value AS session, h.value AS harness, t.turn, t.ts, r.value AS role, t.said
             FROM agent_turn t
             JOIN dict_session s ON s.id = t.session_id
             JOIN dict_harness h ON h.id = (SELECT harness_id FROM agent_session a WHERE a.session_id = t.session_id)
             JOIN dict_role r ON r.id = t.role_id
             WHERE 1=1",
        );
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(harness) = harness {
            sql.push_str(" AND h.value = ?");
            values.push(harness.to_string().into());
        }
        if let Some(session) = session {
            sql.push_str(" AND t.session_id IN (SELECT id FROM dict_session WHERE value = ?)");
            values.push(session.to_string().into());
        }
        if let Some(role) = role {
            sql.push_str(" AND r.value = ?");
            values.push(role.to_string().into());
        }
        if let Some(since) = since {
            sql.push_str(" AND t.ts >= ?");
            values.push((since as i64).into());
        }
        if let Some(until) = until {
            sql.push_str(" AND t.ts <= ?");
            values.push((until as i64).into());
        }
        if let Some(turn_from) = turn_from {
            sql.push_str(" AND t.turn >= ?");
            values.push((turn_from as i64).into());
        }
        if let Some(turn_to) = turn_to {
            sql.push_str(" AND t.turn <= ?");
            values.push((turn_to as i64).into());
        }
        if let Some(path) = path {
            sql.push_str(
                " AND t.session_id IN (
                    SELECT DISTINCT tc.session_id FROM agent_touch tc
                    JOIN dict_path p ON p.id = tc.path_id WHERE p.value LIKE ?) ",
            );
            values.push(format!("{path}%").into());
        }
        sql.push_str(" ORDER BY t.session_id, t.turn");
        if let Some(limit) = limit {
            sql.push_str(" LIMIT ");
            sql.push_str(&limit.to_string());
        }
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = Vec::new();
        let iter = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok(serde_json::json!({
                "session": row.get::<_, String>(0)?,
                "harness": row.get::<_, String>(1)?,
                "turn": row.get::<_, i64>(2)?,
                "ts": row.get::<_, i64>(3)?,
                "role": row.get::<_, String>(4)?,
                "said": row.get::<_, String>(5)?,
            }))
        })?;
        for row in iter {
            rows.push(row?);
        }
        Ok(rows)
    }
}

/// Advance one session's cursor across whatever `ingest` wrote, and record the
/// session as observed. The caller supplies the projection because only the
/// harness layer above knows which adapter owns this transcript.
pub fn sync_session_with(
    store: &Store,
    session: &SessionRef,
    pid: Option<i64>,
    ingest: impl FnOnce(&Store, &SessionRef, u64) -> Result<Ingested>,
) -> Result<SyncStat> {
    let key = session.path.display().to_string();
    let from = store.get_cursor(&session.session_id, &key)?;
    store.set_cursor(&session.session_id, &key, from)?;
    let ingested = ingest(store, session, from)?;
    let observed_ts = session.modified_ms.max(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0),
    );
    store.record_status(
        &session.session_id,
        observed_ts,
        if session.tmux.is_some() {
            "live"
        } else {
            "idle"
        },
        pid,
        session.tmux.as_deref(),
    )?;
    store.project_discovered_session(session)?;
    store.set_cursor_modified(
        &session.session_id,
        &key,
        ingested.next_cursor,
        session.modified_ms,
    )?;
    Ok(ingested.stat)
}

/// The byte-offset transcript projection, which is every file-backed harness.
pub fn project_transcript(store: &Store, session: &SessionRef, from: u64) -> Result<Ingested> {
    let mut file = std::fs::File::open(&session.path)
        .map_err(|error| anyhow::anyhow!("open {}: {error}", session.path.display()))?;
    let result = crate::tail::read_complete_lines(&mut file, from)?;
    if result.lines.is_empty() {
        return Ok(Ingested {
            stat: SyncStat::default(),
            next_cursor: from,
        });
    }
    let mut walk = Walk {
        turn: store.max_turn(&session.session_id)?,
        stat: SyncStat::default(),
    };
    let session_id = store.session_id(&session.session_id)?;
    let path = session.path.display().to_string();
    let path_id = store.intern("dict_path", &path)?;
    let mut record_intern = store.connection.prepare_cached(
        "INSERT INTO dict_record (value) VALUES (?1)
         ON CONFLICT(value) DO UPDATE SET value = excluded.value
         RETURNING id",
    )?;
    let mut cursor_update = store.connection.prepare_cached(
        "UPDATE sync_cursor SET record_id_id = ?3, turn = ?4, timestamp = ?5
         WHERE session_id = ?1 AND path_id = ?2",
    )?;
    for line in &result.lines {
        project_line(store, session, line, &mut walk)?;
        let value: serde_json::Value = serde_json::from_slice(&line.bytes).unwrap_or_default();
        let record_id = value
            .get("uuid")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("record");
        let record_id_id: i64 =
            record_intern.query_row(rusqlite::params![record_id], |row| row.get(0))?;
        cursor_update.execute(rusqlite::params![
            session_id,
            path_id,
            record_id_id,
            walk.turn as i64,
            value
                .get("timestamp")
                .and_then(serde_json::Value::as_str)
                .and_then(crate::session::parse_iso_ms)
                .unwrap_or(0) as i64,
        ])?;
    }
    Ok(Ingested {
        stat: walk.stat,
        next_cursor: result.next_offset,
    })
}

/// The pieces an adapter needs to write turns and facts of its own.
impl Store {
    pub fn begin_walk(&self, session: &str) -> Result<u64> {
        self.max_turn(session)
    }

    #[allow(clippy::too_many_arguments)]
    /// Write one turn, interning the session and role. The store's e2e tests
    /// seed a source session through this; the resident `db sync` writer is
    /// the normal caller.
    pub fn write_turn(
        &self,
        session: &str,
        turn: u64,
        ts: u64,
        role: &str,
        said: &str,
    ) -> Result<usize> {
        self.add_turn(session, turn, ts, role, said)
    }

    pub fn write_tool_fact(
        &self,
        session: &str,
        turn: u64,
        ts: u64,
        name: &str,
        input: Option<&serde_json::Value>,
    ) -> Result<()> {
        emit_tool_fact(self, session, turn, ts, name, input)
    }

    pub fn write_usage(&self, session: &str, turn: u64, usage: &UsageRow) -> Result<(bool, bool)> {
        let (request_ref, is_new) = self.intern_request(usage.message_id, usage.request_id)?;
        let changed = self.add_usage(session, turn, request_ref, is_new, usage)?;
        Ok((is_new, changed))
    }
}

/// Walk one complete line and emit its turns and typed facts.
fn project_line(
    store: &Store,
    session: &SessionRef,
    line: &crate::tail::CompleteLine,
    walk: &mut Walk,
) -> Result<()> {
    let value: serde_json::Value = match serde_json::from_slice(&line.bytes) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    let object = match value.as_object() {
        Some(object) => object,
        None => return Ok(()),
    };
    let ts = object
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .and_then(crate::session::parse_iso_ms)
        .unwrap_or(0);
    let sid = session.session_id.clone();
    let record_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    if record_type == "pr-link" {
        let pr_url = object
            .get("prUrl")
            .or_else(|| object.get("url"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if !pr_url.is_empty() {
            walk.turn += 1;
            let inserted = store.add_turn(&sid, walk.turn, ts, "system", "")?;
            walk.record(inserted);
            store.add_pr(&sid, walk.turn, pr_url)?;
        }
        return Ok(());
    }

    if record_type != "user" && record_type != "assistant" {
        return Ok(());
    }
    let role = record_type;
    let Some(message) = object.get("message") else {
        return Ok(());
    };
    // `message` is an object with `content`, or a bare string on older user
    // records; both are one or more text/tool blocks.
    let blocks: Vec<serde_json::Value> = match message {
        serde_json::Value::String(text) => vec![serde_json::json!({"type":"text","text":text})],
        object => {
            let content = object.get("content");
            if let Some(text) = content.and_then(serde_json::Value::as_str) {
                vec![serde_json::json!({"type":"text","text":text})]
            } else {
                content
                    .and_then(serde_json::Value::as_array)
                    .map(|a| a.to_vec())
                    .unwrap_or_default()
            }
        }
    };
    let mut first_turn: Option<u64> = None;
    for block in &blocks {
        let Some(block) = block.as_object() else {
            continue;
        };
        let kind = block
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        // An ordinal is spent only when a row is written, so COUNT(*) equals
        // MAX(turn) per session and `turn` is a usable count.
        match kind {
            "text" => {
                let said = block
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                walk.turn += 1;
                let inserted = store.add_turn(&sid, walk.turn, ts, role, said)?;
                walk.record(inserted);
                first_turn.get_or_insert(walk.turn);
            }
            "tool_use" => {
                let name = block
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let input = block.get("input");
                walk.turn += 1;
                let inserted = store.add_turn(&sid, walk.turn, ts, "tool", "")?;
                walk.record(inserted);
                first_turn.get_or_insert(walk.turn);
                emit_tool_fact(store, &sid, walk.turn, ts, name, input)?;
            }
            _ => {}
        }
    }

    if let Some(usage) = parse_usage(object, message, ts) {
        let (request_ref, is_new) = store.intern_request(usage.message_id, usage.request_id)?;
        // Usage belongs to the whole response: it takes the first stored block's
        // ordinal, or its own when every block was thinking (31.1% measured).
        let turn = match (is_new, first_turn) {
            (false, _) => 0,
            (true, Some(turn)) => turn,
            (true, None) => {
                walk.turn += 1;
                let inserted = store.add_turn(&sid, walk.turn, ts, role, "")?;
                walk.record(inserted);
                walk.turn
            }
        };
        if store.add_usage(&sid, turn, request_ref, is_new, &usage)? {
            if is_new {
                walk.stat.usage_written += 1;
            } else {
                walk.stat.usage_updated += 1;
            }
        }
    }
    Ok(())
}

/// Read `message.usage` off one assistant record. `None` for every record that
/// is not an LLM response, which is every user record and every tool result.
fn parse_usage<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    message: &'a serde_json::Value,
    ts: u64,
) -> Option<UsageRow<'a>> {
    let usage = message.get("usage")?.as_object()?;
    let message_id = message.get("id").and_then(serde_json::Value::as_str)?;
    let count = |key: &str| -> i64 {
        usage
            .get(key)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    };
    let split = usage.get("cache_creation").and_then(|v| v.as_object());
    let split_count =
        |key: &str| -> Option<i64> { split?.get(key).and_then(serde_json::Value::as_i64) };
    // The flat cache_creation_input_tokens cannot say which window it bought,
    // so it falls into the 5m bucket; 99.7% of records carry the split.
    let (create_5m, create_1h) = match (
        split_count("ephemeral_5m_input_tokens"),
        split_count("ephemeral_1h_input_tokens"),
    ) {
        (None, None) => (count("cache_creation_input_tokens"), 0),
        (five, hour) => (five.unwrap_or(0), hour.unwrap_or(0)),
    };
    Some(UsageRow {
        ts,
        message_id,
        request_id: object
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
        model: message
            .get("model")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown"),
        service_tier: usage
            .get("service_tier")
            .and_then(serde_json::Value::as_str),
        input_tokens: count("input_tokens"),
        output_tokens: count("output_tokens"),
        cache_create_5m_tokens: create_5m,
        cache_create_1h_tokens: create_1h,
        cache_read_tokens: count("cache_read_input_tokens"),
        is_sidechain: object
            .get("isSidechain")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        cost_usd_recorded: object.get("costUSD").and_then(serde_json::Value::as_f64),
    })
}

fn emit_tool_fact(
    store: &Store,
    session: &str,
    turn: u64,
    ts: u64,
    name: &str,
    input: Option<&serde_json::Value>,
) -> Result<()> {
    let field = |key: &str| -> Option<String> {
        input
            .and_then(serde_json::Value::as_object)
            .and_then(|o| o.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    // The canonical verb is this lane's one normalization site: every adapter
    // funnels through write_tool_fact, so lowercasing happens here and nowhere
    // per-adapter. `name` stays the raw spelling stored alongside it.
    let verb = name.to_ascii_lowercase();
    match verb.as_str() {
        "read" | "write" | "edit" | "list" | "glob" | "multiedit" | "grep" => {
            let path = field("file_path")
                .or_else(|| field("filePath"))
                .or_else(|| field("pattern"))
                .or_else(|| field("path"));
            if let Some(path) = path {
                store.add_touch(session, turn, ts, &path, &verb, name)?;
            }
        }
        "bash" => {
            let command = field("command").unwrap_or_default();
            if !command.is_empty() {
                let mut parts = command.splitn(2, char::is_whitespace);
                let program = parts.next().unwrap_or("").to_owned();
                let argline = parts.next().unwrap_or("").to_owned();
                store.add_cmd(session, turn, ts, &program, &argline)?;
            }
        }
        "webfetch" => {
            if let Some(url) = field("url") {
                let domain = domain_of(&url);
                store.add_fetch(session, turn, ts, &url, &domain)?;
            }
        }
        "websearch" => {
            if let Some(query) = field("query").or_else(|| field("q")) {
                store.add_search(session, turn, ts, &query)?;
            }
        }
        "skill" => {
            let skill = field("skill").or_else(|| field("name")).unwrap_or_default();
            if !skill.is_empty() {
                store.add_skill(session, turn, &skill)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn domain_of(url: &str) -> String {
    let remainder = url.split("://").nth(1).unwrap_or(url);
    remainder
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_owned()
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS dict_session (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_harness (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_cwd (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_branch (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_role (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_path (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_verb (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_program (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_url (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_domain (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_netkind (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_skill (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_pr (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_edekind (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_agenttype (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_status (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_pane (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_model (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_service_tier (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_price_source (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_record (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

-- request_id is NOT NULL with '' for absent: SQLite treats NULLs in a UNIQUE
-- index as distinct, and 8.4% of measured records carry no requestId.
CREATE TABLE IF NOT EXISTS dict_request (
  id INTEGER PRIMARY KEY,
  message_id TEXT NOT NULL,
  request_id TEXT NOT NULL DEFAULT '',
  UNIQUE (message_id, request_id)
);

CREATE TABLE IF NOT EXISTS dict_trace (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_attach (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_trace_kind (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_trace_delivery (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_trace_classification (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE IF NOT EXISTS markdown_cache (
  markdown_id INTEGER PRIMARY KEY,
  digest TEXT NOT NULL UNIQUE,
  body TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  first_ts INTEGER NOT NULL
);

-- User-pinned markdown, the one user-authored state in the store: no
-- transcript re-projects it, so rebuild() carries it across the drop by value.
-- source is plain text (a session id, a url, whatever the user typed), never a
-- dict id, so re-import needs no id remap.
CREATE TABLE IF NOT EXISTS agent_favorite (
  favorite_id INTEGER PRIMARY KEY,
  markdown_id INTEGER NOT NULL,
  note TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL DEFAULT '',
  created_ts INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_trace (
  trace_id INTEGER PRIMARY KEY,
  root_session_id INTEGER,
  started_ts INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_trace_span (
  session_id INTEGER PRIMARY KEY,
  trace_id INTEGER NOT NULL,
  attach_id INTEGER NOT NULL,
  attached_ts INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_span_trace ON agent_trace_span(trace_id);

CREATE TABLE IF NOT EXISTS agent_lane (
  spawn_id INTEGER PRIMARY KEY,
  lane_id INTEGER NOT NULL,
  trace_id INTEGER,
  harness_id INTEGER,
  branch_id INTEGER,
  cwd_id INTEGER,
  model_id INTEGER,
  parent_lane_id INTEGER,
  goal TEXT,
  brief_path_id INTEGER,
  brief_markdown_id INTEGER,
  spawned_ts INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_lane_trace ON agent_lane(trace_id);
CREATE INDEX IF NOT EXISTS idx_lane_lane ON agent_lane(lane_id, spawned_ts);

CREATE TABLE IF NOT EXISTS agent_trace_event (
  event_id INTEGER PRIMARY KEY,
  event_key TEXT NOT NULL UNIQUE,
  lane_id INTEGER NOT NULL REFERENCES dict_session(id),
  trace_id INTEGER REFERENCES dict_trace(id),
  session_id INTEGER REFERENCES dict_session(id),
  from_lane_id INTEGER REFERENCES dict_session(id),
  to_lane_id INTEGER REFERENCES dict_session(id),
  kind_id INTEGER NOT NULL REFERENCES dict_trace_kind(id),
  started_ts INTEGER,
  finished_ts INTEGER,
  delivery_state_id INTEGER REFERENCES dict_trace_delivery(id),
  classification_id INTEGER REFERENCES dict_trace_classification(id),
  detail TEXT NOT NULL DEFAULT '',
  created_ts INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_trace_event_lane_time
  ON agent_trace_event(lane_id, created_ts, event_id);
CREATE INDEX IF NOT EXISTS idx_trace_event_trace_time
  ON agent_trace_event(trace_id, created_ts, event_id);

CREATE TABLE IF NOT EXISTS agent_session (
  session_id INTEGER PRIMARY KEY,
  harness_id INTEGER NOT NULL,
  nickname TEXT,
  cwd_id INTEGER,
  branch_id INTEGER,
  started_ts INTEGER
);
CREATE INDEX IF NOT EXISTS idx_session_cwd ON agent_session(cwd_id);

CREATE TABLE IF NOT EXISTS agent_turn (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  role_id INTEGER NOT NULL,
  said TEXT,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS agent_touch (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  path_id INTEGER NOT NULL,
  verb_id INTEGER NOT NULL,
  raw_verb_id INTEGER,
  PRIMARY KEY (session_id, turn, path_id, verb_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_touch_pathid ON agent_touch(path_id);

CREATE TABLE IF NOT EXISTS agent_cmd (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  program_id INTEGER NOT NULL,
  argline TEXT,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

-- One row per outbound network act. A search has no url or domain and a fetch
-- has no query; `query` is payload text, never a key.
CREATE TABLE IF NOT EXISTS agent_fetch (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  kind_id INTEGER NOT NULL,
  url_id INTEGER,
  domain_id INTEGER,
  query TEXT,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_fetch_ts ON agent_fetch(ts);

CREATE TABLE IF NOT EXISTS agent_skill (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  skill_id INTEGER NOT NULL,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS agent_pr (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  pr_url_id INTEGER NOT NULL,
  PRIMARY KEY (session_id, turn, pr_url_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS agent_span (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  path_id INTEGER NOT NULL,
  line_start INTEGER,
  line_end INTEGER,
  PRIMARY KEY (session_id, turn, path_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS agent_edge (
  parent_session_id INTEGER NOT NULL,
  child_session_id INTEGER NOT NULL,
  edge_kind_id INTEGER NOT NULL,
  agent_type_id INTEGER,
  model_id INTEGER,
  first_ts INTEGER,
  last_ts INTEGER,
  n INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (parent_session_id, child_session_id, edge_kind_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_edge_child ON agent_edge(child_session_id, edge_kind_id);

CREATE TABLE IF NOT EXISTS agent_usage (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER NOT NULL,
  request_ref INTEGER NOT NULL,
  model_id INTEGER NOT NULL,
  service_tier_id INTEGER,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cache_create_5m_tokens INTEGER NOT NULL DEFAULT 0,
  cache_create_1h_tokens INTEGER NOT NULL DEFAULT 0,
  cache_read_tokens INTEGER NOT NULL DEFAULT 0,
  is_sidechain INTEGER NOT NULL DEFAULT 0,
  cost_usd_recorded REAL,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;
CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_request ON agent_usage(request_ref);
CREATE INDEX IF NOT EXISTS idx_usage_ts ON agent_usage(ts);
CREATE INDEX IF NOT EXISTS idx_usage_model_ts ON agent_usage(model_id, ts);

-- Rates are USD per MILLION tokens: LiteLLM's per-token values are readable
-- only when scaled, and REAL keeps the precision either way.
CREATE TABLE IF NOT EXISTS model_price (
  model_id INTEGER PRIMARY KEY,
  input_per_mtok REAL NOT NULL,
  output_per_mtok REAL NOT NULL,
  cache_write_5m_per_mtok REAL NOT NULL,
  cache_write_1h_per_mtok REAL NOT NULL,
  cache_read_per_mtok REAL NOT NULL,
  source_id INTEGER NOT NULL,
  fetched_ts INTEGER NOT NULL
);

-- door_kind and door_addr are the address a hail is delivered to: the
-- harness's own control plane, as its LiveSessions pass observed it. Both
-- stay TEXT: an address is payload, never a JOIN key.
CREATE TABLE IF NOT EXISTS agent_live (
  session_id INTEGER PRIMARY KEY,
  pid INTEGER,
  tmux_pane_id INTEGER,
  status_id INTEGER,
  door_kind TEXT,
  door_addr TEXT
);

-- One row per hail put in front of a recipient: what the door answered, keyed
-- on the message and the route it was addressed to, so a re-delivery of the
-- same message to the same route overwrites rather than piles up. `detail`
-- carries an Unreachable reason and is '' for every other outcome.
CREATE TABLE IF NOT EXISTS agent_delivery (
  message_id TEXT NOT NULL,
  route TEXT NOT NULL,
  harness_id INTEGER,
  outcome TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT '',
  at_ms INTEGER NOT NULL,
  PRIMARY KEY (message_id, route)
) WITHOUT ROWID;

-- Historical liveness: [from_ts, to_ts) intervals, open when to_ts IS NULL,
-- folded from observations so a state change closes an interval and repeated
-- identical observations extend nothing.
CREATE TABLE IF NOT EXISTS agent_live_span (
  session_id INTEGER NOT NULL,
  from_ts INTEGER NOT NULL,
  to_ts INTEGER,
  status_id INTEGER NOT NULL,
  pid INTEGER,
  tmux_pane_id INTEGER,
  PRIMARY KEY (session_id, from_ts)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_live_span_open ON agent_live_span(session_id, to_ts);

CREATE TABLE IF NOT EXISTS sync_cursor (
  session_id INTEGER NOT NULL,
  path_id INTEGER NOT NULL,
  offset INTEGER NOT NULL,
  record_id_id INTEGER,
  turn INTEGER NOT NULL DEFAULT 0,
  timestamp INTEGER NOT NULL DEFAULT 0,
  modified_ms INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (session_id, path_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS sync_root_stamp (
  harness_id INTEGER NOT NULL,
  root_path_id INTEGER NOT NULL,
  mtime_ms INTEGER NOT NULL,
  PRIMARY KEY (harness_id, root_path_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS dict_attr_key (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_mood_name (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE IF NOT EXISTS agent_session_attr (
  session_id INTEGER NOT NULL,
  key_id INTEGER NOT NULL,
  value TEXT NOT NULL,
  set_ts INTEGER NOT NULL,
  PRIMARY KEY (session_id, key_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS mood (
  id INTEGER PRIMARY KEY,
  name_id INTEGER NOT NULL UNIQUE,
  template TEXT NOT NULL
);
";

#[cfg(test)]
mod tests {
    use crate::harness_id::HarnessId;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Barrier};
    use std::time::Duration;

    use crate::session::SessionRef;

    use rusqlite::params;
    use rusqlite::trace::{TraceEvent, TraceEventCodes};

    use super::{
        project_transcript, sync_session_with, Store, TraceEvent as LaneTraceEvent, BUSY_TIMEOUT,
        SCHEMA_VERSION,
    };

    static CURSOR_SQL: AtomicUsize = AtomicUsize::new(0);
    static MOOD_SQL: AtomicUsize = AtomicUsize::new(0);

    /// Every statement that reads the attribute rows, whatever else it names.
    fn count_mood_sql(event: TraceEvent<'_>) {
        if let TraceEvent::Stmt(_, sql) = event {
            if sql.contains("agent_session_attr") {
                MOOD_SQL.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn count_cursor_sql(event: TraceEvent<'_>) {
        if let TraceEvent::Stmt(_, sql) = event {
            if sql.contains("dict_record") || sql.contains("sync_cursor") {
                CURSOR_SQL.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("boop_rel_{}_{}", std::process::id(), name))
    }

    fn fresh_store(name: &str) -> (PathBuf, Store) {
        let path = temp_path(name);
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        (path, store)
    }

    fn trace_event(key: &str, created_ts: u64) -> LaneTraceEvent {
        LaneTraceEvent {
            event_key: key.into(),
            lane: "lane-a".into(),
            trace: Some("trace-a".into()),
            session: Some("session-a".into()),
            kind: "turn-finish".into(),
            from_lane: Some("parent-a".into()),
            to_lane: Some("lane-a".into()),
            started_ts: None,
            finished_ts: None,
            delivery_state: Some("nextturn".into()),
            classification: Some("completed".into()),
            detail: "diagnostic".into(),
            created_ts,
        }
    }

    #[test]
    fn trace_events_round_trip_stable_identity_endpoints_and_absent_times() {
        let (path, store) = fresh_store("trace-events");
        let event = trace_event("trace-a/lane-a/run-1/event-1", 20);
        let first_id = store.record_trace_event(&event).unwrap();
        assert_eq!(store.record_trace_event(&event).unwrap(), first_id);
        let rows = store.query_trace_events(Some("lane-a"), 20).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].event_key, event.event_key);
        assert_eq!(rows[0].from_lane.as_deref(), Some("parent-a"));
        assert_eq!(rows[0].to_lane.as_deref(), Some("lane-a"));
        assert_eq!(rows[0].started_ts, None);
        assert_eq!(rows[0].finished_ts, None);
        assert_eq!(rows[0].delivery_state.as_deref(), Some("nextturn"));
        assert_eq!(rows[0].classification.as_deref(), Some("completed"));
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn trace_event_diagnostic_is_redacted_and_bounded() {
        let (path, store) = fresh_store("trace-event-detail");
        let mut event = trace_event("trace-a/lane-a/run-1/event-2", 20);
        event.detail = format!("token=secret-value {}", "x".repeat(700));
        store.record_trace_event(&event).unwrap();
        let row = store
            .query_trace_events(Some("lane-a"), 1)
            .unwrap()
            .remove(0);
        assert!(row.detail.len() <= 512);
        assert!(!row.detail.contains("secret-value"));
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn trace_event_retention_is_deterministic_and_preserves_newest_rows() {
        let (path, store) = fresh_store("trace-event-retention");
        for (key, ts) in [("event-a", 10), ("event-b", 10), ("event-c", 11)] {
            store.record_trace_event(&trace_event(key, ts)).unwrap();
        }
        assert_eq!(store.prune_trace_events(2).unwrap(), 1);
        let rows = store.query_trace_events(Some("lane-a"), 20).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.event_key.as_str())
                .collect::<Vec<_>>(),
            vec!["event-b", "event-c"]
        );
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn v11_open_keeps_existing_rows_when_migration_is_reentered() {
        let (path, store) = fresh_store("trace-event-migration");
        store
            .add_edge_at("old-parent", "old-child", "spawned", 7)
            .unwrap();
        store
            .connection
            .execute_batch("PRAGMA user_version = 10")
            .unwrap();
        drop(store);
        let migrated = Store::open(path.clone()).unwrap();
        assert_eq!(migrated.schema_version().unwrap(), super::SCHEMA_VERSION);
        let edges = migrated.query_edges(None).unwrap();
        assert_eq!(edges.len(), 1);
        drop(migrated);
        let _ = std::fs::remove_file(&path);
    }

    /// Spawn `lane` under `parent`, which is all the mood cascade reads.
    fn spawn_under(store: &Store, lane: &str, parent: Option<&str>) {
        store
            .record_lane_spawn(&super::LaneSpawn {
                lane: lane.to_owned(),
                parent: parent.map(str::to_owned),
                ts: 1,
                ..Default::default()
            })
            .unwrap();
    }

    #[test]
    fn a_fresh_store_carries_the_seed_moods() {
        let (path, store) = fresh_store("mood-seeds");
        assert_eq!(store.mood_names().unwrap(), vec!["board", "plain", "unga"]);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_session_with_no_attr_row_anywhere_takes_the_default() {
        let (path, store) = fresh_store("mood-default");
        let mood = store.effective_mood("lonely").unwrap();
        assert_eq!(mood.name, super::DEFAULT_MOOD);
        assert_eq!(mood.template, super::DEFAULT_MOOD_TEMPLATE);
        assert_eq!(mood.set_by, None);
        assert_eq!(mood.line(), "mood: plain (set by default)");
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_session_mood_round_trips_and_clears() {
        let (path, store) = fresh_store("mood-round-trip");
        store.set_session_mood("coord", "unga", 10).unwrap();
        let set = store.effective_mood("coord").unwrap();
        assert_eq!(set.name, "unga");
        assert_eq!(set.set_by.as_deref(), Some("coord"));
        assert_eq!(set.line(), "mood: unga (set by coord)");
        // Last write wins on one row, never a second row.
        store.set_session_mood("coord", "board", 11).unwrap();
        let rows: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM agent_session_attr", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(rows, 1);
        assert_eq!(store.effective_mood("coord").unwrap().name, "board");
        assert!(store
            .clear_session_attr("coord", super::MOOD_ATTR_KEY)
            .unwrap());
        assert_eq!(
            store.effective_mood("coord").unwrap().name,
            super::DEFAULT_MOOD
        );
        assert!(!store
            .clear_session_attr("coord", super::MOOD_ATTR_KEY)
            .unwrap());
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_unknown_mood_name_names_the_known_ones() {
        let (path, store) = fresh_store("mood-unknown");
        let error = store.set_session_mood("coord", "shouty", 10).unwrap_err();
        assert_eq!(
            error.to_string(),
            "unknown mood shouty; known moods: board, plain, unga"
        );
        assert_eq!(
            store.effective_mood("coord").unwrap().name,
            super::DEFAULT_MOOD
        );
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// A mood set at the root reaches every lane under it, and a lane that
    /// sets its own takes it back from there down.
    #[test]
    fn a_lane_takes_the_nearest_ancestor_mood() {
        let (path, store) = fresh_store("mood-cascade");
        spawn_under(&store, "middle", Some("root"));
        spawn_under(&store, "leaf", Some("middle"));
        store.set_session_mood("root", "unga", 10).unwrap();

        for session in ["root", "middle", "leaf"] {
            let mood = store.effective_mood(session).unwrap();
            assert_eq!(mood.name, "unga", "{session} lost the root mood");
            assert_eq!(mood.set_by.as_deref(), Some("root"));
        }

        store.set_session_mood("middle", "board", 11).unwrap();
        assert_eq!(store.effective_mood("root").unwrap().name, "unga");
        for session in ["middle", "leaf"] {
            let mood = store.effective_mood(session).unwrap();
            assert_eq!(mood.name, "board", "{session} missed the nearer mood");
            assert_eq!(mood.set_by.as_deref(), Some("middle"));
        }
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// COUNT, not end state: the walk is one recursive CTE, so a chain twice as
    /// deep still costs one statement.
    #[test]
    fn resolving_the_effective_mood_is_one_statement() {
        let (path, store) = fresh_store("mood-statement-count");
        let mut parent = "root".to_owned();
        for step in 0..8 {
            let child = format!("lane-{step}");
            spawn_under(&store, &child, Some(&parent));
            parent = child;
        }
        store.set_session_mood("root", "unga", 10).unwrap();

        MOOD_SQL.store(0, Ordering::Relaxed);
        store
            .connection()
            .trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(count_mood_sql));
        let mood = store.effective_mood(&parent).unwrap();
        store.connection().trace_v2(TraceEventCodes::empty(), None);

        assert_eq!(mood.name, "unga");
        assert_eq!(MOOD_SQL.load(Ordering::Relaxed), 1);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// A cycle in the lane tree is a bounded walk, never a hang.
    #[test]
    fn a_cycle_in_the_lane_tree_still_answers() {
        let (path, store) = fresh_store("mood-cycle");
        spawn_under(&store, "one", Some("two"));
        spawn_under(&store, "two", Some("one"));
        assert_eq!(
            store.effective_mood("one").unwrap().name,
            super::DEFAULT_MOOD
        );
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn v13_open_seeds_moods_once_and_keeps_an_edited_template() {
        let (path, store) = fresh_store("mood-migration");
        store.set_session_mood("coord", "unga", 10).unwrap();
        store
            .connection
            .execute_batch(
                "UPDATE mood SET template = 'edited {body}'
                   WHERE name_id = (SELECT id FROM dict_mood_name WHERE value = 'unga');
                 PRAGMA user_version = 12;",
            )
            .unwrap();
        drop(store);

        let migrated = Store::open(path.clone()).unwrap();
        assert_eq!(migrated.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(
            migrated.mood_names().unwrap(),
            vec!["board", "plain", "unga"]
        );
        let mood = migrated.effective_mood("coord").unwrap();
        assert_eq!(mood.name, "unga");
        assert_eq!(mood.template, "edited {body}");
        drop(migrated);
        let _ = std::fs::remove_file(&path);
    }

    /// A resident sync writer holds the write lock in bursts; both open paths
    /// must wait it out instead of surfacing "database is locked".
    #[test]
    fn both_open_paths_carry_a_busy_timeout() {
        let (path, _store) = fresh_store("busy");
        for store in [
            Store::open(path.clone()).unwrap(),
            Store::open_readonly(path.clone()).unwrap(),
        ] {
            let ms: i64 = store
                .connection
                .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
                .unwrap();
            assert_eq!(ms, BUSY_TIMEOUT.as_millis() as i64);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn current_schema_open_and_readonly_query_do_not_wait_for_a_wal_writer() {
        let (path, writer) = fresh_store("current-open-during-writer");
        writer.begin().unwrap();
        let (opened_tx, opened_rx) = mpsc::channel();
        let open_path = path.clone();
        std::thread::spawn(move || opened_tx.send(Store::open(open_path)).unwrap());
        let opened = opened_rx
            .recv_timeout(Duration::from_millis(250))
            .expect("current-schema open must not mutate schema or journal")
            .unwrap();
        drop(opened);

        let read_only = Store::open_readonly(path.clone()).unwrap();
        let count: i64 = read_only
            .connection()
            .query_row("SELECT COUNT(*) FROM agent_turn", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        writer.rollback().unwrap();
        drop(read_only);
        drop(writer);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn concurrent_first_open_converges_on_one_current_schema() {
        let path = temp_path("concurrent-first-open");
        let _ = std::fs::remove_file(&path);
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                Store::open(path)
            }));
        }
        barrier.wait();
        for worker in workers {
            assert_eq!(
                worker.join().unwrap().unwrap().schema_version().unwrap(),
                SCHEMA_VERSION
            );
        }
        let store = Store::open(path.clone()).unwrap();
        let tables: i64 = store
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='agent_turn'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 1);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn trace_and_transcript_style_writers_complete_with_all_rows() {
        let path = temp_path("concurrent-trace-sync-writers");
        let _ = std::fs::remove_file(&path);
        Store::open(path.clone()).unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let trace_path = path.clone();
        let trace_barrier = Arc::clone(&barrier);
        let trace_writer = std::thread::spawn(move || -> anyhow::Result<()> {
            let store = Store::open(trace_path)?;
            trace_barrier.wait();
            for index in 0..16 {
                store.record_trace_event(&trace_event(&format!("trace-event-{index}"), index))?;
            }
            Ok(())
        });
        let sync_path = path.clone();
        let sync_barrier = Arc::clone(&barrier);
        let sync_writer = std::thread::spawn(move || -> anyhow::Result<()> {
            let store = Store::open(sync_path)?;
            sync_barrier.wait();
            for turn in 1..=16 {
                store.write_turn("sync-session", turn, turn, "assistant", "fact")?;
            }
            Ok(())
        });
        barrier.wait();
        trace_writer.join().unwrap().unwrap();
        sync_writer.join().unwrap().unwrap();
        let store = Store::open_readonly(path.clone()).unwrap();
        let counts: (i64, i64) = store
            .connection()
            .query_row(
                "SELECT (SELECT COUNT(*) FROM agent_trace_event), (SELECT COUNT(*) FROM agent_turn)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, (16, 16));
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn writable_store_persists_wal_mode() {
        let (path, store) = fresh_store("wal");
        let mode: String = store
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        drop(store);

        let read_only = Store::open_readonly(path.clone()).unwrap();
        let reopened_mode: String = read_only
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(reopened_mode, "wal");
        drop(read_only);
        let _ = std::fs::remove_file(&path);
    }

    /// The user's word: "markdown_cache is own table for reason". Twenty lanes
    /// reading one brief must leave one row.
    #[test]
    fn the_same_brief_interns_once_however_often_it_is_spawned() {
        let (path, store) = fresh_store("md");
        let body = "# brief\n\ndo the thing\n";
        let first = store.intern_markdown(body, 1).unwrap();
        let second = store.intern_markdown(body, 2).unwrap();
        let other = store.intern_markdown("# other brief\n", 3).unwrap();
        assert_eq!(first, second);
        assert_ne!(first, other);
        let rows: i64 = store
            .connection
            .query_row("SELECT count(*) FROM markdown_cache", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 2);
        let bytes: i64 = store
            .connection
            .query_row(
                "SELECT bytes FROM markdown_cache WHERE markdown_id = ?1",
                params![first],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(bytes, body.len() as i64);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// A session id moves on clear, compact and resume; the trace does not.
    #[test]
    fn one_trace_holds_every_session_id_a_lane_wears() {
        let (path, store) = fresh_store("trace");
        store
            .attach_trace("lane-a", "trace-lane-a", "lane-create", 10)
            .unwrap();
        store
            .attach_trace("ses-1", "trace-lane-a", "supervisor-conversation", 11)
            .unwrap();
        store
            .attach_trace("ses-2", "trace-lane-a", "supervisor-conversation", 12)
            .unwrap();
        assert_eq!(
            store.trace_sessions("trace-lane-a").unwrap(),
            vec!["lane-a".to_owned(), "ses-1".to_owned(), "ses-2".to_owned()]
        );
        assert_eq!(
            store.trace_of("ses-2").unwrap().as_deref(),
            Some("trace-lane-a")
        );
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// Re-attaching is how two unrelated arcs would silently merge, so the
    /// first attach wins and the second is a no-op.
    #[test]
    fn a_session_never_moves_to_a_second_trace() {
        let (path, store) = fresh_store("trace2");
        store
            .attach_trace("ses-1", "trace-one", "lane-create", 10)
            .unwrap();
        store
            .attach_trace("ses-1", "trace-two", "lane-create", 11)
            .unwrap();
        assert_eq!(
            store.trace_of("ses-1").unwrap().as_deref(),
            Some("trace-one")
        );
        assert!(store.trace_sessions("trace-two").unwrap().is_empty());
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// The brief on disk is edited after the lane runs; the stored copy is the
    /// one the lane was actually given.
    #[test]
    fn a_lane_spawn_keeps_the_goal_the_path_and_the_brief_bytes() {
        let (path, store) = fresh_store("lane");
        let spawn = super::LaneSpawn {
            lane: "chore-x".into(),
            trace: Some("trace-chore-x".into()),
            harness: Some("claude".into()),
            branch: Some("chore/x".into()),
            cwd: Some("/repo".into()),
            model: Some("haiku".into()),
            parent: Some("coordinator".into()),
            goal: Some("make the gate green".into()),
            brief_path: Some("/tmp/brief.md".into()),
            brief_body: Some("first version".into()),
            ts: 42,
        };
        store.record_lane_spawn(&spawn).unwrap();
        let mut second = spawn.clone();
        second.brief_body = Some("second version".into());
        second.ts = 43;
        store.record_lane_spawn(&second).unwrap();
        let bodies: Vec<String> = {
            let mut statement = store
                .connection
                .prepare(
                    "SELECT m.body FROM agent_lane l
                       JOIN markdown_cache m ON m.markdown_id = l.brief_markdown_id
                       JOIN dict_session d ON d.id = l.lane_id
                      WHERE d.value = 'chore-x' ORDER BY l.spawned_ts",
                )
                .unwrap();
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap();
            rows.map(|row| row.unwrap()).collect()
        };
        assert_eq!(bodies, vec!["first version", "second version"]);
        let goal: String = store
            .connection
            .query_row("SELECT goal FROM agent_lane LIMIT 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(goal, "make the gate green");
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// Backfill unions `spawned` edges into one trace per component and leaves
    /// an edgeless session alone rather than guessing from timing.
    #[test]
    fn backfill_traces_one_component_and_skips_the_edgeless() {
        let (path, store) = fresh_store("backfill");
        store.add_edge_at("root", "kid-a", "spawned", 100).unwrap();
        store.add_edge_at("kid-a", "kid-b", "spawned", 101).unwrap();
        store.add_edge_at("other", "kid-c", "spawned", 102).unwrap();
        store.session_id("loner").unwrap();
        store.backfill_traces().unwrap();
        let root_trace = store.trace_of("root").unwrap().unwrap();
        assert_eq!(
            store.trace_of("kid-a").unwrap().as_deref(),
            Some(root_trace.as_str())
        );
        assert_eq!(
            store.trace_of("kid-b").unwrap().as_deref(),
            Some(root_trace.as_str())
        );
        assert_ne!(store.trace_of("kid-c").unwrap().unwrap(), root_trace);
        assert_eq!(store.trace_of("loner").unwrap(), None);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// RECEIPT (Job 1). The passthrough runs raw SELECT over a temp store and
    /// hands back columns plus one JSON object per row.
    #[test]
    fn passthrough_select_returns_rows() {
        let db_path = temp_path("pass");
        let _ = std::fs::remove_file(&db_path);
        let writable = Store::open(db_path.clone()).unwrap();
        drop(writable);
        let store = Store::open_readonly(db_path.clone()).unwrap();
        let (names, rows) = store
            .passthrough("SELECT 1 AS x, 'hi' AS y")
            .expect("SELECT runs read-only");
        assert_eq!(names, vec!["x".to_owned(), "y".to_owned()]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["x"], serde_json::json!(1));
        assert_eq!(rows[0]["y"], "hi");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// RECEIPT (Job 1). A write through the passthrough is refused by the
    /// SQLITE_OPEN_READONLY open itself, not by inspecting the SQL.
    #[test]
    fn passthrough_update_is_refused_read_only() {
        let db_path = temp_path("passro");
        let _ = std::fs::remove_file(&db_path);
        let writable = Store::open(db_path.clone()).unwrap();
        drop(writable);
        let store = Store::open_readonly(db_path.clone()).unwrap();
        let result = store.passthrough("UPDATE agent_usage SET input_tokens = 1");
        let message = result
            .expect_err("a write must be refused read-only")
            .to_string();
        assert!(
            message.contains("readonly") || message.contains("read only"),
            "SQLite must name the read-only refusal: {message}"
        );
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    fn session_for(path: &std::path::Path) -> SessionRef {
        SessionRef {
            harness: HarnessId::Claude,
            session_id: "ses-1".to_owned(),
            nickname: "ses-1".to_owned(),
            path: path.to_path_buf(),
            cwd: Some("/w".to_owned()),
            git_branch: Some("main".to_owned()),
            modified_ms: 0,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: None,
        }
    }

    #[test]
    fn discovery_projects_empty_unchanged_session_and_parent_edge() {
        let (path, store) = fresh_store("discovery-empty");
        let transcript = temp_path("discovery-empty.jsonl");
        let _ = std::fs::remove_file(&transcript);
        std::fs::File::create(&transcript).unwrap();
        let session = SessionRef {
            harness: HarnessId::Claude,
            session_id: "empty-child".into(),
            nickname: "empty-child".into(),
            path: transcript.clone(),
            cwd: Some("/repo".into()),
            git_branch: None,
            modified_ms: 7,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: Some("empty-parent".into()),
        };
        store.project_discovered_session(&session).unwrap();
        let graph_row: i64 = store
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM agent_session s JOIN dict_session d ON d.id = s.session_id WHERE d.value = 'empty-child'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(graph_row, 1);
        assert_eq!(store.edge_rows(Some("empty-child")).unwrap().len(), 1);
        let _ = std::fs::remove_file(transcript);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn known_sessions_restore_candidate_metadata_and_cursor() {
        let (path, store) = fresh_store("known-sessions");
        let transcript = temp_path("known-sessions.jsonl");
        let session = SessionRef {
            harness: HarnessId::Claude,
            session_id: "known-child".into(),
            nickname: "known-name".into(),
            path: transcript.clone(),
            cwd: Some("/repo".into()),
            git_branch: Some("main".into()),
            modified_ms: 7,
            size: 12,
            tmux: None,
            tmux_socket: None,
            parent: Some("known-parent".into()),
        };
        store.project_discovered_session(&session).unwrap();
        store
            .set_cursor(&session.session_id, &transcript.display().to_string(), 12)
            .unwrap();

        let known = store.known_sessions().unwrap();
        let candidate = known.get(&transcript).expect("persisted candidate");
        assert_eq!(candidate.session_id, "known-child");
        assert_eq!(candidate.nickname, "known-name");
        assert_eq!(candidate.cwd.as_deref(), Some("/repo"));
        assert_eq!(candidate.git_branch.as_deref(), Some("main"));
        assert_eq!(candidate.parent.as_deref(), Some("known-parent"));
        assert_eq!(candidate.cursor, 12);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    /// Guards the per-tick cost of the native TUI projector pass
    /// (`boop::cli::control::run_native_tui` calls `known_sessions` on every
    /// pass). Seeds a store shaped like production (4k sessions, half with a
    /// spawned parent edge) and runs the query three times. Any run above
    /// `KNOWN_SESSIONS_BUDGET_MS` fails the test; `cargo test` then exits 1.
    /// Override the budget with `BOOP_KNOWN_SESSIONS_BUDGET_MS`.
    #[test]
    fn known_sessions_stays_under_budget() {
        const SESSIONS: usize = 4000;
        const RUNS: usize = 3;
        const KNOWN_SESSIONS_BUDGET_MS: u128 = 60;
        let budget_ms: u128 = std::env::var("BOOP_KNOWN_SESSIONS_BUDGET_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(KNOWN_SESSIONS_BUDGET_MS);

        let (path, store) = fresh_store("known-sessions-budget");
        store.begin().unwrap();
        for i in 0..SESSIONS {
            let sid = format!("s-{i}");
            let transcript = format!("/tmp/budget/{sid}.jsonl");
            let session = SessionRef {
                harness: HarnessId::Codex,
                session_id: sid.clone(),
                nickname: format!("n-{i}"),
                path: PathBuf::from(&transcript),
                cwd: Some("/repo".into()),
                git_branch: Some("main".into()),
                modified_ms: i as u64,
                size: 1,
                tmux: None,
                tmux_socket: None,
                parent: (i % 2 == 0).then(|| format!("s-{}", i / 2)),
            };
            store.project_discovered_session(&session).unwrap();
            store.set_cursor(&sid, &transcript, 1).unwrap();
        }
        store.commit().unwrap();

        let mut runs_ms = Vec::with_capacity(RUNS);
        for _ in 0..RUNS {
            let started = std::time::Instant::now();
            let known = store.known_sessions().unwrap();
            runs_ms.push(started.elapsed().as_millis());
            assert!(known.get(Path::new("/tmp/budget/s-0.jsonl")).is_some());
        }
        eprintln!("known_sessions over {SESSIONS} sessions: {runs_ms:?} ms, budget {budget_ms} ms");
        drop(store);
        let _ = std::fs::remove_file(path);

        let worst = *runs_ms.iter().max().unwrap();
        assert!(
            worst <= budget_ms,
            "known_sessions took {worst} ms (runs {runs_ms:?}), budget {budget_ms} ms; \
             the native TUI poll loop calls this every pass, check idx_edge_child"
        );
    }

    #[test]
    fn sync_projects_facts_and_cursor_is_incremental() {
        let db_path = temp_path("db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();

        let lines_path = temp_path("log");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.100Z","gitBranch":"main","message":"hello"}}"#).unwrap();
        writeln!(file, r#"{{"type":"assistant","sessionId":"ses-1","timestamp":"2026-08-01T00:00:01.000Z","gitBranch":"main","message":{{"content":[{{"type":"tool_use","name":"Read","input":{{"file_path":"/tmp/a.rs"}}}},{{"type":"tool_use","name":"Bash","input":{{"command":"git diff --stat"}}}}]}}}}"#).unwrap();
        drop(file);

        let session = session_for(&lines_path);
        let first = sync_session_with(&store, &session, None, project_transcript).unwrap();
        assert_eq!(first.written, 3, "user text, tool Read, tool Bash");
        assert_eq!(first.dropped, 0);

        let counts = store.counts().unwrap();
        assert_eq!(counts["agent_turn"], 3);
        assert_eq!(counts["agent_touch"], 1);
        assert_eq!(counts["agent_cmd"], 1);
        assert_eq!(counts["agent_session"], 1);

        // second sync with nothing new carries the cursor and writes nothing
        let noop = sync_session_with(&store, &session, None, project_transcript).unwrap();
        assert_eq!(noop.written, 0);
        let counts2 = store.counts().unwrap();
        assert_eq!(counts2["agent_turn"], 3);

        // query turns back out as TEXT rows
        let filter = super::TurnQuery {
            session: Some("ses-1".to_owned()),
            ..Default::default()
        };
        let rows = store.query_turns(&filter).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["role"], "user");
        assert_eq!(rows[0]["said"], "hello");
        assert_eq!(rows[2]["role"], "tool");

        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&lines_path);
    }

    #[test]
    #[cfg(feature = "agent-read")]
    fn claude_cursor_metadata_is_two_statements_per_line_and_resumes() {
        let db_path = temp_path("cursor-batch-db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let fixture_paths = [
            "../boop-harness/tests/fixtures/claude/bench/bench-claude-0001.jsonl",
            "../boop-harness/tests/fixtures/claude/bench/bench-claude-0002.jsonl",
            "../boop-harness/tests/fixtures/claude/bench/bench-claude-0003.jsonl",
            "../boop-harness/tests/fixtures/claude/bench/bench-claude-0004.jsonl",
        ];
        let total_lines: usize = fixture_paths
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap().lines().count())
            .sum();
        assert_eq!(total_lines, 600);

        CURSOR_SQL.store(0, Ordering::Relaxed);
        store
            .connection()
            .trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(count_cursor_sql));
        let mut first_cursor = None;
        for (index, path) in fixture_paths.iter().enumerate() {
            let mut session = session_for(std::path::Path::new(path));
            session.session_id = format!("cursor-fixture-{index}");
            let ingested = project_transcript(&store, &session, 0).unwrap();
            assert!(ingested.next_cursor > 0);
            assert!(ingested.stat.written > 0);
            if first_cursor.is_none() {
                first_cursor = Some((session, ingested.next_cursor));
            }
        }
        store.connection().trace_v2(TraceEventCodes::empty(), None);
        assert_eq!(CURSOR_SQL.load(Ordering::Relaxed), total_lines * 2);

        let (session, cursor) = first_cursor.unwrap();
        let resumed = project_transcript(&store, &session, cursor).unwrap();
        assert_eq!(resumed.next_cursor, cursor);
        assert_eq!(resumed.stat.written, 0);
        assert_eq!(resumed.stat.dropped, 0);

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn sync_writes_the_spawn_edge() {
        let db_path = temp_path("db3");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();

        let lines_path = temp_path("log3");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        writeln!(
            file,
            r#"{{"type":"user","sessionId":"child","message":"go"}}"#
        )
        .unwrap();
        let mut session = session_for(&lines_path);
        session.session_id = "child".to_owned();
        session.parent = Some("parent".to_owned());
        drop(file);
        sync_session_with(&store, &session, None, project_transcript).unwrap();

        let edges = store.query_edges(Some("child")).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["kind"], "agent_edge");
        assert_eq!(edges[0]["parent"], "parent");
        assert_eq!(edges[0]["child"], "child");
        assert_eq!(edges[0]["edge"], "spawned");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&lines_path);
    }

    /// FAIL-FIRST (D1). A per-pass `turn = 0` renumbered from 1 into stored
    /// ordinals, and `add_turn`'s `INSERT OR IGNORE` dropped the collisions.
    #[test]
    fn ordinals_continue_across_an_incremental_sync() {
        let db_path = temp_path("d1db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines_path = temp_path("d1log");
        let _ = std::fs::remove_file(&lines_path);

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.100Z","message":"one"}}"#).unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.200Z","message":"two"}}"#).unwrap();
        drop(file);

        let session = session_for(&lines_path);
        sync_session_with(&store, &session, None, project_transcript).unwrap();

        let mut file = OpenOptions::new().append(true).open(&lines_path).unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.300Z","message":"three"}}"#).unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.400Z","message":"four"}}"#).unwrap();
        drop(file);

        sync_session_with(&store, &session, None, project_transcript).unwrap();

        let counts = store.counts().unwrap();
        assert_eq!(counts["agent_turn"], 4, "all four turns stored");

        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&lines_path);
    }

    /// FAIL-FIRST (D2). The ordinal ticked per content block but only text and
    /// tool_use wrote a row: 1293 of 1312 live sessions had COUNT < MAX(turn).
    #[test]
    fn a_thinking_block_burns_no_ordinal() {
        let db_path = temp_path("d2db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines_path = temp_path("d2log");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        writeln!(file, r#"{{"type":"assistant","sessionId":"ses-1","timestamp":"2026-08-01T00:00:01.000Z","message":{{"content":[{{"type":"thinking","thinking":"hmm"}},{{"type":"text","text":"said"}},{{"type":"tool_result","content":"r"}},{{"type":"tool_use","name":"Read","input":{{"file_path":"/tmp/a.rs"}}}}]}}}}"#).unwrap();
        drop(file);

        let session = session_for(&lines_path);
        sync_session_with(&store, &session, None, project_transcript).unwrap();

        let filter = super::TurnQuery {
            session: Some("ses-1".to_owned()),
            ..Default::default()
        };
        let rows = store.query_turns(&filter).unwrap();
        let ordinals: Vec<i64> = rows
            .iter()
            .map(|row| row["turn"].as_i64().unwrap())
            .collect();
        assert_eq!(ordinals, vec![1, 2], "ordinals are dense, not 2 and 4");

        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&lines_path);
    }

    /// FAIL-FIRST (D3). `agent_edge.model_id` referenced a `dict_model` table
    /// that no `CREATE TABLE` in SCHEMA ever made.
    #[test]
    fn dict_model_exists() {
        let db_path = temp_path("d3db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let found: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='dict_model'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, 1, "agent_edge.model_id needs a real dict_model");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// FAIL-FIRST (D5). One offset per session id made two transcripts sharing
    /// that id fight over it, re-reading one from byte 0 on every sync.
    #[test]
    fn a_cursor_is_per_transcript_not_per_session() {
        let db_path = temp_path("d5db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        store.set_cursor("ses-1", "/a/one.jsonl", 100).unwrap();
        store.set_cursor("ses-1", "/b/one.jsonl", 250).unwrap();
        assert_eq!(store.get_cursor("ses-1", "/a/one.jsonl").unwrap(), 100);
        assert_eq!(store.get_cursor("ses-1", "/b/one.jsonl").unwrap(), 250);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// FAIL-FIRST (migration). A fresh store already carries the version stamp
    /// that tells a pre-dense store apart from a current one.
    #[test]
    fn a_fresh_store_carries_the_schema_version() {
        let db_path = temp_path("mig");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let version: i64 = store
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, super::SCHEMA_VERSION, "a fresh store is stamped");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    fn usage_record(message_id: &str, request: &str, output: i64, extra: &str) -> String {
        format!(
            r#"{{"type":"assistant","sessionId":"ses-1","timestamp":"2026-08-01T00:00:01.000Z",{request}"message":{{"id":"{message_id}","model":"claude-opus-5","usage":{{"input_tokens":10,"output_tokens":{output},"cache_read_input_tokens":700,"service_tier":"standard","cache_creation":{{"ephemeral_5m_input_tokens":33,"ephemeral_1h_input_tokens":4}}}},"content":[{extra}]}}}}"#
        )
    }

    fn sync_lines(store: &Store, name: &str, lines: &[String]) -> super::SyncStat {
        let lines_path = temp_path(name);
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        drop(file);
        let stat =
            sync_session_with(store, &session_for(&lines_path), None, project_transcript).unwrap();
        let _ = std::fs::remove_file(&lines_path);
        stat
    }

    fn usage_totals(store: &Store) -> (i64, i64, i64, i64) {
        store
            .connection
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(output_tokens),0), COALESCE(SUM(input_tokens),0),
                        COALESCE(SUM(cache_create_5m_tokens + cache_create_1h_tokens + cache_read_tokens),0)
                 FROM agent_usage",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap()
    }

    /// ACCEPTANCE. The corpus writes one response many times while it streams
    /// (2.07x measured); summing raw records doubles every number.
    #[test]
    fn usage_dedups_to_one_row_and_the_last_output_wins() {
        let db_path = temp_path("u1db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines: Vec<String> = [3, 3, 396]
            .iter()
            .map(|out| {
                usage_record(
                    "msg_a",
                    r#""requestId":"req_a","#,
                    *out,
                    r#"{"type":"text","text":"hi"}"#,
                )
            })
            .collect();
        let stat = sync_lines(&store, "u1log", &lines);
        assert_eq!(stat.usage_written, 1, "one insert for one API call");
        assert_eq!(stat.usage_updated, 2, "two snapshots raised the count");
        let (rows, output, input, cached) = usage_totals(&store);
        assert_eq!(rows, 1, "three records, one call");
        assert_eq!(output, 396, "the final count wins, not the first snapshot");
        assert_eq!(input, 10, "input is not summed across snapshots");
        assert_eq!(cached, 33 + 4 + 700);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// A replayed transcript must converge, so a lower output count is refused.
    #[test]
    fn a_lower_output_snapshot_never_wins() {
        let db_path = temp_path("u2db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines = vec![
            usage_record(
                "msg_a",
                r#""requestId":"req_a","#,
                396,
                r#"{"type":"text","text":"hi"}"#,
            ),
            usage_record(
                "msg_a",
                r#""requestId":"req_a","#,
                3,
                r#"{"type":"text","text":"hi"}"#,
            ),
        ];
        sync_lines(&store, "u2log", &lines);
        let (rows, output, _, _) = usage_totals(&store);
        assert_eq!(rows, 1);
        assert_eq!(output, 396, "the 3-token snapshot must not overwrite 396");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// 8.4% of measured records carry no requestId; NULLs in a SQLite UNIQUE
    /// index are distinct, so a nullable column would insert each one twice.
    #[test]
    fn a_missing_request_id_still_dedups() {
        let db_path = temp_path("u3db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines = vec![
            usage_record("msg_b", "", 5, r#"{"type":"text","text":"hi"}"#),
            usage_record("msg_b", "", 50, r#"{"type":"text","text":"hi"}"#),
        ];
        sync_lines(&store, "u3log", &lines);
        let (rows, output, _, _) = usage_totals(&store);
        assert_eq!(rows, 1, "one call, not two");
        assert_eq!(output, 50);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// 31.1% of measured usage records carry only thinking blocks, so the first
    /// stored block's ordinal does not exist for them.
    #[test]
    fn a_thinking_only_response_still_gets_a_usage_row() {
        let db_path = temp_path("u4db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines = vec![usage_record(
            "msg_c",
            r#""requestId":"req_c","#,
            77,
            r#"{"type":"thinking","thinking":"hmm"}"#,
        )];
        sync_lines(&store, "u4log", &lines);
        let (rows, output, _, _) = usage_totals(&store);
        assert_eq!(rows, 1);
        assert_eq!(output, 77);
        let turn: i64 = store
            .connection
            .query_row("SELECT turn FROM agent_usage", [], |row| row.get(0))
            .unwrap();
        assert_eq!(turn, 1, "a minted ordinal, dense with the turn table");
        assert!(store.sparse_sessions().unwrap().is_empty());
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// The time-range aggregate must seek the ts index, never scan the table.
    #[test]
    fn the_usage_time_range_query_seeks_an_index() {
        let db_path = temp_path("u5db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let plan: String = store
            .connection
            .query_row(
                "EXPLAIN QUERY PLAN SELECT SUM(output_tokens) FROM agent_usage
                 WHERE ts >= 1 AND ts < 2",
                [],
                |row| row.get(3),
            )
            .unwrap();
        assert!(
            plan.contains("USING INDEX idx_usage_ts"),
            "plan must seek idx_usage_ts, got: {plan}"
        );
        assert!(!plan.contains("SCAN agent_usage"), "plan scans: {plan}");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// ACCEPTANCE (Job 1). A turn with two PR urls keeps two rows; the key is
    /// (session_id, turn, pr_url_id) and a re-sync dedups on the full key.
    #[test]
    fn two_prs_in_one_turn_survive_and_resync_dedups() {
        let db_path = temp_path("prdb");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();

        store
            .add_pr("s1", 1, "https://github.com/a/b/pull/1")
            .unwrap();
        store
            .add_pr("s1", 1, "https://github.com/a/b/pull/2")
            .unwrap();
        let n: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM agent_pr", [], |row| row.get(0))
            .unwrap();
        assert_eq!(n, 2, "two urls on one turn must both survive");

        // A re-sync repeats the same urls; INSERT OR IGNORE dedups by the full
        // key, so no row count can grow.
        store
            .add_pr("s1", 1, "https://github.com/a/b/pull/1")
            .unwrap();
        store
            .add_pr("s1", 1, "https://github.com/a/b/pull/2")
            .unwrap();
        let n: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM agent_pr", [], |row| row.get(0))
            .unwrap();
        assert_eq!(n, 2, "re-sync must not grow the row count");

        // The query surface exposes both urls for the turn.
        let (_, urls) = store
            .passthrough(
                "SELECT dp.value FROM agent_pr pr
                 JOIN dict_pr dp ON dp.id = pr.pr_url_id
                 WHERE pr.session_id = (SELECT id FROM dict_session WHERE value = 's1')
                 ORDER BY dp.value",
            )
            .unwrap();
        assert_eq!(urls.len(), 2);

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// Favorites are user-authored with no transcript behind them; rebuild
    /// drops every projected row and the favorite comes back intact.
    #[test]
    fn a_favorite_survives_rebuild() {
        let db_path = temp_path("fav");
        let _ = std::fs::remove_file(&db_path);
        {
            let store = Store::open(db_path.clone()).unwrap();
            let id = store
                .favorite_add("# kept\n\n| a | b |\n", "the writers table", "chat", 42)
                .unwrap();
            assert!(id > 0);
            store.rebuild().unwrap();
            let (_, rows) = store
                .passthrough(
                    "SELECT f.note, f.source, m.body FROM agent_favorite f
                     JOIN markdown_cache m ON m.markdown_id = f.markdown_id",
                )
                .unwrap();
            assert_eq!(rows.len(), 1, "the favorite crossed the rebuild");
            let row = rows[0].as_object().unwrap();
            assert_eq!(row.get("note").unwrap(), "the writers table");
            assert_eq!(row.get("body").unwrap(), "# kept\n\n| a | b |\n");
        }
        let _ = std::fs::remove_file(&db_path);
    }

    /// ACCEPTANCE (Job 1 migration). A v7 store (agent_pr keyed on
    /// (session_id, turn)) rebuilds onto the three-column key and keeps rows.
    #[test]
    fn a_v7_store_migrates_agent_pr_onto_the_three_column_key() {
        let db_path = temp_path("prmig");
        let _ = std::fs::remove_file(&db_path);
        {
            // The v7 store: agent_pr keyed on (session_id, turn), sync_cursor
            // without the record fields, stamped 7.
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "PRAGMA user_version = 7;
                 CREATE TABLE dict_session (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
                 CREATE TABLE dict_pr (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
                 CREATE TABLE agent_session (
                   session_id INTEGER PRIMARY KEY,
                   harness_id INTEGER NOT NULL,
                   nickname TEXT,
                   cwd_id INTEGER,
                   branch_id INTEGER,
                   started_ts INTEGER);
                 CREATE TABLE dict_harness (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
                 CREATE TABLE agent_pr (
                   session_id INTEGER NOT NULL,
                   turn INTEGER NOT NULL,
                   pr_url_id INTEGER NOT NULL,
                   PRIMARY KEY (session_id, turn)
                 ) WITHOUT ROWID;
                 CREATE TABLE sync_cursor (
                   session_id INTEGER NOT NULL,
                   path_id INTEGER NOT NULL,
                   offset INTEGER NOT NULL,
                   PRIMARY KEY (session_id, path_id)
                 ) WITHOUT ROWID;
                 CREATE TABLE dict_path (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
                 INSERT INTO dict_session (id, value) VALUES (1, 's1');
                 INSERT INTO dict_pr (id, value) VALUES (1, 'https://github.com/a/b/pull/1');
                 INSERT INTO agent_pr (session_id, turn, pr_url_id) VALUES (1, 1, 1);",
            )
            .unwrap();
        }
        {
            let store = Store::open(db_path.clone()).unwrap();
            assert_eq!(store.schema_version().unwrap(), super::SCHEMA_VERSION);
            // The migrated turn endorses a second urls; it must survive.
            store
                .add_pr("s1", 1, "https://github.com/a/b/pull/2")
                .unwrap();
            let (_, rows) = store
                .passthrough("SELECT pr_url_id FROM agent_pr WHERE session_id = 1 AND turn = 1")
                .unwrap();
            assert_eq!(rows.len(), 2, "post-migration, both urls survive one turn");
        }
        let _ = std::fs::remove_file(&db_path);
    }

    /// ACCEPTANCE (v14 migration). A store whose agent_live predates the door
    /// columns gains them and the delivery ledger, keeping its liveness rows.
    #[test]
    fn a_v13_store_gains_the_door_columns_and_the_delivery_ledger() {
        let db_path = temp_path("doormig");
        let _ = std::fs::remove_file(&db_path);
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "PRAGMA user_version = 13;
                 CREATE TABLE dict_session (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
                 CREATE TABLE agent_live (
                   session_id INTEGER PRIMARY KEY,
                   pid INTEGER,
                   tmux_pane_id INTEGER,
                   status_id INTEGER);
                 INSERT INTO dict_session (id, value) VALUES (1, 's1');
                 INSERT INTO agent_live (session_id, pid) VALUES (1, 4242);",
            )
            .unwrap();
        }
        let store = Store::open(db_path.clone()).unwrap();
        assert_eq!(store.schema_version().unwrap(), super::SCHEMA_VERSION);
        store
            .record_live_door("s1", "unix-socket", Some("/tmp/claude-4242.sock"))
            .unwrap();
        let (_, rows) = store
            .passthrough("SELECT pid, door_kind, door_addr FROM agent_live WHERE session_id = 1")
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("pid").unwrap(), 4242);
        assert_eq!(rows[0].get("door_kind").unwrap(), "unix-socket");
        store
            .record_delivery(
                "m-1",
                "coord",
                Some(crate::harness_id::HarnessId::Claude),
                "queued-for-turn-boundary",
                "",
                90,
            )
            .unwrap();
        let (_, rows) = store
            .passthrough("SELECT COUNT(*) AS n FROM agent_delivery")
            .unwrap();
        assert_eq!(rows[0].get("n").unwrap(), 1);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// RECEIPT. Two deliveries of one message to one route leave one row:
    /// the ledger answers "what happened to this hail", not "how many tries".
    #[test]
    fn a_second_delivery_of_one_message_overwrites_its_outcome() {
        let (path, store) = fresh_store("delivery-ledger");
        store
            .record_delivery("m-9", "lane-a", None, "unreachable", "no live session", 10)
            .unwrap();
        store
            .record_delivery(
                "m-9",
                "lane-a",
                Some(crate::harness_id::HarnessId::Codex),
                "injected",
                "",
                20,
            )
            .unwrap();
        let rows = store.delivery_rows("m-9").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].outcome, "injected");
        assert_eq!(rows[0].harness.as_deref(), Some("codex"));
        assert_eq!(rows[0].detail, "");
        assert_eq!(rows[0].at_ms, 20);
        assert!(store.delivery_rows("m-nothing").unwrap().is_empty());
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// ACCEPTANCE (Job 4). The pid-observing sync stores the lane pane pid on
    /// the agent_live row, so a session can be linked to its process.
    #[test]
    fn an_observed_live_lane_row_carries_its_pid() {
        let db_path = temp_path("pid4db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();

        let lines_path = temp_path("pid4log");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        writeln!(
            file,
            r#"{{"type":"user","sessionId":"ses-1","message":"go"}}"#
        )
        .unwrap();
        drop(file);
        let session = session_for(&lines_path);
        sync_session_with(&store, &session, Some(4242), project_transcript).unwrap();
        let pid: Option<i64> = store
            .connection
            .query_row(
                "SELECT pid FROM agent_live
                 WHERE session_id = (SELECT id FROM dict_session WHERE value = 'ses-1')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pid, Some(4242), "the observed live row must carry its pid");

        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&lines_path);
    }

    #[test]
    fn surrogate_ids_intern_once() {
        let db_path = temp_path("db2");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let a = store.intern("dict_path", "/tmp/a.rs");
        let b = store.intern("dict_path", "/tmp/a.rs");
        assert_eq!(a.unwrap(), b.unwrap());
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// ACCEPTANCE (item 6). Repeated hails across one edge are counted, never
    /// collapsed, while a distinct kind stays its own row.
    #[test]
    fn repeated_hails_accumulate_on_one_edge() {
        let db_path = temp_path("edgedb");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        store.add_edge_at("coord", "worker", "spawned", 90).unwrap();
        store.add_edge_at("coord", "worker", "hail", 100).unwrap();
        store.add_edge_at("coord", "worker", "hail", 110).unwrap();
        store.add_edge_at("coord", "worker", "hail", 130).unwrap();

        let edges = store.query_edges(None).unwrap();
        let hail = edges.iter().find(|edge| edge["edge"] == "hail").unwrap();
        assert_eq!(hail["n"], 3, "three hails counted on one edge");
        assert_eq!(hail["first_ts"], 100, "first sighting sticks");
        assert_eq!(hail["last_ts"], 130, "each sighting bumps the last");
        assert!(
            hail["last_ts"].as_i64().unwrap() > hail["first_ts"].as_i64().unwrap(),
            "repeat spans time"
        );
        let spawned = edges.iter().find(|edge| edge["edge"] == "spawned").unwrap();
        assert_eq!(spawned["n"], 1, "one structural spawn stays one");
        assert_eq!(edges.len(), 2, "two distinct edge kinds, one hail row each");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// ACCEPTANCE (item 2). Observations fold into [from_ts, to_ts) intervals:
    /// a state change closes the open interval, a repeated identical one
    /// extends nothing, and a historical point query uses the open-rule.
    #[test]
    fn liveness_intervals_fold_adjacent_and_ignore_repeats() {
        let db_path = temp_path("livdb");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        store.record_status("s1", 100, "live", None, None).unwrap();
        store.record_status("s1", 110, "live", None, None).unwrap();
        store.record_status("s1", 200, "idle", None, None).unwrap();
        store.record_status("s1", 300, "live", None, None).unwrap();

        let spans = store.live_span(Some("s1")).unwrap();
        assert_eq!(spans.len(), 3, "a repeat never inserts a row");
        assert_eq!(spans[0].from_ts, 100);
        assert_eq!(
            spans[0].to_ts,
            Some(200),
            "state change closes the interval"
        );
        assert_eq!(spans[1].from_ts, 200);
        assert_eq!(spans[1].to_ts, Some(300));
        assert_eq!(spans[2].from_ts, 300);
        assert_eq!(spans[2].to_ts, None, "open interval");

        let at_150 = store.query_live_at(150).unwrap();
        assert_eq!(at_150.len(), 1);
        assert_eq!(at_150[0].status, "live");
        let at_250 = store.query_live_at(250).unwrap();
        assert_eq!(at_250[0].status, "idle");
        let at_350 = store.query_live_at(350).unwrap();
        assert_eq!(at_350[0].status, "live", "open interval covers the present");

        let current: String = store
            .connection
            .query_row(
                "SELECT d.value FROM agent_live a JOIN dict_status d ON d.id = a.status_id
                 WHERE a.session_id = (SELECT id FROM dict_session WHERE value = 's1')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(current, "live", "agent_live stays the current-state cache");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// ACCEPTANCE (item 10). Harnesses emit verbs in different casing; the
    /// shared projection lowers once, so a canonical `verb` collides while the
    /// `raw_verb` spelling survives per adapter.
    #[test]
    fn mixed_case_verbs_land_one_canonical_with_raw_retained() {
        let db_path = temp_path("verbdb");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();

        let lines_path = temp_path("verblog");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        writeln!(file, r#"{{"type":"assistant","sessionId":"ses-1","timestamp":"2026-08-01T00:00:01.000Z","gitBranch":"main","message":{{"content":[{{"type":"tool_use","name":"Read","input":{{"file_path":"/tmp/a.rs"}}}},{{"type":"tool_use","name":"Write","input":{{"file_path":"/tmp/b.rs"}}}}]}}}}"#).unwrap();
        drop(file);
        sync_session_with(&store, &session_for(&lines_path), None, project_transcript).unwrap();

        // A second adapter (opencode/codex path) funnels through the same
        // canonical write site with lowercase verbs.
        store
            .write_tool_fact(
                "oc-1",
                1,
                1000,
                "read",
                Some(&serde_json::json!({"file_path": "/tmp/a.rs"})),
            )
            .unwrap();

        // The canonical verb (verb_id) is lowercase for every spelling.
        let verb_sql = "SELECT json_group_array(value) FROM (
                          SELECT DISTINCT dv.value
                          FROM agent_touch t JOIN dict_verb dv ON dv.id = t.verb_id
                          ORDER BY dv.value)";
        let verbs: String = store
            .connection
            .query_row(verb_sql, [], |row| row.get(0))
            .unwrap();
        assert!(
            verbs.contains("\"read\"") && verbs.contains("\"write\""),
            "{verbs}"
        );
        assert!(
            !verbs.contains("\"Read\""),
            "canonical verb is lowercase: {verbs}"
        );

        // The raw spelling (raw_verb_id) retains the harness casing.
        let raw_sql = "SELECT COUNT(*) FROM agent_touch t
                       JOIN dict_verb dv ON dv.id = t.raw_verb_id
                       WHERE dv.value = 'Read'";
        let raw_read: i64 = store
            .connection
            .query_row(raw_sql, [], |row| row.get(0))
            .unwrap();
        assert_eq!(raw_read, 1, "the claude 'Read' raw spelling survives");
        let raw_lower: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM agent_touch t
                 JOIN dict_verb dv ON dv.id = t.raw_verb_id
                 WHERE dv.value = 'read'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(raw_lower, 1, "the opencode 'read' raw spelling survives");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&lines_path);
    }

    /// The per-transcript cursor surfaces transcript identity for a later
    /// stream: harness, session, path, and the byte offset ingest read to.
    #[test]
    #[cfg(feature = "agent-read")]
    fn query_cursors_expose_transcript_identity() {
        let db_path = temp_path("curdb");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines_path = temp_path("curlog");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.100Z","message":"hello"}}"#).unwrap();
        drop(file);
        sync_session_with(&store, &session_for(&lines_path), None, project_transcript).unwrap();

        let cursors = store.query_cursors(Some("ses-1")).unwrap();
        assert_eq!(cursors.len(), 1);
        assert_eq!(cursors[0].session, "ses-1");
        assert_eq!(cursors[0].harness, "claude");
        assert!(cursors[0].byte_offset > 0, "cursor advanced past the line");
        assert!(!cursors[0].record_id.is_empty());
        assert!(cursors[0].turn > 0);
        assert!(cursors[0].timestamp > 0);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&lines_path);
    }

    /// RECEIPT (review of 8d821db, defect 1). A store written by an older
    /// binary holds `gemini` in `dict_harness`, a value `HarnessId` never
    /// named. Every read that crosses that column stays text, so the session
    /// rows, the cursors, the status rows, the session graph and the
    /// known-session map return instead of erroring, and a registry route
    /// naming it resolves with `harness: None`.
    #[test]
    fn a_foreign_dict_harness_value_reads_back_without_error() {
        use crate::_0_session_graph::{load_agent_session_graph, AgentSessionGraphQuery};

        let (db_path, store) = fresh_store("foreign_harness");
        store
            .upsert_session_row("gem-1", "gemini", "gem-1", Some("/w"), None, 1)
            .unwrap();
        assert_eq!(HarnessId::parse("gemini"), None);

        let rows = store.session_rows(None, None).unwrap();
        assert!(rows
            .iter()
            .any(|row| row.session == "gem-1" && row.harness == "gemini"));
        store.query_cursors(None).unwrap();
        store.status_rows(60_000, 2).unwrap();
        store.known_sessions().unwrap();
        let graph = load_agent_session_graph(
            &store,
            AgentSessionGraphQuery {
                cwd: None,
                include_history: true,
                tmux: None,
                history_since_ts: None,
            },
        )
        .unwrap();
        assert!(graph
            .sessions
            .iter()
            .any(|node| node.session.id == "gem-1" && node.session.harness == "gemini"));

        let mail_dir = temp_path("foreign_harness_mail");
        let _ = std::fs::remove_dir_all(&mail_dir);
        std::fs::create_dir_all(&mail_dir).unwrap();
        std::fs::write(
            mail_dir.join("registry.json"),
            r#"{"gem-coord": {"kind": "coordinator", "harness": "gemini", "sessionId": "gem-1"}}"#,
        )
        .unwrap();
        let routes = crate::bus::read_routes(&mail_dir).unwrap();
        assert_eq!(routes["gem-coord"].harness, None);
        store.resolve_lane_runtime("gem-coord", &routes).unwrap();

        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&mail_dir);
    }
}
