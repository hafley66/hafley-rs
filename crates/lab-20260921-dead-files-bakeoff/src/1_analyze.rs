use crate::types::{DeadFile, FileEdge};
use ignore::WalkBuilder;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

pub fn read_edges(input: impl BufRead) -> io::Result<Vec<FileEdge>> {
    input
        .lines()
        .filter_map(|line| match line {
            Ok(line) if line.trim().is_empty() => None,
            Ok(line) => Some(serde_json::from_str(&line).map_err(io::Error::other)),
            Err(error) => Some(Err(error)),
        })
        .collect()
}

pub fn dead_files(
    root: &Path,
    edges: &[FileEdge],
) -> Result<Vec<DeadFile>, Box<dyn std::error::Error>> {
    let root = root.canonicalize()?;
    let mut files = BTreeSet::new();
    for result in WalkBuilder::new(&root).hidden(false).build() {
        let entry = result?;
        if entry.file_type().is_some_and(|kind| kind.is_file()) {
            let relative = entry.path().strip_prefix(&root)?;
            let path = slash_path(relative);
            if is_source(&path) {
                files.insert(path);
            }
        }
    }

    let entrypoints = entrypoints(&root, &files)?;
    let mut db = Connection::open_in_memory()?;
    db.execute_batch(
        "CREATE TABLE file(path TEXT PRIMARY KEY, entrypoint INTEGER NOT NULL);
         CREATE TABLE file_edge(src_path TEXT NOT NULL, dst_path TEXT NOT NULL);
         CREATE VIEW dead_file AS
         SELECT file.path FROM file
         WHERE file.entrypoint = 0
           AND NOT EXISTS (SELECT 1 FROM file_edge WHERE file_edge.dst_path = file.path);",
    )?;
    {
        let tx = db.transaction()?;
        for path in &files {
            tx.execute(
                "INSERT INTO file(path, entrypoint) VALUES (?1, ?2)",
                params![path, entrypoints.contains(path) as i64],
            )?;
        }
        for edge in edges.iter().filter(|edge| edge.record == "file_edge") {
            tx.execute(
                "INSERT INTO file_edge(src_path, dst_path) VALUES (?1, ?2)",
                params![clean_path(&edge.src_path), clean_path(&edge.dst_path)],
            )?;
        }
        tx.commit()?;
    }
    let mut statement = db.prepare("SELECT path FROM dead_file ORDER BY path")?;
    let rows = statement.query_map([], |row| {
        let path: String = row.get(0)?;
        Ok(DeadFile {
            path: PathBuf::from(path),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn compare_orphans(
    ryi: &[DeadFile],
    tool_output: &str,
    root: &Path,
) -> Result<OrphanComparison, Box<dyn std::error::Error>> {
    let files = source_files(root)?;
    let ryi = ryi
        .iter()
        .map(|file| clean_path(&file.path.to_string_lossy()))
        .collect::<BTreeSet<_>>();
    let tool: BTreeSet<String> = if let Ok(json) = serde_json::from_str::<Value>(tool_output) {
        let mut paths = BTreeSet::new();
        collect_tool_paths(&json, &mut paths);
        paths
            .into_iter()
            .map(|path| clean_path(&path))
            .filter(|path| files.contains(path))
            .collect()
    } else {
        tool_output
            .lines()
            .map(str::trim)
            .filter(|line| {
                !line.is_empty() && !line.starts_with('-') && !line.starts_with("Processed ")
            })
            .map(clean_path)
            .filter(|path| files.contains(path))
            .collect()
    };
    Ok(OrphanComparison {
        both: ryi.intersection(&tool).cloned().collect(),
        ryi_only: ryi.difference(&tool).cloned().collect(),
        tool_only: tool.difference(&ryi).cloned().collect(),
    })
}

pub fn rustc_dead_code_files(
    cargo_json: &str,
    root: &Path,
) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let root = root.canonicalize()?;
    let mut files = BTreeSet::new();
    for line in cargo_json.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("reason").and_then(Value::as_str) != Some("compiler-message")
            || value.pointer("/message/code/code").and_then(Value::as_str) != Some("dead_code")
        {
            continue;
        }
        if let Some(spans) = value.pointer("/message/spans").and_then(Value::as_array) {
            for span in spans {
                if span.get("is_primary").and_then(Value::as_bool) != Some(true) {
                    continue;
                }
                let Some(path) = span.get("file_name").and_then(Value::as_str) else {
                    continue;
                };
                let path = Path::new(path);
                let path = if path.is_absolute() {
                    path.canonicalize()?.strip_prefix(&root)?.to_path_buf()
                } else {
                    path.to_path_buf()
                };
                files.insert(slash_path(&path));
            }
        }
    }
    Ok(files)
}

fn collect_tool_paths(value: &Value, paths: &mut BTreeSet<String>) {
    match value {
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_tool_paths(value, paths)),
        Value::String(path) => {
            paths.insert(path.clone());
        }
        Value::Object(values) => {
            for (key, value) in values {
                if matches!(key.as_str(), "file" | "filePath" | "path") {
                    if let Some(path) = value.as_str() {
                        paths.insert(path.to_owned());
                    }
                }
                collect_tool_paths(value, paths);
            }
        }
        _ => {}
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct OrphanComparison {
    pub both: Vec<String>,
    pub ryi_only: Vec<String>,
    pub tool_only: Vec<String>,
}

fn source_files(root: &Path) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let root = root.canonicalize()?;
    let mut files = BTreeSet::new();
    for result in WalkBuilder::new(&root).hidden(false).build() {
        let entry = result?;
        if entry.file_type().is_some_and(|kind| kind.is_file()) {
            let path = slash_path(entry.path().strip_prefix(&root)?);
            if is_source(&path) {
                files.insert(path);
            }
        }
    }
    Ok(files)
}

fn entrypoints(
    root: &Path,
    files: &BTreeSet<String>,
) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let mut entries = BTreeSet::new();
    for path in files {
        if matches!(path.as_str(), "src/main.rs" | "src/lib.rs")
            || path.starts_with("tests/")
            || path.starts_with("benches/")
        {
            entries.insert(path.clone());
        }
    }
    let package = root.join("package.json");
    if package.is_file() {
        let value: Value = serde_json::from_slice(&fs::read(package)?)?;
        for key in ["main", "module", "types", "typings"] {
            if let Some(path) = value.get(key).and_then(Value::as_str) {
                add_existing(&mut entries, files, path);
            }
        }
        for key in ["exports", "bin"] {
            if let Some(value) = value.get(key) {
                collect_package_paths(value, &mut entries, files);
            }
        }
    }
    Ok(entries)
}

fn collect_package_paths(value: &Value, entries: &mut BTreeSet<String>, files: &BTreeSet<String>) {
    match value {
        Value::String(path) => add_existing(entries, files, path),
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_package_paths(value, entries, files)),
        Value::Object(values) => values
            .values()
            .for_each(|value| collect_package_paths(value, entries, files)),
        _ => {}
    }
}

fn add_existing(entries: &mut BTreeSet<String>, files: &BTreeSet<String>, path: &str) {
    let path = clean_path(path.trim_start_matches("./"));
    if files.contains(&path) {
        entries.insert(path);
    }
}

fn clean_path(path: &str) -> String {
    path.trim_start_matches("./").replace('\\', "/")
}

fn is_source(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "rs")
    )
}

fn slash_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
