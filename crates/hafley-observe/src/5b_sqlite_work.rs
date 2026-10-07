//! Work counts of one connection from `sqlite3_trace_v2` (`STMT | PROFILE | ROW`) and
//! `sqlite3_stmt_status`, no clock; the trace also emits `instrument`'s statement events.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::{c_int, c_uint, c_void, CStr};

use rusqlite::{ffi, Connection};

use crate::sqlite::{emit_begin, emit_finished, statement_sql, StatementCounters};

/// SQLite's default `SQLITE_MAX_SQL_LENGTH`: it refuses longer statement text, so it bounds every
/// cursor over a statement's bytes or tokens.
pub const MAX_STATEMENT_BYTES: usize = 1_000_000_000;

/// Counts since counting started on the connection. `since` gives the counts of an interval.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Work {
    /// CREATE statements executed.
    pub creates: u64,
    /// Per CREATE, the rows of the target schema's `sqlite_master` its reparse scans (after it).
    pub schema_rows_parsed: u64,
    /// Statement objects starting their first run, and their SQL bytes.
    pub prepared: u64,
    pub prepared_bytes: u64,
    /// Prepared SQL with each CTE reference replaced by its body, recursively, `WITH` headers dropped.
    pub unfolded_bytes: u64,
    /// Statement executions, and those that returned no row and wrote no row.
    pub executions: u64,
    pub zero_row_executions: u64,
    /// `sqlite3_stmt_status` sums over every execution.
    pub vm_steps: u64,
    pub fullscan_steps: u64,
    pub sorts: u64,
    pub autoindexes: u64,
    pub reprepares: u64,
    /// Result rows returned to the caller.
    pub rows_returned: u64,
    /// `sqlite3_total_changes` over an execution less its nested statements', by written table
    /// (unqualified name).
    pub written: BTreeMap<String, u64>,
}

impl Work {
    fn counts(&self) -> [u64; 13] {
        [self.creates, self.schema_rows_parsed, self.prepared, self.prepared_bytes, self.unfolded_bytes,
         self.executions, self.zero_row_executions, self.vm_steps, self.fullscan_steps, self.sorts,
         self.autoindexes, self.reprepares, self.rows_returned]
    }

    fn from_counts(f: [u64; 13], written: BTreeMap<String, u64>) -> Work {
        Work {
            creates: f[0], schema_rows_parsed: f[1], prepared: f[2], prepared_bytes: f[3], unfolded_bytes: f[4],
            executions: f[5], zero_row_executions: f[6], vm_steps: f[7], fullscan_steps: f[8], sorts: f[9],
            autoindexes: f[10], reprepares: f[11], rows_returned: f[12], written,
        }
    }

    /// The counts after `earlier` and up to `self`; tables with no write in the interval are absent.
    pub fn since(&self, earlier: &Work) -> Work {
        let (a, b) = (self.counts(), earlier.counts());
        let written = self.written.iter()
            .map(|(table, rows)| (table.clone(), rows - earlier.written.get(table).copied().unwrap_or(0)))
            .filter(|(_, rows)| *rows > 0)
            .collect();
        Work::from_counts(std::array::from_fn(|i| a[i] - b[i]), written)
    }

    pub fn add(&mut self, other: &Work) {
        let (a, b) = (self.counts(), other.counts());
        let mut written = std::mem::take(&mut self.written);
        for (table, rows) in &other.written {
            let slot = written.entry(table.clone()).or_default();
            *slot = slot.saturating_add(*rows);
        }
        *self = Work::from_counts(std::array::from_fn(|i| a[i].saturating_add(b[i])), written);
    }

    /// Rows written to the tables `select` accepts.
    pub fn written_where(&self, mut select: impl FnMut(&str) -> bool) -> u64 {
        self.written.iter().filter(|(table, _)| select(table)).map(|(_, rows)| rows).sum()
    }
}

struct Running {
    rows: u64,
    changes: i64,
    nested_changes: u64,
    nested: bool,
}

#[derive(Default)]
struct State {
    work: Work,
    running: HashMap<usize, Running>,
    /// Rows of `sqlite_master` per schema name.
    schema_rows: HashMap<String, u64>,
    /// Rows the schema reparse returned since the last top-level statement started: the reparse
    /// runs with `db->init.busy`, so it has no SQL text and no STMT or PROFILE event.
    reparsed: u64,
    /// Writes of nested statements finished so far.
    nested_changes: u64,
    db: usize,
    /// A write reset before completion (`INSERT ... RETURNING` read once) applies its changes at
    /// the reset, after PROFILE: (table, nested, total changes at PROFILE).
    unsettled: Option<(String, bool, i64)>,
}

impl State {
    fn written(&mut self, table: &str, rows: u64) {
        match self.work.written.get_mut(table) {
            Some(slot) => *slot += rows,
            None => { self.work.written.insert(table.to_owned(), rows); }
        }
    }

    /// Attributes the changes an unfinished write's reset applied since its PROFILE event.
    fn settle_unfinished(&mut self) {
        let Some((table, nested, at)) = self.unsettled.take() else { return };
        let applied = (unsafe { ffi::sqlite3_total_changes(self.db as *mut ffi::sqlite3) as i64 } - at).max(0) as u64;
        if nested { self.nested_changes += applied; }
        if applied > 0 { self.written(&table, applied); }
    }
}

/// Counting on one connection: registered by `start`, unregistered on drop, which must precede
/// the connection's close. Replaces any trace callback the connection had (`instrument`'s too).
pub struct WorkTrace {
    handle: *mut ffi::sqlite3,
    state: Box<RefCell<State>>,
    /// Counts already returned by `interval`.
    reported: Work,
}

// SAFETY: `handle` and the callback's `state` belong to one `rusqlite::Connection`, which is
// `Send`; the trace moves with its connection and is never shared (`WorkTrace` stays `!Sync`).
unsafe impl Send for WorkTrace {}

impl WorkTrace {
    pub fn start(db: &Connection) -> rusqlite::Result<WorkTrace> {
        let mut state = State::default();
        let schemas: Vec<String> = db.prepare("SELECT name FROM pragma_database_list")?
            .query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;
        for schema in schemas {
            let rows: i64 = db.query_row(&format!("SELECT count(*) FROM \"{}\".sqlite_master", schema.replace('"', "\"\"")), [], |row| row.get(0))?;
            state.schema_rows.insert(schema, rows as u64);
        }
        let handle = unsafe { db.handle() };
        state.db = handle as usize;
        let state = Box::new(RefCell::new(state));
        let mask = (ffi::SQLITE_TRACE_STMT | ffi::SQLITE_TRACE_PROFILE | ffi::SQLITE_TRACE_ROW) as c_uint;
        let rc = unsafe { ffi::sqlite3_trace_v2(handle, mask, Some(trace), (&*state as *const RefCell<State>).cast_mut().cast()) };
        if rc != ffi::SQLITE_OK {
            return Err(rusqlite::Error::SqliteFailure(ffi::Error::new(rc), Some("sqlite3_trace_v2".into())));
        }
        Ok(WorkTrace { handle, state, reported: Work::default() })
    }

    pub fn work(&self) -> Work {
        let mut state = self.state.borrow_mut();
        state.settle_unfinished();
        state.work.clone()
    }

    /// The counts since the previous `interval` (since `start` for the first).
    pub fn interval(&mut self) -> Work {
        let now = self.work();
        let interval = now.since(&self.reported);
        self.reported = now;
        interval
    }
}

impl Drop for WorkTrace {
    fn drop(&mut self) {
        unsafe { ffi::sqlite3_trace_v2(self.handle, 0, None, std::ptr::null_mut()); }
    }
}

unsafe extern "C" fn trace(event: c_uint, context: *mut c_void, p: *mut c_void, x: *mut c_void) -> c_int {
    // A panic must never unwind through SQLite's C callback boundary.
    let _ = std::panic::catch_unwind(|| unsafe { count(event, context, p, x) });
    0
}

unsafe fn count(event: c_uint, context: *mut c_void, p: *mut c_void, x: *mut c_void) {
    let state = unsafe { &*(context as *const RefCell<State>) };
    let Ok(mut state) = state.try_borrow_mut() else { return };
    state.settle_unfinished();
    let stmt = p as *mut ffi::sqlite3_stmt;
    let key = p as usize;
    let db = state.db as *mut ffi::sqlite3;
    if event == ffi::SQLITE_TRACE_STMT as c_uint {
        let text = unsafe { CStr::from_ptr(x as *const std::ffi::c_char) }.to_bytes();
        let nested = text.starts_with(b"-- ");
        let sql = unsafe { statement_sql(stmt) };
        if !nested { state.reparsed = 0; }
        if unsafe { ffi::sqlite3_stmt_status(stmt, ffi::SQLITE_STMTSTATUS_RUN, 0) } == 0 {
            state.work.prepared += 1;
            state.work.prepared_bytes += sql.len() as u64;
            state.work.unfolded_bytes = state.work.unfolded_bytes.saturating_add(unfolded_bytes(sql));
        }
        let changes = unsafe { ffi::sqlite3_total_changes(db) as i64 };
        let nested_changes = state.nested_changes;
        state.running.insert(key, Running { rows: 0, changes, nested_changes, nested });
        unsafe { emit_begin(stmt) };
    } else if event == ffi::SQLITE_TRACE_ROW as c_uint {
        if unsafe { ffi::sqlite3_sql(stmt) }.is_null() {
            state.reparsed += 1;
            return;
        }
        if let Some(running) = state.running.get_mut(&key) { running.rows += 1; }
    } else if event == ffi::SQLITE_TRACE_PROFILE as c_uint {
        let Some(running) = state.running.remove(&key) else { return };
        let sql = unsafe { statement_sql(stmt) };
        let total = (unsafe { ffi::sqlite3_total_changes(db) as i64 } - running.changes).max(0) as u64;
        let own = total.saturating_sub(state.nested_changes - running.nested_changes);
        if running.nested { state.nested_changes += total; }
        let counters = unsafe { StatementCounters::take_with(stmt, false) };
        let work = &mut state.work;
        work.executions += 1;
        work.vm_steps += counters.vm_step as u64;
        work.fullscan_steps += counters.fullscan_step as u64;
        work.sorts += counters.sort as u64;
        work.autoindexes += counters.autoindex as u64;
        work.reprepares += counters.reprepare as u64;
        work.rows_returned += running.rows;
        let (verb, table) = target(sql);
        if verb == Verb::Create {
            work.creates += 1;
            let added = std::mem::take(&mut state.reparsed);
            if added > 0 {
                let rows = state.schema_rows.entry(create_schema(sql)).or_default();
                *rows += added;
                let rows = *rows;
                state.work.schema_rows_parsed += rows;
            }
        } else if matches!(verb, Verb::Write | Verb::Read) && running.rows == 0 && own == 0 {
            work.zero_row_executions += 1;
        }
        if own > 0 { state.written(table, own); }
        if verb == Verb::Write && unsafe { ffi::sqlite3_stmt_busy(stmt) } != 0 {
            let at = unsafe { ffi::sqlite3_total_changes(db) as i64 };
            state.unsettled = Some((table.to_owned(), running.nested, at));
        }
        drop(state);
        unsafe { emit_finished(stmt, *(x as *const u64), &counters) };
    }
}

/// The schema a CREATE writes: its object's qualifier, `temp` for `CREATE TEMP`, else `main`.
fn create_schema(sql: &str) -> String {
    let toks = tokens(sql);
    let mut j = 1;
    let mut schema = "main";
    // budget: MAX_STATEMENT_BYTES; each pass advances the cursor
    while toks.get(j).is_some_and(|t| ["TEMP", "TEMPORARY", "UNIQUE", "VIRTUAL", "TABLE", "INDEX", "VIEW", "TRIGGER", "IF", "NOT", "EXISTS"].iter().any(|w| t.is(sql, w))) {
        if toks[j].is(sql, "TEMP") || toks[j].is(sql, "TEMPORARY") { schema = "temp"; }
        j += 1;
    }
    if toks.get(j + 1).is_some_and(|t| t.tok == Tok::Dot) { toks[j].name(sql).to_owned() } else { schema.to_owned() }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verb { Create, Write, Read, Other }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tok { Word, LParen, RParen, Comma, Dot, Other }

struct Token { tok: Tok, lo: usize, hi: usize }

impl Token {
    /// The identifier, unquoted; `""` for a non-word.
    fn name<'s>(&self, sql: &'s str) -> &'s str {
        let text = &sql[self.lo..self.hi];
        if self.tok != Tok::Word { return ""; }
        text.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(text)
    }

    fn is(&self, sql: &str, word: &str) -> bool {
        self.tok == Tok::Word && sql[self.lo..self.hi].eq_ignore_ascii_case(word)
    }
}

fn tokens(sql: &str) -> Vec<Token> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    // budget: MAX_STATEMENT_BYTES; each pass advances the cursor
    while at < bytes.len() {
        let b = bytes[at];
        let lo = at;
        let tok = match b {
            b' ' | b'\n' | b'\t' | b'\r' => { at += 1; continue; }
            b'(' => { at += 1; Tok::LParen }
            b')' => { at += 1; Tok::RParen }
            b',' => { at += 1; Tok::Comma }
            b'.' => { at += 1; Tok::Dot }
            b'\'' | b'"' | b'`' | b'[' => {
                let close = if b == b'[' { b']' } else { b };
                at += 1;
                // budget: MAX_STATEMENT_BYTES; each pass advances the cursor
                while at < bytes.len() {
                    if bytes[at] == close {
                        if at + 1 < bytes.len() && bytes[at + 1] == close && close != b']' { at += 2; continue; }
                        break;
                    }
                    at += 1;
                }
                at = (at + 1).min(bytes.len());
                if b == b'\'' { Tok::Other } else { Tok::Word }
            }
            b'-' if bytes.get(at + 1) == Some(&b'-') => {
                // budget: MAX_STATEMENT_BYTES; each pass advances the cursor
                while at < bytes.len() && bytes[at] != b'\n' { at += 1; }
                continue;
            }
            _ if b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80 => {
                // budget: MAX_STATEMENT_BYTES; each pass advances the cursor
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_' || bytes[at] == b'$' || bytes[at] >= 0x80) { at += 1; }
                Tok::Word
            }
            _ => { at += 1; Tok::Other }
        };
        out.push(Token { tok, lo, hi: at });
    }
    out
}

/// The statement's verb and the table it writes (`""` for a read), after any `WITH` header.
pub fn target(sql: &str) -> (Verb, &str) {
    let toks = tokens(sql);
    let mut depth = 0i32;
    let mut first = None;
    for (i, t) in toks.iter().enumerate() {
        match t.tok {
            Tok::LParen => depth += 1,
            Tok::RParen => depth -= 1,
            Tok::Word if depth == 0 => {
                let w = &sql[t.lo..t.hi];
                if ["INSERT", "REPLACE", "DELETE", "UPDATE", "SELECT", "VALUES", "CREATE"].iter().any(|k| w.eq_ignore_ascii_case(k)) {
                    first = Some(i);
                    break;
                }
                if i == 0 && !w.eq_ignore_ascii_case("WITH") { return (Verb::Other, ""); }
            }
            _ => {}
        }
    }
    let Some(i) = first else { return (Verb::Other, "") };
    let verb = &sql[toks[i].lo..toks[i].hi];
    let after = |word: &str| toks[i..].iter().position(|t| t.is(sql, word)).map(|p| i + p + 1);
    let table_at = |mut j: usize| {
        let mut name = "";
        // budget: MAX_STATEMENT_BYTES; each pass advances the cursor
        while j < toks.len() && toks[j].tok == Tok::Word {
            name = toks[j].name(sql);
            if toks.get(j + 1).map(|t| t.tok) != Some(Tok::Dot) { break; }
            j += 2;
        }
        name
    };
    if verb.eq_ignore_ascii_case("CREATE") { return (Verb::Create, ""); }
    if verb.eq_ignore_ascii_case("SELECT") || verb.eq_ignore_ascii_case("VALUES") { return (Verb::Read, ""); }
    let at = if verb.eq_ignore_ascii_case("DELETE") { after("FROM") }
        else if verb.eq_ignore_ascii_case("UPDATE") {
            let mut j = i + 1;
            if toks.get(j).is_some_and(|t| t.is(sql, "OR")) { j += 2; }
            Some(j)
        } else { after("INTO") };
    (Verb::Write, at.map_or("", table_at))
}

struct Cte { name_tok: usize, body_lo: usize, body_hi: usize, header_lo: usize, scope_hi: usize }

/// Bytes of `sql` with each CTE reference in a FROM position replaced by the CTE's body,
/// recursively, and every `WITH` header dropped (saturating).
pub fn unfolded_bytes(sql: &str) -> u64 {
    let toks = tokens(sql);
    let mut close = vec![usize::MAX; toks.len()];
    let mut enclosing = vec![usize::MAX; toks.len()];
    let mut open = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        enclosing[i] = open.last().copied().unwrap_or(usize::MAX);
        match t.tok {
            Tok::LParen => open.push(i),
            Tok::RParen => { if let Some(o) = open.pop() { close[o] = i; } }
            _ => {}
        }
    }
    let mut ctes: Vec<Cte> = Vec::new();
    let mut headers: Vec<(usize, usize)> = Vec::new();
    for i in 0..toks.len() {
        if !toks[i].is(sql, "WITH") { continue; }
        let scope_hi = match enclosing[i] { usize::MAX => sql.len(), o if close[o] != usize::MAX => toks[close[o]].lo, _ => sql.len() };
        let mut j = i + 1;
        if toks.get(j).is_some_and(|t| t.is(sql, "RECURSIVE")) { j += 1; }
        let first = ctes.len();
        // budget: MAX_STATEMENT_BYTES; each pass advances the cursor
        loop {
            if !toks.get(j).is_some_and(|t| t.tok == Tok::Word) { break; }
            let name_tok = j;
            j += 1;
            if toks.get(j).is_some_and(|t| t.tok == Tok::LParen) && close[j] != usize::MAX { j = close[j] + 1; }
            if !toks.get(j).is_some_and(|t| t.is(sql, "AS")) { break; }
            j += 1;
            if toks.get(j).is_some_and(|t| t.is(sql, "NOT")) { j += 1; }
            if toks.get(j).is_some_and(|t| t.is(sql, "MATERIALIZED")) { j += 1; }
            if !toks.get(j).is_some_and(|t| t.tok == Tok::LParen) || close[j] == usize::MAX { break; }
            ctes.push(Cte { name_tok, body_lo: toks[j].hi, body_hi: toks[close[j]].lo, header_lo: toks[i].lo, scope_hi });
            j = close[j] + 1;
            if toks.get(j).is_some_and(|t| t.tok == Tok::Comma) { j += 1; } else { break; }
        }
        if ctes.len() > first {
            headers.push((toks[i].lo, ctes.last().unwrap().body_hi + 1));
        }
    }
    if ctes.is_empty() { return sql.len() as u64; }
    let declared: HashSet<usize> = ctes.iter().map(|c| c.name_tok).collect();
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (k, c) in ctes.iter().enumerate() { by_name.entry(toks[c.name_tok].name(sql)).or_default().push(k); }
    let mut refs: Vec<(usize, usize, usize)> = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if t.tok != Tok::Word || declared.contains(&i) { continue; }
        if toks.get(i + 1).is_some_and(|n| n.tok == Tok::Dot) || (i > 0 && toks[i - 1].tok == Tok::Dot) { continue; }
        let Some(candidates) = by_name.get(t.name(sql)) else { continue };
        let resolved = candidates.iter().copied()
            .filter(|&k| ctes[k].body_hi < t.lo && t.lo < ctes[k].scope_hi)
            .max_by_key(|&k| ctes[k].header_lo);
        if let Some(k) = resolved { refs.push((t.lo, t.hi, k)); }
    }
    let context = |pos: usize| ctes.iter().enumerate()
        .filter(|(_, c)| c.body_lo <= pos && pos < c.body_hi)
        .max_by_key(|(_, c)| c.body_lo)
        .map(|(k, _)| k);
    let mut mult = vec![0u128; ctes.len()];
    let mut order: Vec<usize> = (0..ctes.len()).collect();
    order.sort_by_key(|&k| std::cmp::Reverse(ctes[k].body_hi));
    let ref_context: Vec<Option<usize>> = refs.iter().map(|(lo, _, _)| context(*lo)).collect();
    for k in order {
        mult[k] = refs.iter().zip(&ref_context)
            .filter(|((_, _, to), _)| *to == k)
            .fold(0u128, |sum, (_, ctx)| sum.saturating_add(ctx.map_or(1, |c| mult[c])));
    }
    let mut intervals: Vec<(usize, usize, u128)> = headers.iter().map(|&(lo, hi)| (lo, hi, 0)).collect();
    intervals.extend(ctes.iter().enumerate().map(|(k, c)| (c.body_lo, c.body_hi, mult[k])));
    intervals.extend(refs.iter().map(|&(lo, hi, _)| (lo, hi, 0)));
    intervals.sort_by_key(|&(lo, hi, _)| (lo, std::cmp::Reverse(hi)));
    let mut total = 0u128;
    let mut cursor = 0usize;
    let mut stack: Vec<(usize, u128)> = Vec::new();
    let emit = |upto: usize, weight: u128, cursor: &mut usize, total: &mut u128| {
        if upto > *cursor { *total = total.saturating_add(weight.saturating_mul((upto - *cursor) as u128)); *cursor = upto; }
    };
    for (lo, hi, weight) in intervals {
        // budget: MAX_STATEMENT_BYTES; each pass advances the cursor
        while let Some(&(top_hi, top_w)) = stack.last() {
            if top_hi > lo { break; }
            emit(top_hi, top_w, &mut cursor, &mut total);
            stack.pop();
        }
        let current = stack.last().map_or(1, |&(_, w)| w);
        emit(lo, current, &mut cursor, &mut total);
        stack.push((hi, weight));
    }
    // budget: MAX_STATEMENT_BYTES; each pass advances the cursor
    while let Some((top_hi, top_w)) = stack.pop() { emit(top_hi, top_w, &mut cursor, &mut total); }
    emit(sql.len(), 1, &mut cursor, &mut total);
    total.min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use oh::test;

    #[test]
    fn unfolds_cte_references() {
        assert_eq!(unfolded_bytes("SELECT 1"), 8);
        let sql = "WITH a AS (12345), b AS (SELECT * FROM a, a) SELECT * FROM b JOIN b";
        let b_body = "SELECT * FROM , ".len() as u64 + 2 * 5;
        let main = " SELECT * FROM  JOIN ".len() as u64 + 2 * b_body;
        assert_eq!(unfolded_bytes(sql), main);
        assert_eq!(unfolded_bytes("WITH x AS MATERIALIZED (1) SELECT x.c FROM y"), " SELECT x.c FROM y".len() as u64);
    }

    #[test]
    fn classifies_targets() {
        assert_eq!(target("WITH d AS (SELECT 1) INSERT INTO frontier_p_n12_d SELECT * FROM d"), (Verb::Write, "frontier_p_n12_d"));
        assert_eq!(target("DELETE FROM \"s3\".\"n4_i\" WHERE w = 0"), (Verb::Write, "n4_i"));
        assert_eq!(target("UPDATE OR IGNORE t SET a = 1"), (Verb::Write, "t"));
        assert_eq!(target("CREATE TABLE x(a)"), (Verb::Create, ""));
        assert_eq!(target("SAVEPOINT s"), (Verb::Other, ""));
        assert_eq!(create_schema("CREATE TABLE IF NOT EXISTS \"s1\".t(a)"), "s1");
        assert_eq!(create_schema("CREATE TEMP VIEW v AS SELECT 1"), "temp");
    }
}
