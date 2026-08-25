//! Bus-compatible registry and mailbox store.
//!
//! `bus` keeps the lane registry at `~/.agent/mail/registry.json` and the
//! message log as NDJSON `.ndjson` files beside it. `boop` reads and writes
//! the SAME files in the SAME shape so both tools can run against one registry
//! during the changeover. No new registry format, no migration.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::OptionalExtension;
use serde_json::{Map, Value};

use crate::harness_id::HarnessId;

/// A mailbox envelope as it appears on disk.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub id: String,
    pub from: String,
    pub to: String,
    pub from_timestamp: String,
    pub to_timestamp: Option<String>,
    pub kind: String,
    pub reply_to: Option<String>,
    pub body: String,
    pub r#ref: Option<String>,
    /// A lane's exit code, carried on `kind=result` rows only.
    pub rc: Option<i32>,
    /// Why a lane exited the way it did, when the supervisor knows a reason.
    pub detail: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Route {
    /// `lane` is the default for registry rows written before kinds existed.
    pub kind: String,
    pub harness: Option<HarnessId>,
    pub tmux: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub session_id: Option<String>,
    pub source_path: Option<String>,
    /// The lane that summoned this one; `None` when spawned without `--parent`.
    pub parent: Option<String>,
    /// What the lane is running toward; `None` when spawned without `--goal`.
    pub goal: Option<String>,
    /// When this lane last registered (`lane create`/dispatch), ISO-8601. The
    /// wait since-boundary that skips a previous run's result rows.
    pub registered_at: Option<String>,
    /// The base sha the spawn branched from. The `lane wait`/`lane list`
    /// worktree-escape rail compares the registered worktree HEAD against it.
    pub base_sha: Option<String>,
    /// The worktree the spawn should have worked in; `None` for a main-tree
    /// spawn (no worktree was created).
    pub worktree_dir: Option<String>,
    /// The managed Codex app-server socket shared by a native TUI and Boop.
    pub app_server_socket: Option<String>,
}

/// Read the route map out of the mailbox `dir` addresses.
pub fn read_routes(dir: &Path) -> Result<BTreeMap<String, Route>> {
    routes_in(&open_store(dir)?)
}

fn route_from_value(entry: &Value) -> Route {
    let object = match entry.as_object() {
        Some(object) => object,
        // a bare string is a shorthand for a session id route
        None if entry.is_string() => {
            return Route {
                kind: "lane".into(),
                session_id: entry.as_str().map(str::to_owned),
                ..Route::unset()
            };
        }
        None => return Route::unset(),
    };
    Route {
        kind: string_field(object, "kind").unwrap_or_else(|| "lane".into()),
        harness: string_field(object, "harness")
            .as_deref()
            .and_then(HarnessId::parse),
        tmux: string_field(object, "tmux"),
        cwd: string_field(object, "cwd"),
        model: string_field(object, "model"),
        mode: string_field(object, "mode"),
        session_id: string_field(object, "sessionId")
            .or_else(|| string_field(object, "session_id")),
        source_path: string_field(object, "sourcePath")
            .or_else(|| string_field(object, "source_path")),
        parent: string_field(object, "parent"),
        goal: string_field(object, "goal"),
        registered_at: string_field(object, "registeredAt")
            .or_else(|| string_field(object, "registered_at")),
        base_sha: string_field(object, "baseSha").or_else(|| string_field(object, "base_sha")),
        worktree_dir: string_field(object, "worktreeDir")
            .or_else(|| string_field(object, "worktree_dir")),
        app_server_socket: string_field(object, "appServerSocket"),
    }
}

impl Route {
    fn unset() -> Self {
        Route {
            kind: "lane".into(),
            harness: None,
            tmux: None,
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        }
    }
}
fn string_field(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn int_field(object: &Map<String, Value>, key: &str) -> Option<i32> {
    object
        .get(key)
        .and_then(Value::as_i64)
        .map(|value| value as i32)
}

/// The one mailbox `dir` addresses. A caller that folded across many ndjson
/// files now folds across one database.
pub fn read_boxes(dir: &Path) -> Result<Vec<PathBuf>> {
    open_store(dir)?;
    Ok(vec![db_path(dir)?])
}

/// Read one mailbox back. A `.ndjson` path is still parsed as a file so the
/// importer and old fixtures keep working; anything else is a database.
pub fn parse_box(path: &Path) -> Vec<Message> {
    if path.extension().is_some_and(|ext| ext == "ndjson") {
        return parse_ndjson(path);
    }
    crate::ident::Store::open(path.to_path_buf())
        .and_then(|store| messages_in(&store))
        .unwrap_or_default()
}

pub fn parse_line(line: &str) -> Option<Message> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value: Value = serde_json::from_str(trimmed).ok()?;
    let object = value.as_object()?;
    let id = string_field(object, "id")?;
    let to = string_field(object, "to")?;
    let body = string_field(object, "body").unwrap_or_default();
    let kind = string_field(object, "kind").unwrap_or_else(|| "note".into());
    let (legacy_rc, legacy_detail) = match kind == "result" {
        true => rc_from_body(&body),
        false => (None, None),
    };
    Some(Message {
        id,
        to,
        from: string_field(object, "from").unwrap_or_default(),
        from_timestamp: string_field(object, "from_timestamp")
            .or_else(|| string_field(object, "ts"))
            .unwrap_or_default(),
        to_timestamp: string_field(object, "to_timestamp"),
        reply_to: string_field(object, "reply_to"),
        r#ref: string_field(object, "ref"),
        rc: int_field(object, "rc").or(legacy_rc),
        detail: string_field(object, "detail").or(legacy_detail),
        kind,
        body,
    })
}

/// The one reader of `lane <id> done rc=N (why)` prose, for result rows
/// appended before `rc` and `detail` were columns. No other site parses a body.
fn rc_from_body(body: &str) -> (Option<i32>, Option<String>) {
    let rc = body.split_whitespace().find_map(|token| {
        token
            .strip_prefix("rc=")
            .or_else(|| token.strip_prefix("rc:"))?
            .parse::<i32>()
            .ok()
    });
    let detail = body
        .split_once('(')
        .and_then(|(_, rest)| rest.rsplit_once(')'))
        .map(|(inner, _)| inner.to_owned());
    match rc {
        Some(rc) => (Some(rc), detail),
        None => (None, None),
    }
}

/// Fold rows: the last row per id wins, but an ack survives a later resend of
/// the same envelope (an ack is a fact about the transcript). Output preserves
/// first-seen order, matching the JS `Map` insertion order.
pub fn fold(rows: &[Message]) -> Vec<Message> {
    let mut latest: HashMap<String, Message> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for row in rows {
        if !latest.contains_key(&row.id) {
            order.push(row.id.clone());
        }
        let prior_ack = latest
            .get(&row.id)
            .and_then(|prior| prior.to_timestamp.clone());
        let to_timestamp = prior_ack.or_else(|| row.to_timestamp.clone());
        let mut merged = row.clone();
        merged.to_timestamp = to_timestamp;
        latest.insert(row.id.clone(), merged);
    }
    order
        .into_iter()
        .filter_map(|id| latest.remove(&id))
        .collect()
}

pub fn unacked(rows: &[Message]) -> Vec<Message> {
    fold(rows)
        .into_iter()
        .filter(|row| row.to_timestamp.is_none())
        .collect()
}

pub fn message_line(message: &Message) -> String {
    // Serialize a struct so key order matches `bus` (its MailStore.line emits
    // id, from, to, from_timestamp, to_timestamp, kind, reply_to, body, ref).
    #[derive(serde::Serialize)]
    struct Line<'a> {
        id: &'a str,
        from: &'a str,
        to: &'a str,
        from_timestamp: &'a str,
        to_timestamp: Option<&'a str>,
        kind: &'a str,
        reply_to: Option<&'a str>,
        body: &'a str,
        #[serde(rename = "ref")]
        r#ref: Option<&'a str>,
        // A row that carries no exit code keeps the byte shape `bus` reads.
        #[serde(skip_serializing_if = "Option::is_none")]
        rc: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<&'a str>,
    }
    let line = Line {
        id: &message.id,
        from: &message.from,
        to: &message.to,
        from_timestamp: &message.from_timestamp,
        to_timestamp: message.to_timestamp.as_deref(),
        kind: &message.kind,
        reply_to: message.reply_to.as_deref(),
        body: &message.body,
        r#ref: message.r#ref.as_deref(),
        rc: message.rc,
        detail: message.detail.as_deref(),
    };
    serde_json::to_string(&line).unwrap_or_default()
}

/// The line injected into a pane so cass can prove a read by finding this text
/// in the recipient's transcript.
pub fn injected_line(message: &Message) -> String {
    format!("[bus {}] {}", message.id, message.body)
}

/// Content-hashed compare-and-swap write to the registry, matching
/// `casUpdateJson`: the mutation runs against the exact bytes that were hashed,
/// and a concurrent writer is detected by a hash mismatch.
pub fn cas_update_json(
    path: &Path,
    mutate: impl Fn(&mut Map<String, Value>) -> Result<()>,
) -> Result<()> {
    if path.file_name().is_some_and(|name| name == "registry.json") {
        let dir = path.parent().unwrap_or(Path::new("."));
        return registry_update(dir, mutate);
    }
    let max_attempts = 5;
    for attempt in 0..max_attempts {
        let raw = fs::read(path).ok();
        let mut current: Map<String, Value> = match &raw {
            Some(bytes) => serde_json::from_slice(bytes)
                .with_context(|| format!("registry.json is invalid JSON at {}", path.display()))?,
            None => Map::new(),
        };
        let digest = raw.as_deref().map(sha256_hex);
        mutate(&mut current)?;
        let fresh = fs::read(path).ok();
        if fresh.as_deref().map(sha256_hex) == digest {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).context("create registry parent dir")?;
            }
            let bytes = serde_json::to_vec_pretty(&current).context("serialize registry")?;
            atomic_write(path, &bytes)?;
            return Ok(());
        }
        if attempt + 1 == max_attempts {
            anyhow::bail!("cas_update_json gave up after {max_attempts} attempts");
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut output = Vec::new();
    output.extend_from_slice(bytes);
    output.push(b'\n');
    fs::write(&tmp, &output).context("write registry temp")?;
    fs::rename(&tmp, path).context("rename registry into place")?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    // A fast digest is enough for CAS detection; this is not a security check.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// The configured mail dir: `BOOP_MAIL_DIR`, else the directory `BOOP_DB`
/// names, else `~/.agent/mail`. A caller that redirected the store has
/// redirected the mailbox with it, so no verb reaches into `~/.agent` behind
/// its back.
pub fn default_mail_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("BOOP_MAIL_DIR").filter(|dir| !dir.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    if let Some(db) = std::env::var_os("BOOP_DB").filter(|db| !db.is_empty()) {
        if let Some(parent) = PathBuf::from(db).parent() {
            return Ok(parent.to_path_buf());
        }
    }
    let home = dirs::home_dir().context("resolve home directory")?;
    Ok(home.join(".agent").join("mail"))
}

pub fn now_iso() -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

pub fn mint_id() -> String {
    use std::fmt::Write;
    let mut digest = [0u8; 8];
    getrandom_bytes(&mut digest);
    let mut hex = String::with_capacity(10);
    hex.push_str("m-");
    for byte in &digest[..4] {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn getrandom_bytes(out: &mut [u8]) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = seed ^ (std::process::id() as u64);
    for slot in out.iter_mut() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *slot = (state >> 33) as u8;
    }
}


// ---------------------------------------------------------------------------
// the sqlite mailbox
// ---------------------------------------------------------------------------

/// The first transition every appended envelope carries.
const APPENDED: &str = "appended";

/// The database `dir` addresses: a mail dir holds its own `boop.db`, and the
/// default mail dir maps to the one store every other verb opens.
pub fn db_path(dir: &Path) -> Result<PathBuf> {
    if default_mail_dir().is_ok_and(|home| home == dir) {
        return crate::ident::Store::default_path();
    }
    Ok(dir.join("boop.db"))
}

/// Open the mailbox `dir` addresses, importing any `bus.ndjson` and
/// `registry.json` left beside it exactly once.
pub fn open_store(dir: &Path) -> Result<crate::ident::Store> {
    let path = db_path(dir)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create mailbox parent dir")?;
    }
    let store = crate::ident::Store::open(path)?;
    import_legacy(&store, dir)?;
    Ok(store)
}

/// Tail every legacy file beside the database. The files are never renamed,
/// moved or deleted: a `boop` that predates the mailbox keeps appending to
/// them, and every open reads only what was written since the last one.
fn import_legacy(store: &crate::ident::Store, dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    import_registry_file(store, &dir.join("registry.json"))?;
    for path in read_ndjson_files(dir)? {
        import_ndjson_tail(store, &path)?;
    }
    Ok(())
}

/// The byte offset and content digest this path was last read at.
fn import_mark(store: &crate::ident::Store, path: &Path) -> Result<(i64, String)> {
    let key = path.display().to_string();
    let mark = store
        .connection()
        .query_row(
            "SELECT offset, digest FROM mail_import WHERE path = ?1",
            rusqlite::params![key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(mark.unwrap_or((0, String::new())))
}

fn set_import_mark(
    store: &crate::ident::Store,
    path: &Path,
    offset: i64,
    digest: &str,
) -> Result<()> {
    store.connection().execute(
        "INSERT INTO mail_import (path, offset, digest) VALUES (?1, ?2, ?3)
         ON CONFLICT(path) DO UPDATE SET offset = excluded.offset, digest = excluded.digest",
        rusqlite::params![path.display().to_string(), offset, digest],
    )?;
    Ok(())
}

/// Read one ndjson file from the last imported byte to the last complete line.
/// A row whose id the mailbox already holds is a no-op, so a re-read of the
/// same bytes doubles nothing; the offset makes the usual case read nothing.
fn import_ndjson_tail(store: &crate::ident::Store, path: &Path) -> Result<()> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    let length = metadata.len() as i64;
    let (mark, _) = import_mark(store, path)?;
    // A file shorter than the mark was rotated or truncated under us.
    let offset = match length < mark {
        true => 0,
        false => mark,
    };
    if length == offset {
        return Ok(());
    }
    let bytes = read_from(path, offset as u64)?;
    let Some(last) = bytes.iter().rposition(|byte| *byte == b'\n') else {
        return Ok(());
    };
    let complete = &bytes[..=last];
    let mailbox = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("bus")
        .to_owned();
    let connection = store.connection();
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<()> {
        for line in String::from_utf8_lossy(complete).lines() {
            let Some(message) = parse_line(line) else {
                continue;
            };
            write_message(store, &mailbox, &message, "imported from ndjson")?;
        }
        set_import_mark(store, path, offset + complete.len() as i64, "")
    })();
    finish(connection, result)
}

fn read_from(path: &Path, offset: u64) -> Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    file.seek(SeekFrom::Start(offset))
        .with_context(|| format!("seek {}", path.display()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("read {}", path.display()))?;
    Ok(bytes)
}

/// Merge the registry file into the route table by name. The newer
/// `registeredAt` wins, so a route the mailbox already carries is not rolled
/// back to an older spawn's spelling. Nothing is deleted: a name only the
/// table holds survives a file that never knew it.
fn import_registry_file(store: &crate::ident::Store, path: &Path) -> Result<()> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(());
    };
    if text.trim().is_empty() {
        return Ok(());
    }
    let digest = sha256_hex(text.as_bytes());
    let (_, seen) = import_mark(store, path)?;
    if seen == digest {
        return Ok(());
    }
    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("registry.json is invalid JSON at {}", path.display()))?;
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    let connection = store.connection();
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<()> {
        let held = routes_in(store)?;
        for (id, entry) in object {
            let incoming = route_from_value(entry);
            if outranks(held.get(id), &incoming) {
                continue;
            }
            upsert_route(store, id, &incoming)?;
        }
        set_import_mark(store, path, 0, &digest)
    })();
    finish(connection, result)
}

/// Whether the route the table already holds beats the one the file carries.
/// An unstamped file row never displaces a stamped stored one.
fn outranks(held: Option<&Route>, incoming: &Route) -> bool {
    let Some(held) = held else {
        return false;
    };
    match (held.registered_at.as_deref(), incoming.registered_at.as_deref()) {
        (Some(held_at), Some(new_at)) => held_at >= new_at,
        (Some(_), None) => true,
        _ => false,
    }
}

fn finish(connection: &rusqlite::Connection, result: Result<()>) -> Result<()> {
    match result {
        Ok(()) => {
            connection.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn read_ndjson_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).context("read mail dir")? {
        let entry = entry.context("read mail entry")?;
        if entry.path().extension().is_some_and(|ext| ext == "ndjson") {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn parse_ndjson(path: &Path) -> Vec<Message> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines().filter_map(parse_line).collect()
}

/// Append one envelope and its first transition in one transaction. The
/// `agent_mail_needs_transition` trigger refuses the row if the pair splits.
pub fn insert_message(
    store: &crate::ident::Store,
    mailbox: &str,
    message: &Message,
    detail: &str,
) -> Result<()> {
    let connection = store.connection();
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = write_message(store, mailbox, message, detail);
    finish(connection, result)
}

fn write_message(
    store: &crate::ident::Store,
    mailbox: &str,
    message: &Message,
    detail: &str,
) -> Result<()> {
    let connection = store.connection();
    if !store.has_delivery_transition(&message.id)? {
        store.append_delivery_transition(
            &message.id,
            &message.to,
            None,
            APPENDED,
            detail,
            None,
            now_ms(),
        )?;
    }
    connection.execute(
        "INSERT INTO agent_mail
           (message_id, mailbox, from_route, to_route, from_timestamp, to_timestamp,
            kind, reply_to, body, ref_id, rc, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(message_id) DO UPDATE SET
           to_timestamp = COALESCE(excluded.to_timestamp, agent_mail.to_timestamp),
           kind = excluded.kind,
           body = excluded.body,
           reply_to = excluded.reply_to,
           ref_id = excluded.ref_id,
           rc = excluded.rc,
           detail = excluded.detail",
        rusqlite::params![
            message.id,
            mailbox,
            message.from,
            message.to,
            message.from_timestamp,
            message.to_timestamp,
            message.kind,
            message.reply_to,
            message.body,
            message.r#ref,
            message.rc,
            message.detail,
        ],
    )?;
    Ok(())
}

/// Append one envelope to the mailbox `dir` addresses.
pub fn append(dir: &Path, mailbox: &str, message: &Message) -> Result<()> {
    let store = open_store(dir)?;
    insert_message(&store, mailbox, message, "mailbox")
}

/// Stamp `ids` taken. Returns how many rows were open before the stamp.
pub fn ack_messages(store: &crate::ident::Store, ids: &[String], stamp: &str) -> Result<usize> {
    let connection = store.connection();
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let mut stamped = 0usize;
    let result = (|| -> Result<()> {
        for id in ids {
            stamped += connection.execute(
                "UPDATE agent_mail SET to_timestamp = ?1
                 WHERE message_id = ?2 AND to_timestamp IS NULL",
                rusqlite::params![stamp, id],
            )?;
        }
        Ok(())
    })();
    finish(connection, result)?;
    Ok(stamped)
}

/// Every envelope in append order.
pub fn messages_in(store: &crate::ident::Store) -> Result<Vec<Message>> {
    let mut statement = store.connection().prepare(
        "SELECT message_id, from_route, to_route, from_timestamp, to_timestamp,
                kind, reply_to, body, ref_id, rc, detail
         FROM agent_mail ORDER BY seq",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(Message {
            id: row.get(0)?,
            from: row.get(1)?,
            to: row.get(2)?,
            from_timestamp: row.get(3)?,
            to_timestamp: row.get(4)?,
            kind: row.get(5)?,
            reply_to: row.get(6)?,
            body: row.get(7)?,
            r#ref: row.get(8)?,
            rc: row.get(9)?,
            detail: row.get(10)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Every envelope in the mailbox `dir` addresses.
pub fn read_messages(dir: &Path) -> Result<Vec<Message>> {
    messages_in(&open_store(dir)?)
}

/// Every route the store can address.
pub fn routes_in(store: &crate::ident::Store) -> Result<BTreeMap<String, Route>> {
    let mut statement = store.connection().prepare(
        "SELECT route, kind, harness, tmux, cwd, model, mode, session_id, source_path,
                parent, goal, registered_at, base_sha, worktree_dir, app_server_socket
         FROM agent_route ORDER BY route",
    )?;
    let rows = statement.query_map([], |row| {
        let harness: Option<String> = row.get(2)?;
        Ok((
            row.get::<_, String>(0)?,
            Route {
                kind: row.get(1)?,
                harness: harness.as_deref().and_then(HarnessId::parse),
                tmux: row.get(3)?,
                cwd: row.get(4)?,
                model: row.get(5)?,
                mode: row.get(6)?,
                session_id: row.get(7)?,
                source_path: row.get(8)?,
                parent: row.get(9)?,
                goal: row.get(10)?,
                registered_at: row.get(11)?,
                base_sha: row.get(12)?,
                worktree_dir: row.get(13)?,
                app_server_socket: row.get(14)?,
            },
        ))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (id, route) = row?;
        out.insert(id, route);
    }
    Ok(out)
}

fn upsert_route(store: &crate::ident::Store, id: &str, route: &Route) -> Result<()> {
    store.connection().execute(
        "INSERT OR REPLACE INTO agent_route
           (route, kind, harness, tmux, cwd, model, mode, session_id, source_path,
            parent, goal, registered_at, base_sha, worktree_dir, app_server_socket)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            id,
            route.kind,
            route.harness.map(|id| id.as_str().to_owned()),
            route.tmux,
            route.cwd,
            route.model,
            route.mode,
            route.session_id,
            route.source_path,
            route.parent,
            route.goal,
            route.registered_at,
            route.base_sha,
            route.worktree_dir,
            route.app_server_socket,
        ],
    )?;
    Ok(())
}

/// The registry as the JSON shape every caller's mutation is written against.
pub fn route_to_value(route: &Route) -> Value {
    let mut object = Map::new();
    object.insert("kind".into(), Value::String(route.kind.clone()));
    let pairs: [(&str, Option<String>); 13] = [
        ("harness", route.harness.map(|id| id.as_str().to_owned())),
        ("tmux", route.tmux.clone()),
        ("cwd", route.cwd.clone()),
        ("model", route.model.clone()),
        ("mode", route.mode.clone()),
        ("sessionId", route.session_id.clone()),
        ("sourcePath", route.source_path.clone()),
        ("parent", route.parent.clone()),
        ("goal", route.goal.clone()),
        ("registeredAt", route.registered_at.clone()),
        ("baseSha", route.base_sha.clone()),
        ("worktreeDir", route.worktree_dir.clone()),
        ("appServerSocket", route.app_server_socket.clone()),
    ];
    for (key, value) in pairs {
        if let Some(value) = value {
            object.insert(key.into(), Value::String(value));
        }
    }
    Value::Object(object)
}

/// Run one caller mutation against the route table under `BEGIN IMMEDIATE`.
/// The table is the whole map, so a key the mutation dropped is deleted.
fn registry_update(
    dir: &Path,
    mutate: impl Fn(&mut Map<String, Value>) -> Result<()>,
) -> Result<()> {
    let store = open_store(dir)?;
    let connection = store.connection();
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<()> {
        let mut map = Map::new();
        for (id, route) in routes_in(&store)? {
            map.insert(id, route_to_value(&route));
        }
        mutate(&mut map)?;
        connection.execute("DELETE FROM agent_route", [])?;
        for (id, entry) in &map {
            upsert_route(&store, id, &route_from_value(entry))?;
        }
        Ok(())
    })();
    finish(connection, result)
}

/// One envelope and the last transition its id carries.
pub struct Landed {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub outcome: String,
    pub detail: String,
}

/// The newest `limit` envelopes to or from `route`, each with the transition
/// it last took. The join is inner: a row with no transition cannot exist.
pub fn mail_with_landing(
    store: &crate::ident::Store,
    route: &str,
    limit: usize,
) -> Result<Vec<Landed>> {
    let mut statement = store.connection().prepare(
        "SELECT m.message_id, m.from_route, m.to_route, m.kind, t.outcome, t.detail
         FROM agent_mail m
         JOIN agent_delivery_transition t ON t.message_id = m.message_id
          AND t.sequence = (SELECT MAX(sequence) FROM agent_delivery_transition
                             WHERE message_id = m.message_id)
         WHERE m.from_route = ?1 OR m.to_route = ?1
         ORDER BY m.seq DESC LIMIT ?2",
    )?;
    let rows = statement.query_map(rusqlite::params![route, limit as i64], |row| {
        Ok(Landed {
            id: row.get(0)?,
            from: row.get(1)?,
            to: row.get(2)?,
            kind: row.get(3)?,
            outcome: row.get(4)?,
            detail: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    out.reverse();
    Ok(out)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{fold, injected_line, parse_line, read_routes, unacked};
    use crate::harness_id::HarnessId;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "boop_bus_{}_{}_{}",
            std::process::id(),
            tag,
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn send(id: &str) -> super::Message {
        super::Message {
            id: id.into(),
            from: "sender".into(),
            to: "lane".into(),
            from_timestamp: "2026-01-01T00:00:00.000Z".into(),
            to_timestamp: None,
            kind: "request".into(),
            reply_to: None,
            body: "hello".into(),
            r#ref: None,
            rc: None,
            detail: None,
        }
    }

    /// A row appended before `rc` and `detail` were columns, copied byte for
    /// byte out of `~/.agent/mail/bus.ndjson`.
    #[test]
    fn a_row_written_before_the_typed_columns_still_carries_its_code() {
        let captured = r#"{"id":"m-74400d60","from":"feature-boop-tell-parent-pro4","to":"codex-147","from_timestamp":"2026-08-19T01:33:06.894997Z","to_timestamp":null,"kind":"result","reply_to":null,"body":"lane feature-boop-tell-parent-pro4 done rc=1 (stalled: 300s with no harness activity)","ref":null}"#;
        let row = parse_line(captured).expect("a live bus row still parses");
        assert_eq!(row.rc, Some(1));
        assert_eq!(
            row.detail.as_deref(),
            Some("stalled: 300s with no harness activity")
        );
    }

    /// A non-result body mentioning `rc=` is prose, never an exit code.
    #[test]
    fn only_a_result_row_takes_a_code_from_its_body() {
        let mut note = send("m-abcdef02");
        note.kind = "note".into();
        note.body = "grep for rc=0 in the log".into();
        let parsed = parse_line(&super::message_line(&note)).unwrap();
        assert_eq!(parsed.rc, None);
    }

    /// The typed columns round-trip, and a row with neither keeps the byte
    /// shape `bus` reads.
    #[test]
    fn a_typed_result_row_round_trips_and_a_plain_row_grows_no_keys() {
        let mut result = send("m-abcdef03");
        result.kind = "result".into();
        result.body = "lane mine done rc=7 (killed by SIGTERM)".into();
        result.rc = Some(7);
        result.detail = Some("killed by SIGTERM".into());
        let line = super::message_line(&result);
        assert!(line.contains(r#""rc":7"#), "{line}");
        assert_eq!(parse_line(&line).unwrap(), result);
        let plain = super::message_line(&send("m-abcdef04"));
        assert!(!plain.contains("\"rc\""), "{plain}");
        assert!(!plain.contains("\"detail\""), "{plain}");
    }

    #[test]
    fn the_file_is_a_log_of_send_then_ack_rows() {
        let send_line = super::message_line(&send("m-abcdef01"));
        let ack_line = super::message_line(&{
            let mut message = send("m-abcdef01");
            message.to_timestamp = Some("2026-01-01T00:00:01.000Z".into());
            message
        });
        let ack = parse_line(&ack_line).unwrap();
        assert_eq!(ack.id, "m-abcdef01");
        assert_eq!(
            ack.to_timestamp.as_deref(),
            Some("2026-01-01T00:00:01.000Z")
        );
        assert!(injected_line(&send("m-abcdef01")).contains("m-abcdef01"));
        let _ = send_line;
    }

    #[test]
    fn unacked_drops_rows_with_a_timestamp() {
        let rows = vec![send("a"), {
            let mut m = send("b");
            m.to_timestamp = Some("2026-01-01T00:00:01.000Z".into());
            m
        }];
        let pending = unacked(&rows);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "a");
    }

    #[test]
    fn fold_keeps_the_ack_across_a_resend() {
        let send_row = send("x");
        let mut ack_row = send("x");
        ack_row.to_timestamp = Some("2026-01-01T00:00:01.000Z".into());
        let resend = send("x");
        let folded = fold(&[send_row, ack_row, resend]);
        assert_eq!(folded.len(), 1);
        assert_eq!(
            folded[0].to_timestamp.as_deref(),
            Some("2026-01-01T00:00:01.000Z")
        );
    }

    /// RECEIPT. The store refuses an envelope whose id carries no transition,
    /// so no code path can leave a row nobody owns.
    #[test]
    fn a_mail_row_without_a_transition_is_refused() {
        let dir = temp_dir("orphan");
        let store = super::open_store(&dir).unwrap();
        let refused = store.connection().execute(
            "INSERT INTO agent_mail
               (message_id, mailbox, from_route, to_route, from_timestamp, kind, body)
             VALUES ('m-orphan', 'bus', 'a', 'b', '2026-01-01T00:00:00Z', 'note', 'hi')",
            [],
        );
        let error = refused.expect_err("a row with no transition is a schema violation");
        assert!(
            error.to_string().contains("without a delivery transition"),
            "{error}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. One append writes the envelope and its first transition, and
    /// a re-append of the same id adds no second `appended`.
    #[test]
    fn an_append_carries_its_first_transition() {
        let dir = temp_dir("append");
        super::append(&dir, "bus", &send("m-append01")).unwrap();
        super::append(&dir, "bus", &send("m-append01")).unwrap();
        let store = super::open_store(&dir).unwrap();
        let transitions: i64 = store
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM agent_delivery_transition WHERE message_id = 'm-append01'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(transitions, 1);
        assert_eq!(super::messages_in(&store).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. The files beside the database are read where they lie and
    /// left there, so a `boop` that predates the mailbox keeps appending to
    /// them while a newer one tails.
    #[test]
    fn the_files_beside_the_database_are_read_in_place() {
        let dir = temp_dir("import");
        std::fs::write(
            dir.join("registry.json"),
            r#"{"child": {"harness": "opencode", "parent": "coordinator"}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("bus.ndjson"),
            format!("{}\n", super::message_line(&send("m-import01"))),
        )
        .unwrap();
        let routes = super::read_routes(&dir).unwrap();
        assert_eq!(
            routes.get("child").unwrap().parent.as_deref(),
            Some("coordinator")
        );
        assert_eq!(super::read_messages(&dir).unwrap().len(), 1);
        assert!(dir.join("bus.ndjson").is_file(), "the file stays put");
        assert!(dir.join("registry.json").is_file(), "the file stays put");
        assert!(!dir.join("bus.ndjson.imported").exists());
        assert!(!dir.join("registry.json.imported").exists());
        assert_eq!(super::read_messages(&dir).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. Each open reads only the bytes written since the last one, so
    /// rows an old `boop` appends after the first open still land exactly once.
    #[test]
    fn each_open_imports_only_the_ndjson_tail() {
        use std::io::Write;
        let dir = temp_dir("tail");
        let path = dir.join("bus.ndjson");
        std::fs::write(&path, format!("{}\n", super::message_line(&send("m-tail01")))).unwrap();
        assert_eq!(super::read_messages(&dir).unwrap().len(), 1);

        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        for id in ["m-tail02", "m-tail03"] {
            writeln!(file, "{}", super::message_line(&send(id))).unwrap();
        }
        drop(file);
        let after_append = super::read_messages(&dir).unwrap();
        assert_eq!(after_append.len(), 3, "the tail brought exactly two rows");

        let store = super::open_store(&dir).unwrap();
        assert_eq!(super::messages_in(&store).unwrap().len(), 3, "no fourth read");
        let mark: i64 = store
            .connection()
            .query_row(
                "SELECT offset FROM mail_import WHERE path = ?1",
                rusqlite::params![path.display().to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            mark as u64,
            std::fs::metadata(&path).unwrap().len(),
            "the mark sits at the end of the file"
        );
        let transitions: i64 = store
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM agent_delivery_transition WHERE message_id LIKE 'm-tail%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(transitions, 3, "one transition per row, never a second");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. A half-written last line waits for its newline instead of
    /// being imported as a truncated row.
    #[test]
    fn a_partial_last_line_is_left_for_the_next_open() {
        use std::io::Write;
        let dir = temp_dir("partial");
        let path = dir.join("bus.ndjson");
        let whole = super::message_line(&send("m-part01"));
        let half = super::message_line(&send("m-part02"));
        std::fs::write(&path, format!("{whole}\n{}", &half[..half.len() / 2])).unwrap();
        assert_eq!(super::read_messages(&dir).unwrap().len(), 1);

        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(file, "{}\n", &half[half.len() / 2..]).unwrap();
        drop(file);
        let rows = super::read_messages(&dir).unwrap();
        assert_eq!(rows.len(), 2, "the completed line lands whole");
        assert_eq!(rows[1].id, "m-part02");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. The route the mailbox already carries is not rolled back to an
    /// older spawn's spelling by a stale registry file.
    #[test]
    fn a_newer_registered_route_outranks_the_file() {
        let dir = temp_dir("outrank");
        std::fs::write(
            dir.join("registry.json"),
            r#"{"route-outrank": {"harness": "opencode", "registeredAt": "2026-01-01T00:00:00Z", "goal": "old"}}"#,
        )
        .unwrap();
        assert_eq!(
            super::read_routes(&dir).unwrap()["route-outrank"].goal.as_deref(),
            Some("old")
        );
        let store = super::open_store(&dir).unwrap();
        let fresh = super::Route {
            registered_at: Some("2026-06-01T00:00:00Z".to_owned()),
            goal: Some("new".to_owned()),
            ..super::route_from_value(&serde_json::json!({"harness": "opencode"}))
        };
        super::upsert_route(&store, "route-outrank", &fresh).unwrap();
        std::fs::write(
            dir.join("registry.json"),
            r#"{"route-outrank": {"harness": "opencode", "registeredAt": "2026-01-01T00:00:00Z", "goal": "older still"}}"#,
        )
        .unwrap();
        assert_eq!(
            super::read_routes(&dir).unwrap()["route-outrank"].goal.as_deref(),
            Some("new"),
            "the stale file must not roll the route back"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_route_round_trips_its_parent_field() {
        let dir = temp_dir("parent");
        let path = dir.join("registry.json");
        std::fs::write(
            &path,
            r#"{"child": {"harness": "opencode", "parent": "coordinator"}}"#,
        )
        .unwrap();
        let routes = read_routes(&dir).unwrap();
        let child = routes.get("child").unwrap();
        assert_eq!(child.parent.as_deref(), Some("coordinator"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_registry_without_the_parent_field_still_loads() {
        let dir = temp_dir("noparent");
        let path = dir.join("registry.json");
        std::fs::write(
            &path,
            r#"{"child": {"harness": "opencode", "tmux": "lane-child"}}"#,
        )
        .unwrap();
        let routes = read_routes(&dir).unwrap();
        let child = routes.get("child").unwrap();
        assert_eq!(child.parent, None);
        assert_eq!(child.harness, Some(HarnessId::Opencode));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_route_round_trips_its_goal_field() {
        let dir = temp_dir("goal");
        let path = dir.join("registry.json");
        std::fs::write(
            &path,
            r#"{"child": {"harness": "opencode", "goal": "ship the edge"}}"#,
        )
        .unwrap();
        let routes = read_routes(&dir).unwrap();
        let child = routes.get("child").unwrap();
        assert_eq!(child.goal.as_deref(), Some("ship the edge"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_registry_without_the_goal_field_still_loads() {
        let dir = temp_dir("nogoal");
        let path = dir.join("registry.json");
        std::fs::write(
            &path,
            r#"{"child": {"harness": "opencode", "tmux": "lane-child"}}"#,
        )
        .unwrap();
        let routes = read_routes(&dir).unwrap();
        let child = routes.get("child").unwrap();
        assert_eq!(child.goal, None);
        assert_eq!(child.harness, Some(HarnessId::Opencode));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
