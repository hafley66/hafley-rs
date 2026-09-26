use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::{mpsc, Mutex, OnceLock};

use clap::Parser as _;
use serde::Serialize;
use serde_json::Value;

use crate::models::file_args::FileArgs;
use crate::ops_auto::{
    CleaveArgs, DiffArgs, FastArgs, GraphArgs, IngestArgs, MoveArgs, OpError, OpResult,
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

static STDOUT_GATE: OnceLock<Mutex<()>> = OnceLock::new();

struct RestoreStdout(i32);
impl Drop for RestoreStdout {
    fn drop(&mut self) {
        let _ = std::io::stdout().flush();
        unsafe { libc::dup2(self.0, libc::STDOUT_FILENO); libc::close(self.0); }
    }
}

/// The edit handlers still print through stdout. Redirect that fd only while
/// one handler runs, and relay its rows through a bounded channel. The gate
/// keeps concurrent HTTP requests from crossing streams.
fn produce<A: clap::Args + Serialize>(verb: &str, args: &A) -> mpsc::Receiver<OpResult<Vec<u8>>> {
    let (tx, rx) = mpsc::sync_channel(64);
    let argv = argv(verb, args);
    let captures_stdout = !matches!(verb, "" | "fast" | "slow" | "scip" | "ingest" | "schema" | "trail");
    std::thread::spawn(move || {
        let result = (|| -> OpResult<()> {
            let argv = argv?;
            let ryi = crate::Ryi::try_parse_from(std::iter::once(OsString::from("ryi")).chain(argv))
                .map_err(|error| OpError(error.to_string()))?;
            let (reader, writer) = UnixStream::pair()?;
            let sink = writer.try_clone()?;
            let _gate = captures_stdout.then(|| STDOUT_GATE.get_or_init(|| Mutex::new(())).lock().unwrap());
            let restore = if captures_stdout {
                let saved = unsafe { libc::dup(libc::STDOUT_FILENO) };
                if saved < 0 { return Err(OpError::from(std::io::Error::last_os_error())); }
                if unsafe { libc::dup2(writer.as_raw_fd(), libc::STDOUT_FILENO) } < 0 {
                    unsafe { libc::close(saved); }
                    return Err(OpError::from(std::io::Error::last_os_error()));
                }
                Some(RestoreStdout(saved))
            } else { None };
            drop(writer);
            let row_tx = tx.clone();
            let rows = std::thread::spawn(move || {
                let mut input = BufReader::new(reader);
                let mut line = Vec::new();
                loop {
                    line.clear();
                    match input.read_until(b'\n', &mut line) {
                        Ok(0) => break,
                        Ok(_) => { let _ = row_tx.send(Ok(line.clone())); },
                        Err(error) => { let _ = row_tx.send(Err(OpError::from(error))); break; },
                    }
                }
            });
            let outcome = crate::run_verb(ryi, Box::new(sink)).map_err(|error| OpError(error.to_string()));
            drop(restore);
            let _ = rows.join();
            outcome
        })();
        if let Err(error) = result { let _ = tx.send(Err(error)); }
    });
    rx
}

fn stream<A: clap::Args + Serialize>(verb: &str, args: &A) -> Box<dyn Iterator<Item = OpResult<Value>> + Send> {
    Box::new(produce(verb, args).into_iter().map(|line| {
        let line = line?;
        Ok(serde_json::from_slice(&line)?)
    }))
}

fn one<A: clap::Args + Serialize>(verb: &str, args: &A) -> OpResult<Value> {
    let mut output = Vec::new();
    for line in produce(verb, args) {
        output.extend(line?);
    }
    Ok(Value::String(String::from_utf8_lossy(&output).into_owned()))
}

pub fn fast(args: &FastArgs) -> Box<dyn Iterator<Item = OpResult<Value>> + Send> { stream("fast", args) }
pub fn file(args: &FileArgs) -> Box<dyn Iterator<Item = OpResult<Value>> + Send> {
    let mut args = args.clone();
    args.format = None;
    stream("", &args)
}
pub fn slow(args: &SlowArgs) -> Box<dyn Iterator<Item = OpResult<Value>> + Send> { stream("slow", args) }
pub fn scip(args: &ScipArgs) -> Box<dyn Iterator<Item = OpResult<Value>> + Send> { stream("scip", args) }
pub fn graph(args: &GraphArgs) -> Box<dyn Iterator<Item = OpResult<Value>> + Send> { stream("graph", args) }
pub fn query(args: &QueryArgs) -> Box<dyn Iterator<Item = OpResult<Value>> + Send> { stream("query", args) }
pub fn watch(args: &WatchArgs) -> Box<dyn Iterator<Item = OpResult<Value>> + Send> { stream("watch", args) }
pub fn diff(args: &DiffArgs) -> Box<dyn Iterator<Item = OpResult<Value>> + Send> { stream("diff", args) }

pub fn cleave(args: &CleaveArgs) -> OpResult<Value> { one("cleave", args) }
pub fn r#move(args: &MoveArgs) -> OpResult<Value> { one("move", args) }
pub fn rename(args: &RenameArgs) -> OpResult<Value> { one("rename", args) }
pub fn region(args: &RegionArgs) -> OpResult<Value> { one("region", args) }
pub fn schema(args: &SchemaArgs) -> OpResult<Value> { one("schema", args) }
pub fn trail(args: &TrailArgs) -> OpResult<Value> { one("trail", args) }

pub fn ingest(args: &IngestArgs, input: impl Iterator<Item = OpResult<Value>>) -> OpResult<Value> {
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
