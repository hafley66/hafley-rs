use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::thread::JoinHandle;

use clap::Args as _;
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

fn child_command<A: clap::Args + Serialize>(verb: &str, args: &A) -> OpResult<Command> {
    let binary = match std::env::var_os("RYI_BIN") {
        Some(path) => path,
        None => std::env::current_exe()?.into_os_string(),
    };
    let mut command = Command::new(binary);
    command.args(argv(verb, args)?);
    let cap = std::env::var("RYI_MAX_MEM_MB").ok().and_then(|value| value.parse::<usize>().ok()).unwrap_or(2048).min(2048);
    command.env("RYI_MAX_MEM_MB", cap.to_string());
    command.env("RYI_STREAM_FLUSH", "1");
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    Ok(command)
}

struct LegacyRows {
    child: Child,
    lines: std::io::Lines<BufReader<ChildStdout>>,
    stderr: Option<JoinHandle<String>>,
    done: bool,
}

impl LegacyRows {
    fn start<A: clap::Args + Serialize>(verb: &str, args: &A) -> OpResult<Self> {
        let mut command = child_command(verb, args)?;
        command.stdin(Stdio::null());
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        let stderr = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.take(64 * 1024).read_to_string(&mut text);
            text
        });
        Ok(Self { child, lines: BufReader::new(stdout).lines(), stderr: Some(stderr), done: false })
    }
}

impl Iterator for LegacyRows {
    type Item = OpResult<Value>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done { return None; }
        match self.lines.next() {
            Some(Ok(line)) => Some(serde_json::from_str(&line).map_err(OpError::from)),
            Some(Err(error)) => {
                self.done = true;
                Some(Err(OpError::from(error)))
            }
            None => {
                self.done = true;
                let status = self.child.wait().map_err(OpError::from);
                let stderr = self.stderr.take().and_then(|reader| reader.join().ok()).unwrap_or_default();
                match status {
                    Ok(status) if status.success() => None,
                    Ok(status) => Some(Err(OpError(format!("ryi exited {status}: {}", stderr.trim())))),
                    Err(error) => Some(Err(error)),
                }
            }
        }
    }
}

impl Drop for LegacyRows {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn stream<A: clap::Args + Serialize>(verb: &str, args: &A) -> Box<dyn Iterator<Item = OpResult<Value>> + Send> {
    match LegacyRows::start(verb, args) {
        Ok(rows) => Box::new(rows),
        Err(error) => Box::new(std::iter::once(Err(error))),
    }
}

fn one<A: clap::Args + Serialize>(verb: &str, args: &A) -> OpResult<Value> {
    let output = child_command(verb, args)?.stdin(Stdio::null()).output()?;
    if !output.status.success() {
        return Err(OpError(format!("ryi exited {}: {}", output.status, String::from_utf8_lossy(&output.stderr).trim())));
    }
    Ok(Value::String(String::from_utf8_lossy(&output.stdout).into_owned()))
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
