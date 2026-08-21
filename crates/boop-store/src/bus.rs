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
use serde_json::{Map, Value};

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
    pub harness: Option<String>,
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

/// Read the route map out of the `--mail-dir` registry. Corrupt JSON is an
/// error naming the path; it is never silently reset.
pub fn read_routes(dir: &Path) -> Result<BTreeMap<String, Route>> {
    let path = dir.join("registry.json");
    let text = fs::read_to_string(&path).unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("registry.json is invalid JSON at {}", path.display()))?;
    let Some(object) = value.as_object() else {
        return Ok(BTreeMap::new());
    };
    let mut routes = BTreeMap::new();
    for (id, entry) in object {
        routes.insert(id.clone(), route_from_value(entry));
    }
    Ok(routes)
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
        harness: string_field(object, "harness"),
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

/// Read every `.ndjson` mailbox file, newest file first is not needed; callers
/// fold across all of them.
pub fn read_boxes(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if !dir.is_dir() {
        return Ok(paths);
    }
    for entry in fs::read_dir(dir).context("read mail dir")? {
        let entry = entry.context("read mail entry")?;
        if entry.path().extension().is_some_and(|ext| ext == "ndjson") {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

/// Parse the NDJSON lines in one mailbox file, skipping malformed lines.
pub fn parse_box(path: &Path) -> Vec<Message> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines().filter_map(parse_line).collect()
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

/// Default mail dir: `~/.agent/mail`.
pub fn default_mail_dir() -> Result<PathBuf> {
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

#[cfg(test)]
mod tests {
    use super::{fold, injected_line, parse_line, read_routes, unacked};

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
        assert_eq!(child.harness.as_deref(), Some("opencode"));
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
        assert_eq!(child.harness.as_deref(), Some("opencode"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
