//! Any `record`-tagged row straight into its table's columnar buffer: no JSON
//! round-trip, one prepared chunk statement per table, text in one arena.
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;

use rusqlite::Connection;
use serde::ser::{self, Impossible, Serialize};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Rows per chunk statement, before the variable cap divides it down.
const CHUNK_ROWS: usize = 64;

/// Column kinds per table; a `json` column stores its value JSON-encoded,
/// the same text the typed writers bind.
const FACTS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/schema/generated/5_facts.json"));

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
    Text(u32, u32),
}

/// One table's statements, shared with the writer thread.
struct Meta {
    width: usize,
    chunk_rows: usize,
    chunk_sql: String,
    one_sql: String,
}

/// One table's rows, `width` values per row, plus the text they point into.
/// Handed whole to the writer thread and handed back cleared.
struct Batch {
    table: usize,
    vals: Vec<Val>,
    text: String,
    rows: usize,
}

impl Batch {
    fn empty(table: usize) -> Self {
        Self { table, vals: Vec::new(), text: String::new(), rows: 0 }
    }

    fn text(&mut self, value: &str) -> Val {
        let start = self.text.len() as u32;
        self.text.push_str(value);
        Val::Text(start, value.len() as u32)
    }

    fn bind(&self, width: usize, statement: &mut rusqlite::Statement<'_>, row: usize, first: usize) -> rusqlite::Result<()> {
        for (offset, val) in self.vals[row * width..(row + 1) * width].iter().enumerate() {
            let parameter = first + offset;
            match *val {
                Val::Null => statement.raw_bind_parameter(parameter, rusqlite::types::Null)?,
                Val::Int(v) => statement.raw_bind_parameter(parameter, v)?,
                Val::Real(v) => statement.raw_bind_parameter(parameter, v)?,
                Val::Text(start, len) => statement
                    .raw_bind_parameter(parameter, &self.text[start as usize..(start + len) as usize])?,
            }
        }
        Ok(())
    }

    /// Full chunks through the chunk statement, the tail one row at a time.
    fn drain(&mut self, meta: &Meta, connection: &Connection) -> rusqlite::Result<()> {
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
        self.vals.clear();
        self.text.clear();
        self.rows = 0;
        Ok(())
    }
}

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

    fn spawn(&mut self, meta: Arc<Vec<Meta>>) -> Result<()> {
        let Slot::Local(_) = self else { return Ok(()) };
        let Slot::Local(connection) = std::mem::replace(self, Slot::Moving) else { unreachable!() };
        let (batches, inbox) = sync_channel::<Batch>(QUEUE_BATCHES);
        let (give_back, recycled) = sync_channel::<Batch>(QUEUE_BATCHES + 1);
        let handle = std::thread::Builder::new().name("sqlite-writer".into()).spawn(move || {
            let mut error = None;
            for mut batch in inbox {
                if error.is_none() {
                    if let Err(e) = batch.drain(&meta[batch.table], &connection) {
                        error = Some(e.to_string());
                    }
                }
                batch.vals.clear();
                batch.text.clear();
                batch.rows = 0;
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
    by_name: HashMap<String, usize>,
    path: String,
    threaded: bool,
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
        let json_columns: std::collections::HashSet<(String, String)> = specs
            .into_iter()
            .flat_map(|spec| {
                let table = spec.table;
                spec.columns
                    .into_iter()
                    .filter(|column| column.kind == "json")
                    .map(move |column| (table.clone(), column.name))
            })
            .collect();
        let mut meta = Vec::with_capacity(names.len());
        let mut columns = Vec::with_capacity(names.len());
        let mut json = Vec::with_capacity(names.len());
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
            meta.push(Meta {
                width,
                chunk_rows,
                chunk_sql: format!("{prefix}{}", vec![tuple.as_str(); chunk_rows].join(", ")),
                one_sql: format!("{prefix}{tuple}"),
            });
            json.push(column_names.iter().map(|c| json_columns.contains(&(name.clone(), c.clone()))).collect());
            columns.push(column_names.into_iter().enumerate().map(|(i, c)| (c, i)).collect());
            by_name.insert(name, index);
        }
        let buffers = (0..meta.len()).map(Batch::empty).collect();
        Ok(Self { meta: Arc::new(meta), buffers, columns, json, by_name, path: String::new(), threaded })
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
        self.path.clear();
        let mut writer = RowWriter {
            binder: self,
            table: None,
            text_mark: 0,
            prefix_len: 0,
            meta: (row, input_path, content_id),
        };
        let written = value.serialize(&mut writer);
        let (opened, text_mark) = (writer.table, writer.text_mark);
        if let Err(error) = written {
            if let Some(table) = opened {
                let buffer = &mut self.buffers[table];
                buffer.vals.truncate(buffer.rows * self.meta[table].width);
                buffer.text.truncate(text_mark);
            }
            return Err(error.0);
        }
        let Some(table) = opened else {
            return Err("row has no `record` tag".into());
        };
        self.buffers[table].rows += 1;
        if self.buffers[table].rows == self.meta[table].chunk_rows * CHUNKS_PER_BATCH {
            self.submit(slot, table)?;
        }
        Ok(())
    }

    fn submit(&mut self, slot: &mut Slot, table: usize) -> Result<()> {
        if !self.threaded {
            let connection = slot.local()?;
            self.buffers[table].drain(&self.meta[table], connection)?;
            return Ok(());
        }
        slot.spawn(Arc::clone(&self.meta))?;
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
    /// The table's text length before this row: a rejected row rolls back to it.
    text_mark: usize,
    prefix_len: usize,
    meta: (i64, Option<&'b str>, Option<&'b str>),
}

impl RowWriter<'_> {
    fn open(&mut self, record: &str) -> std::result::Result<(), Error> {
        let Some(&index) = self.binder.by_name.get(record) else {
            return err(format!("no table for record `{record}`"));
        };
        self.table = Some(index);
        let (row, input_path, content_id) = self.meta;
        let width = self.binder.meta[index].width;
        let columns = &self.binder.columns[index];
        let table = &mut self.binder.buffers[index];
        self.text_mark = table.text.len();
        let base = table.vals.len();
        table.vals.resize(base + width, Val::Null);
        let set = |table: &mut Batch, column: &str, val: Val| {
            if let Some(&i) = columns.get(column) {
                table.vals[base + i] = val;
            }
        };
        set(table, "_row", Val::Int(row));
        let v = input_path.map_or(Val::Null, |p| table.text(p));
        set(table, "_input_path", v);
        let v = content_id.map_or(Val::Null, |c| table.text(c));
        set(table, "_content_id", v);
        let v = table.text(record);
        set(table, "record", v);
        Ok(())
    }

    fn field<T: ?Sized + Serialize>(&mut self, key: &str, value: &T) -> std::result::Result<(), Error> {
        if self.prefix_len == 0 && key == "record" {
            let mut probe = Scalar { arena: None, val: None, text: None };
            value.serialize(&mut probe)?;
            let Some(record) = probe.text else {
                return err("`record` is not text".into());
            };
            return self.open(&record);
        }
        let Some(index) = self.table else {
            return err(format!("field `{key}` before `record`"));
        };
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

    fn column<T: ?Sized + Serialize>(&mut self, index: usize, column: usize, value: &T) -> std::result::Result<(), Error> {
        let width = self.binder.meta[index].width;
        let table = &mut self.binder.buffers[index];
        if self.binder.json[index][column] {
            let json = serde_json::to_string(value).map_err(|e| Error(e.into()))?;
            let val = table.text(&json);
            table.vals[table.rows * width + column] = val;
            return Ok(());
        }
        let mark = table.text.len();
        let mut probe = Scalar { arena: Some(&mut table.text), val: None, text: None };
        let val = match value.serialize(&mut probe) {
            Ok(()) => probe.val.unwrap_or(Val::Null),
            Err(Compound) => {
                table.text.truncate(mark);
                let json = serde_json::to_string(value).map_err(|e| Error(e.into()))?;
                table.text(&json)
            }
        };
        let base = table.rows * width;
        table.vals[base + column] = val;
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
        let mut probe = Scalar { arena: None, val: None, text: None };
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
    arena: Option<&'t mut String>,
    val: Option<Val>,
    text: Option<String>,
}

impl Scalar<'_> {
    fn put(&mut self, value: &str) {
        match self.arena.as_deref_mut() {
            Some(arena) => {
                let start = arena.len() as u32;
                arena.push_str(value);
                self.val = Some(Val::Text(start, value.len() as u32));
            }
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
