//! `extract diff`: the snapshot delta between two commits, one shot. Both
//! revisions are enumerated through soopy and every blob is read through
//! `git cat-file`, never through a checkout.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{params, Connection};
use serde::Serialize;

struct Options {
    root: PathBuf,
    from: String,
    to: String,
    patterns: Vec<soopy::Pattern>,
    sqlite: Option<PathBuf>,
}

/// One revision's snapshot: the full sha, the path-to-blob-oid map, and the
/// bytes `git cat-file` answered for every blob in it.
struct Side {
    sha: String,
    files: BTreeMap<String, String>,
    _bytes: BTreeMap<String, Arc<[u8]>>,
}

pub fn run(arguments: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse(arguments)?;
    let root = std::fs::canonicalize(&options.root)
        .map_err(|error| format!("extract diff root {}: {error}", options.root.display()))?;
    let repository = soopy::open(&root)?;
    let mut tree = soopy::SourceTree::open(repository.clone());
    let mut batch = soopy::GitBatch::open(&repository.root)?;
    let mut blobs: BTreeMap<String, Arc<[u8]>> = BTreeMap::new();
    let a = snapshot_side(&mut tree, &mut batch, &mut blobs, &options, &options.from)?;
    let b = snapshot_side(&mut tree, &mut batch, &mut blobs, &options, &options.to)?;

    let mut counts = Counts::default();
    let mut rows = file_rows(&a, &b, &mut counts);

    // The header counts every relation, so it is built last and sorted first.
    let mut sortable: Vec<(u8, String, DiffRow)> =
        vec![(0, String::new(), DiffRow::Run(run_row(&a, &b, &counts)))];
    sortable.extend(
        rows.into_iter()
            .map(|row| (row.rank(), row.sort_path().to_string(), row)),
    );
    sortable.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    match &options.sqlite {
        Some(path) => write_sqlite(path, &sortable),
        None => write_jsonl(&sortable),
    }
}

fn snapshot_side(
    tree: &mut soopy::SourceTree,
    batch: &mut soopy::GitBatch,
    blobs: &mut BTreeMap<String, Arc<[u8]>>,
    options: &Options,
    revision: &str,
) -> Result<Side, Box<dyn std::error::Error>> {
    let resolved = tree.resolve_revision(soopy::Revision::Named(Arc::from(revision)))?;
    let soopy::RevisionId::Commit(commit) = resolved else {
        return Err(format!("extract diff: {revision} is not a commit").into());
    };
    let snapshot = tree.snapshot(&soopy::SourceQuery {
        revision: soopy::Revision::Commit(commit.clone()),
        patterns: options.patterns.clone(),
    })?;
    let mut files = BTreeMap::new();
    let mut bytes = BTreeMap::new();
    for entry in &snapshot.files {
        let soopy::ContentId::GitBlob(oid) = &entry.content else {
            return Err(format!(
                "extract diff: {} at {revision} carries no Git blob",
                entry.source.path.0
            )
            .into());
        };
        let oid = oid.0.to_string();
        files.insert(entry.source.path.0.to_string(), oid.clone());
        if !blobs.contains_key(&oid) {
            let read = batch.read(&soopy::ObjectId(Arc::from(oid.as_str())))?;
            blobs.insert(oid.clone(), read);
        }
        bytes.insert(entry.source.path.0.to_string(), blobs[&oid].clone());
    }
    Ok(Side {
        sha: commit.0.to_string(),
        files,
        _bytes: bytes,
    })
}

fn parse(
    mut arguments: impl Iterator<Item = String>,
) -> Result<Options, Box<dyn std::error::Error>> {
    let _command = arguments.next();
    let root = arguments.next().ok_or(
        "usage: extract diff ROOT --from REV --to REV [--pattern GLOB] [--sqlite PATH] [--json]",
    )?;
    let mut from = None;
    let mut to = None;
    let mut patterns = Vec::new();
    let mut sqlite = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--from" => from = Some(arguments.next().ok_or("--from requires a revision")?),
            "--to" => to = Some(arguments.next().ok_or("--to requires a revision")?),
            "--pattern" => patterns.push(soopy::Pattern(
                arguments.next().ok_or("--pattern requires a glob")?.into(),
            )),
            "--sqlite" => {
                sqlite = Some(PathBuf::from(
                    arguments.next().ok_or("--sqlite requires a path")?,
                ))
            }
            // JSONL is the only wire this verb writes, so the flag is an
            // explicit alias for the default rather than a second mode.
            "--json" => {}
            unknown => return Err(format!("unknown extract diff argument {unknown}").into()),
        }
    }
    Ok(Options {
        root: PathBuf::from(root),
        from: from.ok_or("extract diff requires --from REV")?,
        to: to.ok_or("extract diff requires --to REV")?,
        patterns: if patterns.is_empty() {
            crate::watch::default_patterns()
        } else {
            patterns
        },
        sqlite,
    })
}

/// The four change words this wire carries. `changed` is the `file` relation's
/// digest inequality; `origin_changed` is one key answered by two legs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Change {
    Added,
    Removed,
    Changed,
    OriginChanged,
}

impl Change {
    fn as_str(self) -> &'static str {
        match self {
            Change::Added => "added",
            Change::Removed => "removed",
            Change::Changed => "changed",
            Change::OriginChanged => "origin_changed",
        }
    }
}

#[derive(Clone, Copy, Default, Serialize)]
struct ChangeCounts {
    added: u32,
    removed: u32,
    changed: u32,
    origin_changed: u32,
}

impl ChangeCounts {
    fn add(&mut self, change: Change) {
        match change {
            Change::Added => self.added += 1,
            Change::Removed => self.removed += 1,
            Change::Changed => self.changed += 1,
            Change::OriginChanged => self.origin_changed += 1,
        }
    }
}

#[derive(Default, Serialize)]
struct Counts {
    file: ChangeCounts,
    resolved_edge: ChangeCounts,
    resolved_type_edge: ChangeCounts,
    resolved_import: ChangeCounts,
    file_unresolved: ChangeCounts,
    unresolved: ChangeCounts,
}

#[derive(Serialize)]
struct RunRow {
    record: &'static str,
    from: String,
    to: String,
    files_a: usize,
    files_b: usize,
    changed_blobs: usize,
    counts: Counts,
}

#[derive(Serialize)]
struct FileRow {
    record: &'static str,
    relation: &'static str,
    change: &'static str,
    path: String,
    from_digest: Option<String>,
    to_digest: Option<String>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum DiffRow {
    Run(RunRow),
    File(FileRow),
}

impl DiffRow {
    /// The header is rank 0; the rest follow the brief's file-first order, which
    /// is also the order the counts object lists them in.
    fn rank(&self) -> u8 {
        match self {
            DiffRow::Run(_) => 0,
            DiffRow::File(_) => 1,
        }
    }

    fn sort_path(&self) -> &str {
        match self {
            DiffRow::Run(_) => "",
            DiffRow::File(row) => &row.path,
        }
    }
}

fn run_row(a: &Side, b: &Side, counts: &Counts) -> RunRow {
    RunRow {
        record: "diff_run",
        from: a.sha.clone(),
        to: b.sha.clone(),
        files_a: a.files.len(),
        files_b: b.files.len(),
        changed_blobs: counts.file.changed as usize,
        counts: Counts {
            file: counts.file,
            resolved_edge: counts.resolved_edge,
            resolved_type_edge: counts.resolved_type_edge,
            resolved_import: counts.resolved_import,
            file_unresolved: counts.file_unresolved,
            unresolved: counts.unresolved,
        },
    }
}

/// The `file` relation: this is the snapshot diff restated as rows, so it runs
/// before every resolved relation and its digests are the Git blob oids.
fn file_rows(a: &Side, b: &Side, counts: &mut Counts) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let mut push = |change: Change, path: &str, from: Option<&String>, to: Option<&String>| {
        counts.file.add(change);
        rows.push(DiffRow::File(FileRow {
            record: "diff_file",
            relation: "file",
            change: change.as_str(),
            path: path.to_string(),
            from_digest: from.cloned(),
            to_digest: to.cloned(),
        }));
    };
    for (path, from) in &a.files {
        match b.files.get(path) {
            None => push(Change::Removed, path, Some(from), None),
            Some(to) if to != from => push(Change::Changed, path, Some(from), Some(to)),
            Some(_) => {}
        }
    }
    for (path, to) in &b.files {
        if !a.files.contains_key(path) {
            push(Change::Added, path, None, Some(to));
        }
    }
    rows
}

fn write_jsonl(rows: &[(u8, String, DiffRow)]) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = std::io::stdout();
    let mut output = std::io::BufWriter::with_capacity(256 * 1024, stdout.lock());
    for (_, _, row) in rows {
        serde_json::to_writer(&mut output, row)?;
        output.write_all(b"\n")?;
    }
    output.flush()?;
    Ok(())
}

/// A private staging database published on success, refusing an existing path
/// exactly as `--sqlite` does for the per-file export.
fn write_sqlite(
    path: &Path,
    rows: &[(u8, String, DiffRow)],
) -> Result<(), Box<dyn std::error::Error>> {
    if path.as_os_str().is_empty() || path == Path::new(":memory:") {
        return Err("--sqlite requires a filesystem path for a new database".into());
    }
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(format!(
            "--sqlite: {} already exists; supply a new database path",
            path.display()
        )
        .into());
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = tempfile::Builder::new()
        .prefix(".extract-diff-sqlite-")
        .tempfile_in(parent)?;
    let connection = Connection::open(temporary.path())?;
    connection.execute_batch(DIFF_DDL)?;
    {
        let mut run_statement = connection.prepare(INSERT_RUN)?;
        let mut file_statement = connection.prepare(INSERT_FILE)?;
        for (ordinal, (_, _, row)) in rows.iter().enumerate() {
            let ordinal = ordinal as i64 + 1;
            match row {
                DiffRow::Run(row) => {
                    run_statement.execute(params![
                        row.record,
                        row.from,
                        row.to,
                        row.files_a as i64,
                        row.files_b as i64,
                        row.changed_blobs as i64,
                        serde_json::to_string(&row.counts)?,
                        ordinal
                    ])?;
                }
                DiffRow::File(row) => {
                    file_statement.execute(params![
                        row.record,
                        row.change,
                        row.path,
                        row.from_digest,
                        row.to_digest,
                        ordinal
                    ])?;
                }
            }
        }
    }
    connection.execute_batch("COMMIT;")?;
    connection.close().map_err(|(_, error)| error)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(path)?;
    let mut out = std::io::stdout().lock();
    let quoted = format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    writeln!(out, "Wrote {} ({} rows)", path.display(), rows.len())?;
    writeln!(out, "Tables: sqlite3 {quoted} '.tables'")?;
    writeln!(out, "Schema: sqlite3 {quoted} '.schema'")?;
    writeln!(
        out,
        "Query:  sqlite3 -header -column {quoted} 'SELECT record, change, path FROM diff_file;'"
    )?;
    Ok(())
}

const DIFF_DDL: &str = "\
BEGIN IMMEDIATE;
CREATE TABLE diff_run (
    record TEXT NOT NULL, from_sha TEXT NOT NULL, to_sha TEXT NOT NULL,
    files_a INTEGER NOT NULL, files_b INTEGER NOT NULL,
    changed_blobs INTEGER NOT NULL, counts TEXT NOT NULL, _row INTEGER NOT NULL);
CREATE TABLE diff_file (
    record TEXT NOT NULL, change TEXT NOT NULL, path TEXT NOT NULL,
    from_digest TEXT, to_digest TEXT, _row INTEGER NOT NULL);";

const INSERT_RUN: &str = "\
INSERT INTO diff_run
    (record, from_sha, to_sha, files_a, files_b, changed_blobs, counts, _row)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

const INSERT_FILE: &str = "\
INSERT INTO diff_file (record, change, path, from_digest, to_digest, _row)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6)";
