//! The unified reader over AI-harness session ledgers on disk, so a host can
//! browse turns and favorite any message regardless of editor. Two on-disk
//! formats are collapsed to one `Message` shape with a stable identity, and
//! each harness's sessions are shaped into one `SessionMeta` row for a strip.
//! claude writes `~/.claude/projects/<cwd '/'->'-'>/<sessionId>.jsonl`
//! (append-only NDJSON, one record per line; identity = (sessionId, uuid));
//! opencode writes `~/.local/share/opencode/opencode.db` (SQLite; message(id,
//! session_id, time_created, data-json); identity = (session_id, message.id)).
//! Read-only: we never write a harness's own store.
//!
//! This module holds only the harness-neutral surface: the two wire types, the
//! timestamp parser, and the bounded file-read helpers every adapter's
//! `describe`/`messages` share. Each harness's transcript-specific reader
//! lives beside its `impl Harness` block in `harness/{claude,codex,kimi,
//! opencode}.rs`.

use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::harness::HarnessId;

/// The one row the harness strip renders. Discovery is boop-harness's
/// `SessionRef`; every field here is boop-harness's own shaping of it for a
/// UI.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub harness: HarnessId,
    pub cwd: String,
    pub source_path: Option<String>,
    pub title: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub input_tokens: Option<u64>,
    pub parent_id: Option<String>,
    pub parent_kind: Option<&'static str>,
    pub created_at_ms: u64,
    pub last_activity_ms: u64,
}

/// One turn in a session, the wire shape the UI reads back.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "editor")]
    pub harness: HarnessId,
    pub session_id: String,
    pub id: String, // uuid (claude) / message.id (opencode) — stable identity
    pub seq: u64,   // order key: line index (claude) / time_created (opencode)
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    pub ts: u64, // unix ms
    pub preview: String,
    pub text: String,    // full extracted plain text (for cache/copy)
    pub locator: String, // "claude:<path>#L<n>" | "opencode:#msg=<id>"
}

pub fn iso_to_ms(s: &str) -> u64 {
    // Avoid a chrono dependency: ledger timestamps are only used for display and
    // ordering (seq is the real order key), so a lenient parse is fine. Fall back
    // to 0 when absent.
    chrono_lite(s).unwrap_or(0)
}

// Minimal RFC3339 → unix ms without a crate. Returns None on any shape mismatch.
fn chrono_lite(s: &str) -> Option<u64> {
    // 2026-06-26T12:17:10.619Z
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let yr: i64 = s.get(0..4)?.parse().ok()?;
    let mo: i64 = s.get(5..7)?.parse().ok()?;
    let da: i64 = s.get(8..10)?.parse().ok()?;
    let hh: i64 = s.get(11..13)?.parse().ok()?;
    let mi: i64 = s.get(14..16)?.parse().ok()?;
    let ss: i64 = s.get(17..19)?.parse().ok()?;
    let ms: i64 = s.get(20..23).and_then(|m| m.parse().ok()).unwrap_or(0);
    // days since unix epoch via a civil-from-days algorithm (Howard Hinnant).
    let y = if mo <= 2 { yr - 1 } else { yr };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + hh * 3600 + mi * 60 + ss;
    Some((secs * 1000 + ms) as u64)
}

// ---- Shared shaping helpers (called by each adapter's `Harness::describe`).

pub(crate) fn mtime(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn created(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.created().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn json(line: &str) -> Option<Value> {
    serde_json::from_str(line).ok()
}

// Session shaping is called when the harness strip mounts. Rollout files can
// be hundreds of megabytes, while their identity lives at the head and their
// latest usage lives at the tail. Keep the read bounded instead of pulling
// every transcript into memory before the first frame can finish booting.
const SESSION_HEAD_LINES: usize = 128;
const SESSION_TAIL_BYTES: u64 = 256 * 1024;

pub(crate) fn head_values(path: &Path) -> Vec<Value> {
    let Ok(file) = fs::File::open(path) else {
        return vec![];
    };
    BufReader::new(file)
        .lines()
        .take(SESSION_HEAD_LINES)
        .map_while(Result::ok)
        .filter_map(|line| json(&line))
        .collect()
}

pub(crate) fn tail_values(path: &Path) -> Vec<Value> {
    let Ok(mut file) = fs::File::open(path) else {
        return vec![];
    };
    let Ok(len) = file.seek(SeekFrom::End(0)) else {
        return vec![];
    };
    let start = len.saturating_sub(SESSION_TAIL_BYTES);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return vec![];
    }
    let mut text = String::new();
    if file.read_to_string(&mut text).is_err() {
        return vec![];
    }
    let mut lines = text.lines();
    if start > 0 {
        lines.next(); // the bounded read may begin in the middle of a JSON line
    }
    lines.filter_map(json).collect()
}

pub(crate) fn usage(value: &Value, depth: usize) -> Option<&Value> {
    if depth > 6 {
        return None;
    }
    let object = value.as_object()?;
    if object.get("input_tokens").and_then(Value::as_u64).is_some() {
        return Some(value);
    }
    object.values().find_map(|child| usage(child, depth + 1))
}

// Cap a string to `max` chars with an ellipsis. Tool inputs/outputs and thinking
// traces can be huge; the searchable text only needs enough to match the screen.
pub(crate) fn cap(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        s.chars().take(max).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}

pub(crate) fn preview_of(text: &str) -> String {
    let one = text.replace('\n', " ");
    let trimmed = one.trim();
    if trimmed.chars().count() > 200 {
        trimmed.chars().take(200).collect::<String>() + "…"
    } else {
        trimmed.to_string()
    }
}

// Extracted turn text: `full` is everything the harness rendered (prose +
// thinking + tool calls/results) so a search matches whatever's on screen;
// `display` is just the assistant's prose, used for a clean preview/label.
pub(crate) struct Extracted {
    pub(crate) full: String,
    pub(crate) display: String,
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
