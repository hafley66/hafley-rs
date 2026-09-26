//! Any `record`-tagged row straight into its table's columnar buffer: no JSON
//! round-trip, one prepared chunk statement per table, text in one arena.
use std::collections::HashMap;
use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicPtr, AtomicU64};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::Instant;

use rusqlite::Connection;
use rusqlite::vtab::{Context, Filters, IndexInfo, Module, VTab, VTabConnection, VTabCursor};
use serde::ser::{self, Impossible, Serialize};
use bumpalo::Bump;
use rustc_hash::FxHasher;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Rows per chunk statement, before the variable cap divides it down.
const CHUNK_ROWS: usize = 64;

/// Column kinds per table; a `json` column stores its value JSON-encoded,
/// the same text the typed writers bind.
const FACTS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/schema/generated/5_facts.json"));
const COLUMN_KINDS: [&str; 7] = ["string", "uint32", "int64", "boolean", "int32", "json", "uint64"];

#[derive(Clone, Copy, Default)]
struct KindTime {
    calls: u64,
    nulls: u64,
    nanos: u64,
}

#[derive(serde::Deserialize)]
struct TableSpec {
    table: String,
    columns: Vec<ColumnSpec>,
}

#[derive(serde::Deserialize)]
struct ColumnSpec {
    name: String,
    kind: String,
}

#[derive(Clone, Copy)]
enum Val {
    Null,
    Int(i64),
    Real(f64),
    Text(*const u8, u32),
}

/// One table's statements, shared with the writer thread.
struct Meta {
    name: String,
    width: usize,
    row_column: Option<usize>,
    path_column: Option<usize>,
    content_column: Option<usize>,
    record_column: Option<usize>,
    chunk_rows: usize,
    chunk_sql: String,
    one_sql: String,
    select_sql: String,
    active: Arc<AtomicPtr<Batch>>,
}

#[repr(C)]
struct BatchTable {
    base: rusqlite::vtab::sqlite3_vtab,
    width: usize,
    active: Arc<AtomicPtr<Batch>>,
}

// Module's static only stores callbacks; table instances are owned by SQLite.
unsafe impl Sync for BatchTable {}

unsafe impl<'vtab> VTab<'vtab> for BatchTable {
    type Aux = (usize, Arc<AtomicPtr<Batch>>);
    type Cursor = BatchCursor;

    fn connect(_: &mut VTabConnection, aux: Option<&Self::Aux>, _: &[u8], _: &[u8], _: &[u8], _: &[&[u8]])
        -> rusqlite::Result<(Cow<'static, CStr>, Self)> {
        let (width, active) = aux.ok_or_else(|| rusqlite::Error::ModuleError("missing batch table".into()))?;
        let columns = (0..*width).map(|index| format!("c{index}")).collect::<Vec<_>>().join(",");
        let ddl = CString::new(format!("CREATE TABLE x({columns})")).unwrap();
        Ok((Cow::Owned(ddl), Self { base: Default::default(), width: *width, active: Arc::clone(active) }))
    }

    fn best_index(&self, info: &mut IndexInfo) -> rusqlite::Result<bool> {
        info.set_estimated_cost(1.0);
        Ok(true)
    }

    fn open(&mut self) -> rusqlite::Result<Self::Cursor> {
        Ok(BatchCursor { base: Default::default(), active: Arc::clone(&self.active), row: 0, width: self.width })
    }
}

#[repr(C)]
struct BatchCursor {
    base: rusqlite::vtab::sqlite3_vtab_cursor,
    active: Arc<AtomicPtr<Batch>>,
    row: usize,
    width: usize,
}

unsafe impl VTabCursor for BatchCursor {
    fn filter(&mut self, _: i32, _: Option<&str>, _: &Filters<'_>) -> rusqlite::Result<()> {
        self.row = 0;
        Ok(())
    }
    fn next(&mut self) -> rusqlite::Result<()> { self.row += 1; Ok(()) }
    fn eof(&self) -> bool {
        let batch = self.active.load(Ordering::Relaxed);
        batch.is_null() || self.row >= unsafe { (*batch).rows }
    }
    fn column(&self, ctx: &mut Context, i: i32) -> rusqlite::Result<()> {
        let batch = self.active.load(Ordering::Relaxed);
        let batch = unsafe { &*batch };
        match batch.vals[self.row * self.width + i as usize] {
            Val::Null => ctx.set_result(&rusqlite::types::Null),
            Val::Int(value) => ctx.set_result(&value),
            Val::Real(value) => ctx.set_result(&value),
            Val::Text(_, _) => {
                // Every span came from a str or serde_json's UTF-8 writer.
                let value = unsafe { std::str::from_utf8_unchecked(batch.bytes(batch.vals[self.row * self.width + i as usize])) };
                ctx.set_result(&value)
            }
        }
    }
    fn rowid(&self) -> rusqlite::Result<i64> { Ok(self.row as i64 + 1) }
}

static BATCH_MODULE: Module<'static, BatchTable> = Module::eponymous_only_module();

/// One table's rows, `width` values per row, plus the text they point into.
/// Handed whole to the writer thread and handed back cleared.
struct Batch {
    table: usize,
    vals: Vec<Val>,
    text: Bump,
    interned: HashMap<u64, Vec<Val>>,
    rows: usize,
}

impl Batch {
    fn empty(table: usize) -> Self {
        Self { table, vals: Vec::new(), text: Bump::new(), interned: HashMap::new(), rows: 0 }
    }

    #[inline(always)]
    fn text(&mut self, value: &str) -> Val {
        let span = self.text.alloc_slice_copy(value.as_bytes());
        Val::Text(span.as_ptr(), span.len() as u32)
    }

    fn json<T: ?Sized + Serialize>(&mut self, value: &T) -> std::result::Result<Val, serde_json::Error> {
        let mut bytes = bumpalo::collections::Vec::new_in(&self.text);
        serde_json::to_writer(&mut bytes, value)?;
        let span = bytes.into_bump_slice();
        Ok(Val::Text(span.as_ptr(), span.len() as u32))
    }

    #[inline(always)]
    fn bytes(&self, val: Val) -> &[u8] {
        let Val::Text(ptr, len) = val else { unreachable!("text span required") };
        // Every text pointer is allocated from `self.text` and remains valid
        // until clear resets the arena, after the SQLite read has completed.
        unsafe { std::slice::from_raw_parts(ptr, len as usize) }
    }

    #[inline(always)]
    fn intern(&mut self, value: &str) -> Val {
        if value.len() > 128 { return self.text(value); }
        let mut hasher = FxHasher::default();
        value.hash(&mut hasher);
        let hash = hasher.finish();
        if let Some(spans) = self.interned.get(&hash) {
            if let Some(&span) = spans.iter().find(|&&span| self.bytes(span) == value.as_bytes()) {
                return span;
            }
        }
        let val = self.text(value);
        self.interned.entry(hash).or_default().push(val);
        val
    }

    fn clear(&mut self) {
        self.vals.clear();
        self.text.reset();
        self.interned.clear();
        self.rows = 0;
    }

    #[inline(always)]
    fn bind(&self, width: usize, statement: &mut rusqlite::Statement<'_>, row: usize, first: usize) -> rusqlite::Result<()> {
        for (offset, val) in self.vals[row * width..(row + 1) * width].iter().enumerate() {
            let parameter = first + offset;
            match *val {
                Val::Null => statement.raw_bind_parameter(parameter, rusqlite::types::Null)?,
                Val::Int(v) => statement.raw_bind_parameter(parameter, v)?,
                Val::Real(v) => statement.raw_bind_parameter(parameter, v)?,
                Val::Text(_, _) => {
                    let value = unsafe { std::str::from_utf8_unchecked(self.bytes(*val)) };
                    statement.raw_bind_parameter(parameter, value)?;
                }
            }
        }
        Ok(())
    }

    /// Full chunks through the chunk statement, the tail one row at a time.
    fn drain(&mut self, meta: &Meta, connection: &Connection) -> rusqlite::Result<()> {
        let span = tracing::info_span!("sqlite_table_batch_drain", table = %meta.name, rows = self.rows);
        span.in_scope(|| self.drain_inner(meta, connection))
    }

    fn drain_inner(&mut self, meta: &Meta, connection: &Connection) -> rusqlite::Result<()> {
        if std::env::var_os("RYI_SQLITE_VALUES").is_none() {
            // This connection is exclusive to the writer thread. SQLite reads
            // the batch only during execute; clear the pointer before the
            // buffer can be recycled or mutated again.
            meta.active.store(self as *mut Batch, Ordering::Relaxed);
            let result = connection.execute(&meta.select_sql, []);
            meta.active.store(std::ptr::null_mut(), Ordering::Relaxed);
            result?;
            self.clear();
            return Ok(());
        }
        let mut row = 0;
        if self.rows >= meta.chunk_rows {
            let mut statement = connection.prepare_cached(&meta.chunk_sql)?;
            while self.rows - row >= meta.chunk_rows {
                for index in 0..meta.chunk_rows {
                    self.bind(meta.width, &mut statement, row + index, 1 + index * meta.width)?;
                }
                statement.raw_execute()?;
                row += meta.chunk_rows;
            }
        }
        if row < self.rows {
            let mut statement = connection.prepare_cached(&meta.one_sql)?;
            while row < self.rows {
                self.bind(meta.width, &mut statement, row, 1)?;
                statement.raw_execute()?;
                row += 1;
            }
        }
        self.clear();
        Ok(())
    }
}

// The batch and its bump are transferred together to one writer thread. The
// text pointers remain owned by that moved bump and are read before reset.
unsafe impl Send for Batch {}

/// The writer thread: owns the connection while batches stream in, returns
/// it (and the first error, if any) when the channel closes.
pub struct Worker {
    batches: SyncSender<Batch>,
    recycled: Receiver<Batch>,
    handle: JoinHandle<(Connection, Option<String>)>,
}

/// Where the connection lives right now.
pub enum Slot {
    Local(Connection),
    Worker(Worker),
    Moving,
}

impl Slot {
    /// The connection on this thread; joins the writer thread first.
    pub fn local(&mut self) -> Result<&Connection> {
        if let Slot::Worker(_) = self {
            let Slot::Worker(worker) = std::mem::replace(self, Slot::Moving) else { unreachable!() };
            drop(worker.batches);
            let (connection, error) = worker.handle.join().map_err(|_| "SQLite writer thread panicked")?;
            *self = Slot::Local(connection);
            if let Some(error) = error {
                return Err(error.into());
            }
        }
        match self {
            Slot::Local(connection) => Ok(connection),
            _ => Err("SQLite connection is gone".into()),
        }
    }

    /// The connection when it is on this thread; `None` while the writer
    /// thread holds it.
    pub fn get(&self) -> Option<&Connection> {
        match self {
            Slot::Local(connection) => Some(connection),
            _ => None,
        }
    }

    pub fn into_local(mut self) -> Result<Connection> {
        self.local()?;
        match self {
            Slot::Local(connection) => Ok(connection),
            _ => Err("SQLite connection is gone".into()),
        }
    }

    fn spawn(
        &mut self,
        meta: Arc<Vec<Meta>>,
        table_insert_nanos: Arc<Vec<AtomicU64>>,
        profile_enabled: bool,
    ) -> Result<()> {
        let Slot::Local(_) = self else { return Ok(()) };
        let Slot::Local(connection) = std::mem::replace(self, Slot::Moving) else { unreachable!() };
        let (batches, inbox) = sync_channel::<Batch>(QUEUE_BATCHES);
        let (give_back, recycled) = sync_channel::<Batch>(QUEUE_BATCHES + 1);
        let handle = std::thread::Builder::new().name("sqlite-writer".into()).spawn(move || {
            let mut error = None;
            for mut batch in inbox {
                if error.is_none() {
                    let started = profile_enabled.then(Instant::now);
                    if let Err(e) = batch.drain(&meta[batch.table], &connection) {
                        error = Some(e.to_string());
                    }
                    if let Some(started) = started {
                        let nanos = started.elapsed().as_nanos() as u64;
                        table_insert_nanos[batch.table].fetch_add(nanos, Ordering::Relaxed);
                    }
                }
                batch.clear();
                let _ = give_back.try_send(batch);
            }
            (connection, error)
        })?;
        *self = Slot::Worker(Worker { batches, recycled, handle });
        Ok(())
    }
}

/// Batches in flight to the writer thread: the memory bound on the queue.
const QUEUE_BATCHES: usize = 16;

/// Chunk statements per batch handed to the writer thread.
const CHUNKS_PER_BATCH: usize = 16;

pub struct Binder {
    meta: Arc<Vec<Meta>>,
    buffers: Vec<Batch>,
    columns: Vec<HashMap<String, usize>>,
    json: Vec<Vec<bool>>,
    column_kinds: Vec<Vec<usize>>,
    by_name: HashMap<String, usize>,
    path: String,
    threaded: bool,
    profile_enabled: bool,
    table_bind_nanos: Vec<u64>,
    table_rows: Vec<u64>,
    submit_nanos: u64,
    table_insert_nanos: Arc<Vec<AtomicU64>>,
    kind_time: [KindTime; COLUMN_KINDS.len()],
}

impl Binder {
    /// Every base table's columns, read off the live schema. `threaded` hands
    /// full batches to a writer thread instead of executing them inline.
    pub fn new(connection: &Connection, threaded: bool) -> Result<Self> {
        let max_variables = connection.limit(rusqlite::limits::Limit::SQLITE_LIMIT_VARIABLE_NUMBER)? as usize;
        let names: Vec<String> = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<_>>()?;
        let specs: Vec<TableSpec> = serde_json::from_str(FACTS)?;
        let mut types = HashMap::new();
        for spec in &specs {
            for column in &spec.columns {
                let kind = COLUMN_KINDS.iter().position(|name| *name == column.kind)
                    .ok_or_else(|| format!("unknown SQLite column kind `{}`", column.kind))?;
                types.insert((spec.table.clone(), column.name.clone()), kind);
            }
        }
        let mut meta = Vec::with_capacity(names.len());
        let mut columns = Vec::with_capacity(names.len());
        let mut json = Vec::with_capacity(names.len());
        let mut column_kinds = Vec::with_capacity(names.len());
        let mut by_name = HashMap::new();
        for (index, name) in names.into_iter().enumerate() {
            let column_names: Vec<String> = connection
                .prepare(&format!("PRAGMA table_info(\"{name}\")"))?
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<_>>()?;
            let width = column_names.len();
            let chunk_rows = CHUNK_ROWS.min(max_variables / width.max(1)).max(1);
            let list = column_names.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
            let tuple = format!("({})", vec!["?"; width].join(", "));
            let prefix = format!("INSERT INTO \"{name}\" ({list}) VALUES ");
            let active = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
            let module_name = format!("ryi_batch_{index}");
            connection.create_module(module_name.as_str(), &BATCH_MODULE, Some((width, Arc::clone(&active))))?;
            let select_cols = (0..width).map(|index| format!("c{index}")).collect::<Vec<_>>().join(", ");
            meta.push(Meta {
                name: name.clone(),
                width,
                row_column: column_names.iter().position(|c| c == "_row"),
                path_column: column_names.iter().position(|c| c == "_input_path"),
                content_column: column_names.iter().position(|c| c == "_content_id"),
                record_column: column_names.iter().position(|c| c == "record"),
                chunk_rows,
                chunk_sql: format!("{prefix}{}", vec![tuple.as_str(); chunk_rows].join(", ")),
                one_sql: format!("{prefix}{tuple}"),
                select_sql: format!("INSERT INTO \"{name}\" ({list}) SELECT {select_cols} FROM {module_name}"),
                active,
            });
            let kinds = column_names.iter().map(|column| types.get(&(name.clone(), column.clone())).copied()
                .ok_or_else(|| format!("missing SQLite column kind for {name}.{column}")))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            json.push(kinds.iter().map(|kind| *kind == 5).collect());
            column_kinds.push(kinds);
            columns.push(column_names.into_iter().enumerate().map(|(i, name)| (name, i)).collect());
            by_name.insert(name, index);
        }
        let buffers = (0..meta.len()).map(Batch::empty).collect();
        let table_count = meta.len();
        let profile_enabled = tracing::enabled!(tracing::Level::DEBUG);
        Ok(Self { meta: Arc::new(meta), buffers, columns, json, column_kinds, by_name, path: String::new(), threaded,
            profile_enabled,
            table_bind_nanos: vec![0; table_count],
            table_rows: vec![0; table_count],
            submit_nanos: 0,
            table_insert_nanos: Arc::new((0..table_count).map(|_| AtomicU64::new(0)).collect()),
            kind_time: [KindTime::default(); COLUMN_KINDS.len()] })
    }

    pub fn profiling_enabled(&self) -> bool {
        self.profile_enabled
    }

    /// Serialize one row into its table's buffer; a full buffer goes to the
    /// writer thread (or runs inline when unthreaded).
    pub fn push(
        &mut self,
        slot: &mut Slot,
        row: i64,
        input_path: Option<&str>,
        content_id: Option<&str>,
        value: &impl Serialize,
    ) -> Result<()> {
        let started = self.profile_enabled.then(Instant::now);
        self.path.clear();
        let mut writer = RowWriter {
            binder: self,
            table: None,
            prefix_len: 0,
            meta: (row, input_path, content_id),
        };
        let written = value.serialize(&mut writer);
        let opened = writer.table;
        if let Err(error) = written {
            if let Some(table) = opened {
                let buffer = &mut self.buffers[table];
                buffer.vals.truncate(buffer.rows * self.meta[table].width);
                buffer.interned.clear();
            }
            return Err(error.0);
        }
        let Some(table) = opened else {
            return Err("row has no `record` tag".into());
        };
        self.buffers[table].rows += 1;
        if self.profile_enabled {
            self.table_rows[table] += 1;
        }
        if self.buffers[table].rows == self.meta[table].chunk_rows * CHUNKS_PER_BATCH {
            self.submit(slot, table)?;
        }
        if let Some(started) = started {
            self.table_bind_nanos[table] += started.elapsed().as_nanos() as u64;
        }
        Ok(())
    }

    fn submit(&mut self, slot: &mut Slot, table: usize) -> Result<()> {
        let started = self.profile_enabled.then(Instant::now);
        if !self.threaded {
            let connection = slot.local()?;
            let insert_started = self.profile_enabled.then(Instant::now);
            self.buffers[table].drain(&self.meta[table], connection)?;
            if let Some(insert_started) = insert_started {
                let nanos = insert_started.elapsed().as_nanos() as u64;
                self.table_insert_nanos[table].fetch_add(nanos, Ordering::Relaxed);
            }
            if let Some(started) = started {
                self.submit_nanos += started.elapsed().as_nanos() as u64;
            }
            return Ok(());
        }
        slot.spawn(Arc::clone(&self.meta), Arc::clone(&self.table_insert_nanos), self.profile_enabled)?;
        let Slot::Worker(worker) = slot else {
            return Err("SQLite writer thread did not start".into());
        };
        let fresh = match worker.recycled.try_recv() {
            Ok(mut batch) => {
                batch.table = table;
                batch
            }
            Err(_) => Batch::empty(table),
        };
        let full = std::mem::replace(&mut self.buffers[table], fresh);
        if worker.batches.send(full).is_err() {
            slot.local()?;
            return Err("SQLite writer thread stopped".into());
        }
        if let Some(started) = started {
            self.submit_nanos += started.elapsed().as_nanos() as u64;
        }
        Ok(())
    }

    /// Every buffered row to the connection; the connection is back on this
    /// thread afterwards.
    pub fn flush(&mut self, slot: &mut Slot) -> Result<()> {
        for table in 0..self.buffers.len() {
            if self.buffers[table].rows > 0 {
                self.submit(slot, table)?;
            }
        }
        slot.local()?;
        Ok(())
    }

    pub fn record_profile(&self, bind_span: &tracing::Span, bind_seconds: f64) {
        if !self.profile_enabled {
            return;
        }
        bind_span.record("seconds", bind_seconds);
        let column_seconds = self.kind_time.iter().map(|time| time.nanos).sum::<u64>();
        let dispatch_meta_lookup = self.table_bind_nanos.iter().sum::<u64>()
            .saturating_sub(column_seconds)
            .saturating_sub(self.submit_nanos);
        bind_span.record("dispatch_meta_lookup_seconds", dispatch_meta_lookup as f64 / 1e9);
        for (kind, time) in COLUMN_KINDS.iter().zip(self.kind_time) {
            bind_span.in_scope(|| tracing::info!(
                kind = %kind,
                calls = time.calls,
                nulls = time.nulls,
                seconds = time.nanos as f64 / 1e9,
                "sqlite bind column kind"
            ));
        }
        for (index, meta) in self.meta.iter().enumerate() {
            let rows = self.table_rows[index];
            if rows > 0 {
                let bind_seconds = self.table_bind_nanos[index] as f64 / 1e9;
                let insert_seconds = self.table_insert_nanos[index].load(Ordering::Relaxed) as f64 / 1e9;
                bind_span.in_scope(|| tracing::info!(
                    table = %meta.name,
                    rows,
                    bind_seconds,
                    insert_seconds,
                    "sqlite table profile"
                ));
            }
        }
    }
}

#[derive(Debug)]
pub struct Error(Box<dyn std::error::Error>);

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error(msg.to_string().into())
    }
}

fn err<T>(msg: String) -> std::result::Result<T, Error> {
    Err(Error(msg.into()))
}

/// The struct/map level: every key extends the column path by `__key`.
struct RowWriter<'b> {
    binder: &'b mut Binder,
    table: Option<usize>,
    prefix_len: usize,
    meta: (i64, Option<&'b str>, Option<&'b str>),
}

impl RowWriter<'_> {
    #[inline(always)]
    fn open(&mut self, index: usize) -> std::result::Result<(), Error> {
        self.table = Some(index);
        let (row, input_path, content_id) = self.meta;
        let width = self.binder.meta[index].width;
        let meta = &self.binder.meta[index];
        let table = &mut self.binder.buffers[index];
        let base = table.vals.len();
        table.vals.resize(base + width, Val::Null);
        let set = |table: &mut Batch, column: Option<usize>, val: Val| {
            if let Some(index) = column {
                table.vals[base + index] = val;
            }
        };
        set(table, meta.row_column, Val::Int(row));
        if meta.path_column.is_some() {
            let v = input_path.map_or(Val::Null, |p| table.intern(p));
            set(table, meta.path_column, v);
        }
        if meta.content_column.is_some() {
            let v = content_id.map_or(Val::Null, |c| table.intern(c));
            set(table, meta.content_column, v);
        }
        if meta.record_column.is_some() {
            let v = table.intern(&meta.name);
            set(table, meta.record_column, v);
        }
        Ok(())
    }

    #[inline(always)]
    fn field<T: ?Sized + Serialize>(&mut self, key: &str, value: &T) -> std::result::Result<(), Error> {
        if self.prefix_len == 0 && key == "record" {
            let mut probe = Scalar { arena: None, val: None, text: None,
                lookup: Some(&self.binder.by_name), table: None };
            value.serialize(&mut probe)?;
            let Some(index) = probe.table else {
                return err(match probe.text {
                    Some(record) => format!("no table for record `{record}`"),
                    None => "`record` is not text".into(),
                });
            };
            return self.open(index);
        }
        let Some(index) = self.table else {
            return err(format!("field `{key}` before `record`"));
        };
        if self.prefix_len == 0 {
            if let Some(&column) = self.binder.columns[index].get(key) {
                return self.column(index, column, value);
            }
        }
        let restore = self.binder.path.len();
        if self.prefix_len > 0 {
            self.binder.path.push_str("__");
        }
        self.binder.path.push_str(key);
        let result = match self.binder.columns[index].get(self.binder.path.as_str()).copied() {
            Some(column) => self.column(index, column, value),
            None => {
                let saved = self.prefix_len;
                self.prefix_len = self.binder.path.len();
                let nested = value.serialize(&mut *self);
                self.prefix_len = saved;
                nested
            }
        };
        self.binder.path.truncate(restore);
        result
    }

    #[inline(always)]
    fn column<T: ?Sized + Serialize>(&mut self, index: usize, column: usize, value: &T) -> std::result::Result<(), Error> {
        let started = self.binder.profile_enabled.then(Instant::now);
        let kind = self.binder.column_kinds[index][column];
        let width = self.binder.meta[index].width;
        let table = &mut self.binder.buffers[index];
        let val = if self.binder.json[index][column] {
            table.json(value).map_err(|e| Error(e.into()))?
        } else {
            let mut probe = Scalar { arena: Some(&mut *table), val: None, text: None,
                lookup: None, table: None };
            match value.serialize(&mut probe) {
                Ok(()) => probe.val.unwrap_or(Val::Null),
                Err(Compound) => {
                    table.interned.clear();
                    table.json(value).map_err(|e| Error(e.into()))?
                }
            }
        };
        let base = table.rows * width;
        table.vals[base + column] = val;
        if let Some(started) = started {
            let time = &mut self.binder.kind_time[kind];
            time.calls += 1;
            time.nulls += u64::from(matches!(val, Val::Null));
            time.nanos += started.elapsed().as_nanos() as u64;
        }
        Ok(())
    }
}

macro_rules! unsupported {
    ($($f:ident($($t:ty),*) -> $r:ty;)*) => {
        $(fn $f(self, $(_: $t),*) -> std::result::Result<$r, Error> {
            err(format!("`{}` has no column", self.binder.path))
        })*
    };
}

impl<'a, 'b> ser::Serializer for &'a mut RowWriter<'b> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Impossible<(), Error>;
    type SerializeTuple = Impossible<(), Error>;
    type SerializeTupleStruct = Impossible<(), Error>;
    type SerializeTupleVariant = Impossible<(), Error>;
    type SerializeMap = MapWriter<'a, 'b>;
    type SerializeStruct = Self;
    type SerializeStructVariant = Impossible<(), Error>;

    fn serialize_none(self) -> std::result::Result<(), Error> {
        Ok(())
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> std::result::Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> std::result::Result<(), Error> {
        Ok(())
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _: &'static str, value: &T) -> std::result::Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> std::result::Result<Self, Error> {
        Ok(self)
    }
    fn serialize_map(self, _: Option<usize>) -> std::result::Result<MapWriter<'a, 'b>, Error> {
        Ok(MapWriter { row: self, key: String::new() })
    }
    unsupported! {
        serialize_bool(bool) -> ();
        serialize_i8(i8) -> (); serialize_i16(i16) -> (); serialize_i32(i32) -> (); serialize_i64(i64) -> ();
        serialize_u8(u8) -> (); serialize_u16(u16) -> (); serialize_u32(u32) -> (); serialize_u64(u64) -> ();
        serialize_f32(f32) -> (); serialize_f64(f64) -> (); serialize_char(char) -> ();
        serialize_str(&str) -> (); serialize_bytes(&[u8]) -> ();
        serialize_unit_struct(&'static str) -> ();
        serialize_unit_variant(&'static str, u32, &'static str) -> ();
        serialize_seq(Option<usize>) -> Impossible<(), Error>;
        serialize_tuple(usize) -> Impossible<(), Error>;
        serialize_tuple_struct(&'static str, usize) -> Impossible<(), Error>;
        serialize_tuple_variant(&'static str, u32, &'static str, usize) -> Impossible<(), Error>;
        serialize_struct_variant(&'static str, u32, &'static str, usize) -> Impossible<(), Error>;
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(self, _: &'static str, _: u32, _: &'static str, _: &T) -> std::result::Result<(), Error> {
        err(format!("`{}` has no column", self.binder.path))
    }
}

impl ser::SerializeStruct for &mut RowWriter<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> std::result::Result<(), Error> {
        self.field(key, value)
    }
    fn end(self) -> std::result::Result<(), Error> {
        Ok(())
    }
}

pub struct MapWriter<'a, 'b> {
    row: &'a mut RowWriter<'b>,
    key: String,
}

impl ser::SerializeMap for MapWriter<'_, '_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> std::result::Result<(), Error> {
        let mut probe = Scalar { arena: None, val: None, text: None,
            lookup: None, table: None };
        key.serialize(&mut probe)?;
        self.key = probe.text.ok_or_else(|| Error("map key is not text".into()))?;
        Ok(())
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> std::result::Result<(), Error> {
        let key = std::mem::take(&mut self.key);
        self.row.field(&key, value)
    }
    fn end(self) -> std::result::Result<(), Error> {
        Ok(())
    }
}

/// One column's value. Text goes straight into `arena` when there is one; a
/// compound value reports `Compound` and lands as JSON.
struct Scalar<'t> {
    arena: Option<&'t mut Batch>,
    val: Option<Val>,
    text: Option<String>,
    lookup: Option<&'t HashMap<String, usize>>,
    table: Option<usize>,
}

impl Scalar<'_> {
    #[inline(always)]
    fn put(&mut self, value: &str) {
        if let Some(lookup) = self.lookup {
            self.table = lookup.get(value).copied();
            if self.table.is_none() { self.text = Some(value.to_owned()); }
            return;
        }
        match self.arena.as_deref_mut() {
            Some(arena) => self.val = Some(arena.intern(value)),
            None => self.text = Some(value.to_owned()),
        }
    }
}

#[derive(Debug)]
struct Compound;

impl Display for Compound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("compound")
    }
}

impl std::error::Error for Compound {}

impl ser::Error for Compound {
    fn custom<T: Display>(_: T) -> Self {
        Compound
    }
}

impl From<Compound> for Error {
    fn from(_: Compound) -> Self {
        Error("compound value where text was expected".into())
    }
}

impl ser::Serializer for &mut Scalar<'_> {
    type Ok = ();
    type Error = Compound;
    type SerializeSeq = Impossible<(), Compound>;
    type SerializeTuple = Impossible<(), Compound>;
    type SerializeTupleStruct = Impossible<(), Compound>;
    type SerializeTupleVariant = Impossible<(), Compound>;
    type SerializeMap = Impossible<(), Compound>;
    type SerializeStruct = Impossible<(), Compound>;
    type SerializeStructVariant = Impossible<(), Compound>;

    fn serialize_bool(self, v: bool) -> std::result::Result<(), Compound> {
        self.val = Some(Val::Int(v as i64));
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> std::result::Result<(), Compound> {
        self.serialize_i64(v as i64)
    }
    fn serialize_i16(self, v: i16) -> std::result::Result<(), Compound> {
        self.serialize_i64(v as i64)
    }
    fn serialize_i32(self, v: i32) -> std::result::Result<(), Compound> {
        self.serialize_i64(v as i64)
    }
    fn serialize_i64(self, v: i64) -> std::result::Result<(), Compound> {
        self.val = Some(Val::Int(v));
        Ok(())
    }
    fn serialize_u8(self, v: u8) -> std::result::Result<(), Compound> {
        self.serialize_i64(v as i64)
    }
    fn serialize_u16(self, v: u16) -> std::result::Result<(), Compound> {
        self.serialize_i64(v as i64)
    }
    fn serialize_u32(self, v: u32) -> std::result::Result<(), Compound> {
        self.serialize_i64(v as i64)
    }
    fn serialize_u64(self, v: u64) -> std::result::Result<(), Compound> {
        match i64::try_from(v) {
            Ok(v) => self.serialize_i64(v),
            Err(_) => {
                self.put(&v.to_string());
                Ok(())
            }
        }
    }
    fn serialize_f32(self, v: f32) -> std::result::Result<(), Compound> {
        self.serialize_f64(v as f64)
    }
    fn serialize_f64(self, v: f64) -> std::result::Result<(), Compound> {
        self.val = Some(Val::Real(v));
        Ok(())
    }
    fn serialize_char(self, v: char) -> std::result::Result<(), Compound> {
        self.put(v.encode_utf8(&mut [0; 4]));
        Ok(())
    }
    fn serialize_str(self, v: &str) -> std::result::Result<(), Compound> {
        self.put(v);
        Ok(())
    }
    fn serialize_bytes(self, _: &[u8]) -> std::result::Result<(), Compound> {
        Err(Compound)
    }
    fn serialize_none(self) -> std::result::Result<(), Compound> {
        self.val = Some(Val::Null);
        Ok(())
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> std::result::Result<(), Compound> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> std::result::Result<(), Compound> {
        self.val = Some(Val::Null);
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> std::result::Result<(), Compound> {
        self.serialize_unit()
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str) -> std::result::Result<(), Compound> {
        self.serialize_str(variant)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _: &'static str, value: &T) -> std::result::Result<(), Compound> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(self, _: &'static str, _: u32, _: &'static str, _: &T) -> std::result::Result<(), Compound> {
        Err(Compound)
    }
    fn serialize_seq(self, _: Option<usize>) -> std::result::Result<Self::SerializeSeq, Compound> {
        Err(Compound)
    }
    fn serialize_tuple(self, _: usize) -> std::result::Result<Self::SerializeTuple, Compound> {
        Err(Compound)
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> std::result::Result<Self::SerializeTupleStruct, Compound> {
        Err(Compound)
    }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> std::result::Result<Self::SerializeTupleVariant, Compound> {
        Err(Compound)
    }
    fn serialize_map(self, _: Option<usize>) -> std::result::Result<Self::SerializeMap, Compound> {
        Err(Compound)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> std::result::Result<Self::SerializeStruct, Compound> {
        Err(Compound)
    }
    fn serialize_struct_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> std::result::Result<Self::SerializeStructVariant, Compound> {
        Err(Compound)
    }
}

#[cfg(test)]
mod tests {
    use super::{Batch, Val};
    use std::hash::{Hash, Hasher};
    use rustc_hash::FxHasher;

    #[test]
    fn repeated_text_reuses_one_batch_span_and_recycled_capacity() {
        let mut batch = Batch::empty(0);
        let first = batch.intern("shared-name");
        let second = batch.intern("shared-name");
        assert!(matches!((first, second), (Val::Text(a, n), Val::Text(b, m)) if a == b && n == m));
        assert_eq!(batch.bytes(first), b"shared-name");
        batch.intern("other-name");
        let allocated = batch.text.allocated_bytes_including_metadata();
        batch.clear();
        assert_eq!(batch.text.allocated_bytes_including_metadata(), allocated);
        assert!(batch.interned.is_empty());
        let recycled = batch.intern("shared-name");
        assert_eq!(batch.bytes(recycled), b"shared-name");
    }

    #[test]
    fn json_columns_append_utf8_directly_to_the_batch_arena() {
        let mut batch = Batch::empty(0);
        batch.text("prefix");
        let value = serde_json::json!({"text": "λ\n\"", "array": [null, true, 7]});
        let encoded = batch.json(&value).unwrap();
        assert_eq!(batch.bytes(encoded), serde_json::to_string(&value).unwrap().as_bytes());
    }

    #[test]
    fn hash_collision_keeps_distinct_text_spans() {
        let mut batch = Batch::empty(0);
        let mut hasher = FxHasher::default();
        "other".hash(&mut hasher);
        let seed = batch.text("seed");
        batch.interned.insert(hasher.finish(), vec![seed]);
        let other = batch.intern("other");
        assert_ne!(batch.bytes(other), batch.bytes(seed));
        assert_eq!(batch.bytes(other), b"other");
        let repeated = batch.intern("other");
        assert_eq!(batch.bytes(repeated), b"other");
        assert_eq!(batch.interned.values().map(Vec::len).sum::<usize>(), 2);
        let long = "x".repeat(129);
        let before = batch.interned.values().map(Vec::len).sum::<usize>();
        let bypassed = batch.intern(&long);
        assert_eq!(batch.bytes(bypassed), long.as_bytes());
        assert_eq!(batch.interned.values().map(Vec::len).sum::<usize>(), before);
    }
}
