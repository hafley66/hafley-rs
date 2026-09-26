use std::collections::HashMap;
use std::ffi::OsString;
use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use clap::Parser as _;
use serde::Serialize;
use serde_json::Value;

use crate::models::file_args::FileArgs;
use crate::ops_auto::{
    CleaveArgs, DiffArgs, ExtractArgs, FastArgs, GraphArgs, IngestArgs, MoveArgs, OpError, OpResult,
    QueryArgs, RegionArgs, RenameArgs, SchemaArgs, ScipArgs, SlowArgs, TrailArgs, WatchArgs,
};

fn flat_fields(value: &Value, fields: &mut HashMap<String, Value>) {
    if let Value::Object(object) = value {
        for (name, value) in object {
            if value.is_object() {
                flat_fields(value, fields);
            } else {
                let empty = value.is_null() || value.as_array().is_some_and(Vec::is_empty);
                if !empty || !fields.contains_key(name) {
                    fields.insert(name.clone(), value.clone());
                }
            }
        }
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        value => value.to_string(),
    }
}

fn argv<A: clap::Args + Serialize>(verb: &str, args: &A) -> OpResult<Vec<OsString>> {
    let mut fields = HashMap::new();
    flat_fields(&serde_json::to_value(args)?, &mut fields);
    let command = A::augment_args(clap::Command::new("ryi"));
    let mut flags = Vec::new();
    let mut positionals = Vec::new();
    for (ordinal, arg) in command.get_arguments().enumerate() {
        let Some(value) = fields.get(arg.get_id().as_str()) else { continue };
        if value.is_null() || value == false || value.as_array().is_some_and(Vec::is_empty) {
            continue;
        }
        let values: Vec<&Value> = match value {
            Value::Array(values) => values.iter().collect(),
            value => vec![value],
        };
        if let Some(long) = arg.get_long() {
            for value in values {
                flags.push(OsString::from(format!("--{long}")));
                if value != true {
                    flags.push(OsString::from(scalar(value)));
                }
            }
        } else {
            positionals.push((arg.get_index().unwrap_or(ordinal + 1), values.into_iter().map(scalar).collect::<Vec<_>>()));
        }
    }
    positionals.sort_by_key(|(index, _)| *index);
    let mut output = if verb.is_empty() { Vec::new() } else { vec![OsString::from(verb)] };
    output.extend(flags);
    for (_, values) in positionals {
        output.extend(values.into_iter().map(OsString::from));
    }
    Ok(output)
}

struct RowSink {
    tx: mpsc::SyncSender<OpResult<Vec<u8>>>,
    pending: Vec<u8>,
}

impl Write for RowSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut remaining = buf;
        while let Some(end) = remaining.iter().position(|byte| *byte == b'\n') {
            self.pending.extend_from_slice(&remaining[..=end]);
            remaining = &remaining[end + 1..];
            {
                let line = std::mem::take(&mut self.pending);
                self.tx.send(Ok(line)).map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
            }
        }
        self.pending.extend_from_slice(remaining);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.tx.send(Ok(line)).map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        }
        Ok(())
    }
}

#[derive(Clone)]
struct SharedSink(Arc<Mutex<RowSink>>);

impl Write for SharedSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

thread_local! {
    static OP_SINK: RefCell<Option<SharedSink>> = const { RefCell::new(None) };
    static OP_WRITE_ERROR: RefCell<Option<std::io::Error>> = const { RefCell::new(None) };
    static REQUEST_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub(crate) fn with_request_root<T>(root: PathBuf, run: impl FnOnce() -> T) -> T {
    REQUEST_ROOT.with(|slot| {
        let previous = slot.replace(Some(root));
        let result = run();
        slot.replace(previous);
        result
    })
}

pub(crate) fn request_root() -> PathBuf {
    REQUEST_ROOT.with(|slot| slot.borrow().clone()).unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn print_line(args: std::fmt::Arguments<'_>) {
    OP_SINK.with(|slot| {
        if let Some(sink) = slot.borrow_mut().as_mut() {
            if let Err(error) = writeln!(sink, "{args}") {
                OP_WRITE_ERROR.with(|failure| *failure.borrow_mut() = Some(error));
            }
        } else {
            let _ = writeln!(std::io::stdout().lock(), "{args}");
        }
    });
}

pub(crate) fn flush_line() {
    OP_SINK.with(|slot| {
        if let Some(sink) = slot.borrow_mut().as_mut() {
            if let Err(error) = sink.flush() {
                OP_WRITE_ERROR.with(|failure| *failure.borrow_mut() = Some(error));
            }
        } else {
            let _ = std::io::stdout().flush();
        }
    });
}

struct Rows {
    rx: mpsc::Receiver<OpResult<Vec<u8>>>,
    cancelled: Arc<AtomicBool>,
}

impl Iterator for Rows {
    type Item = OpResult<Vec<u8>>;
    fn next(&mut self) -> Option<Self::Item> { self.rx.recv().ok() }
}

impl Drop for Rows {
    fn drop(&mut self) { self.cancelled.store(true, Ordering::Release); }
}

fn produce<A: clap::Args + Serialize>(verb: &str, args: &A) -> Rows {
    let (tx, rx) = mpsc::sync_channel(64);
    let argv = argv(verb, args);
    let cancelled = Arc::new(AtomicBool::new(false));
    let operation_cancelled = Arc::clone(&cancelled);
    let request_root = request_root();
    std::thread::spawn(move || {
        let result = with_request_root(request_root, || -> OpResult<()> {
            let argv = argv?;
            let ryi = crate::Ryi::try_parse_from(std::iter::once(OsString::from("ryi")).chain(argv))
                .map_err(|error| OpError(error.to_string(), 2))?;
            let sink = SharedSink(Arc::new(Mutex::new(RowSink { tx: tx.clone(), pending: Vec::new() })));
            OP_SINK.with(|slot| *slot.borrow_mut() = Some(sink.clone()));
            let outcome = crate::run_verb(ryi, Box::new(sink.clone()), Some(operation_cancelled)).map_err(|error| {
                let message = if error.message.is_empty() { format!("ryi exited {}", error.code) } else { error.message };
                OpError(message, error.code)
            });
            OP_SINK.with(|slot| *slot.borrow_mut() = None);
            sink.0.lock().unwrap().flush()?;
            if let Some(error) = OP_WRITE_ERROR.with(|failure| failure.borrow_mut().take()) {
                return Err(OpError::from(error));
            }
            outcome
        });
        if let Err(error) = result { let _ = tx.send(Err(error)); }
    });
    Rows { rx, cancelled }
}

fn stream<A: clap::Args + Serialize>(verb: &str, args: &A) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    Box::new(produce(verb, args))
}

fn one<A: clap::Args + Serialize>(verb: &str, args: &A) -> OpResult<Vec<u8>> {
    let mut output = Vec::new();
    for line in produce(verb, args) {
        output.extend(line?);
    }
    Ok(output)
}

pub fn fast(args: &FastArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> { stream("fast", args) }
pub fn extract(args: &ExtractArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> { file(&args.args) }
pub fn file(args: &FileArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    let mut args = args.clone();
    args.format = None;
    stream("", &args)
}
pub fn slow(args: &SlowArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> { stream("slow", args) }
pub fn scip(args: &ScipArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> { stream("scip", args) }
pub fn graph(args: &GraphArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> { stream("graph", args) }
pub fn query(args: &QueryArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> { stream("query", args) }
pub fn watch(args: &WatchArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> { stream("watch", args) }
pub fn diff(args: &DiffArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> { stream("diff", args) }

pub fn cleave(args: &CleaveArgs) -> OpResult<Vec<u8>> { one("cleave", args) }
pub fn r#move(args: &MoveArgs) -> OpResult<Vec<u8>> { one("move", args) }
pub fn rename(args: &RenameArgs) -> OpResult<Vec<u8>> { one("rename", args) }
pub fn region(args: &RegionArgs) -> OpResult<Vec<u8>> { one("region", args) }
pub fn schema(args: &SchemaArgs) -> OpResult<Vec<u8>> { one("schema", args) }
pub fn trail(args: &TrailArgs) -> OpResult<Vec<u8>> { one("trail", args) }

pub fn ingest(args: &IngestArgs, input: impl Iterator<Item = OpResult<Value>>) -> OpResult<Vec<u8>> {
    let mut staged = tempfile::NamedTempFile::new()?;
    let mut received = false;
    for value in input {
        serde_json::to_writer(&mut staged, &value?)?;
        staged.write_all(b"\n")?;
        received = true;
    }
    if !received {
        return one("ingest", args);
    }
    let mut args = args.clone();
    args.paths = vec![staged.path().to_path_buf()];
    one("ingest", &args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_response_cancels_and_closes_its_row_sink() {
        let (tx, rx) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let rows = Rows { rx, cancelled: Arc::clone(&cancelled) };
        drop(rows);
        assert!(cancelled.load(Ordering::Acquire));
        let mut sink = RowSink { tx, pending: Vec::new() };
        assert_eq!(sink.write_all(b"row\n").unwrap_err().kind(), std::io::ErrorKind::BrokenPipe);
    }
}
