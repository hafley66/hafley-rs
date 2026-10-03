//! lab_writer write ENGINE INPUT.db OUT | append ENGINE OUT BATCHES ROWS PAUSE_MS | sql QUERY.scm
//! W1 writer, X1 appender, Q1 SQL printer; usage per mode in README.md.
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const DICTS: [&str; 5] = ["scmpp_dict_path", "scmpp_dict_kind", "scmpp_dict_field", "scmpp_dict_capture", "scmpp_dict_text"];
const NODE_COLUMNS: usize = 12;
const CAPTURE_COLUMNS: usize = 8;
const BATCH: usize = 256;

/// ryii's scm++ DDL (crates/sprefa-extract/src/bin/ryi/1b_scmpp_sqlite.rs), verbatim.
const SQLITE_DDL: &str = "\
CREATE TABLE scmpp_dict_path (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_dict_kind (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_dict_field (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_dict_capture (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_dict_text (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_node (file INTEGER NOT NULL, pre INTEGER NOT NULL, last INTEGER NOT NULL, \
parent INTEGER NOT NULL, depth INTEGER NOT NULL, sib INTEGER, idx INTEGER NOT NULL, kind INTEGER NOT NULL, \
field INTEGER NOT NULL, start INTEGER NOT NULL, \"end\" INTEGER NOT NULL, named INTEGER NOT NULL, \
PRIMARY KEY (file, pre)) WITHOUT ROWID;
CREATE TABLE scmpp_capture (file INTEGER NOT NULL, pattern INTEGER NOT NULL, \"match\" INTEGER NOT NULL, \
capture INTEGER NOT NULL, node INTEGER NOT NULL, start INTEGER NOT NULL, \"end\" INTEGER NOT NULL, text INTEGER, \
PRIMARY KEY (file, pattern, \"match\", capture, node)) WITHOUT ROWID;";

const SQLITE_INDEXES: &str = "\
CREATE INDEX IF NOT EXISTS scmpp_node_sibling ON scmpp_node(file, parent, sib);
CREATE INDEX IF NOT EXISTS scmpp_capture_node ON scmpp_capture(pattern, capture, file, node);";

/// The same tables in DuckDB types; `pk` keeps the primary and unique keys (ART indexes).
#[cfg(feature = "duck")]
fn duck_ddl(pk: bool) -> String {
    let (key, unique) = if pk { (true, " UNIQUE") } else { (false, "") };
    let mut ddl = String::new();
    for dict in DICTS {
        ddl += &format!("CREATE TABLE {dict} (id BIGINT{}, text VARCHAR NOT NULL{unique});\n", if key { " PRIMARY KEY" } else { "" });
    }
    ddl += "CREATE TABLE scmpp_node (file BIGINT NOT NULL, pre BIGINT NOT NULL, last BIGINT NOT NULL, \
parent BIGINT NOT NULL, depth BIGINT NOT NULL, sib BIGINT, idx BIGINT NOT NULL, kind BIGINT NOT NULL, \
field BIGINT NOT NULL, start BIGINT NOT NULL, \"end\" BIGINT NOT NULL, named BIGINT NOT NULL";
    ddl += if pk { ", PRIMARY KEY (file, pre));\n" } else { ");\n" };
    ddl += "CREATE TABLE scmpp_capture (file BIGINT NOT NULL, pattern BIGINT NOT NULL, \"match\" BIGINT NOT NULL, \
capture BIGINT NOT NULL, node BIGINT NOT NULL, start BIGINT NOT NULL, \"end\" BIGINT NOT NULL, text BIGINT";
    ddl += if pk { ", PRIMARY KEY (file, pattern, \"match\", capture, node));\n" } else { ");\n" };
    ddl
}

struct Input {
    dicts: Vec<(&'static str, Vec<(i64, String)>)>,
    nodes: Vec<[Option<i64>; NODE_COLUMNS]>,
    captures: Vec<[Option<i64>; CAPTURE_COLUMNS]>,
}

fn read_input(path: &str) -> Result<Input> {
    let db = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut dicts = Vec::new();
    for dict in DICTS {
        let mut statement = db.prepare(&format!("SELECT id, text FROM {dict} ORDER BY id"))?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        dicts.push((dict, rows));
    }
    let mut statement = db.prepare("SELECT file, pre, last, parent, depth, sib, idx, kind, field, start, \"end\", named FROM scmpp_node")?;
    let nodes = statement
        .query_map([], |row| {
            let mut out = [None; NODE_COLUMNS];
            for (index, slot) in out.iter_mut().enumerate() {
                *slot = row.get(index)?;
            }
            Ok(out)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut statement = db.prepare("SELECT file, pattern, \"match\", capture, node, start, \"end\", text FROM scmpp_capture")?;
    let captures = statement
        .query_map([], |row| {
            let mut out = [None; CAPTURE_COLUMNS];
            for (index, slot) in out.iter_mut().enumerate() {
                *slot = row.get(index)?;
            }
            Ok(out)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Input { dicts, nodes, captures })
}

fn insert_sql(table: &str, columns: usize, rows: usize) -> String {
    let row = format!("({})", vec!["?"; columns].join(","));
    format!("INSERT INTO {table} VALUES {}", vec![row; rows].join(","))
}

/// As ryii: ryii's pragmas, one transaction, dict rows one statement each, node/capture 256-row statements.
fn write_sqlite(input: &Input, out: &str) -> Result<f64> {
    let db = rusqlite::Connection::open(out)?;
    db.execute_batch(
        "PRAGMA page_size=65536; PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; \
         PRAGMA cache_size=-16384; PRAGMA temp_store=MEMORY; BEGIN IMMEDIATE;",
    )?;
    db.execute_batch(SQLITE_DDL)?;
    for (dict, rows) in &input.dicts {
        let mut statement = db.prepare_cached(&format!("INSERT INTO {dict} (id, text) VALUES (?1, ?2)"))?;
        for (id, text) in rows {
            statement.execute(rusqlite::params![id, text])?;
        }
    }
    fn batches<const N: usize>(db: &rusqlite::Connection, table: &str, rows: &[[Option<i64>; N]]) -> Result<()> {
        for chunk in rows.chunks(BATCH) {
            let sql = insert_sql(table, N, chunk.len());
            let mut statement = db.prepare_cached(&sql)?;
            statement.execute(rusqlite::params_from_iter(chunk.iter().flatten()))?;
        }
        Ok(())
    }
    batches(&db, "scmpp_node", &input.nodes)?;
    batches(&db, "scmpp_capture", &input.captures)?;
    db.execute_batch("COMMIT;")?;
    let started = Instant::now();
    db.execute_batch(SQLITE_INDEXES)?;
    Ok(started.elapsed().as_secs_f64())
}

#[cfg(feature = "duck")]
fn write_duckdb(input: &Input, out: &str, pk: bool) -> Result<f64> {
    let db = duckdb::Connection::open(out)?;
    db.execute_batch(&duck_ddl(pk))?;
    for (dict, rows) in &input.dicts {
        let mut appender = db.appender(dict)?;
        for (id, text) in rows {
            appender.append_row(duckdb::params![id, text])?;
        }
    }
    {
        let mut appender = db.appender("scmpp_node")?;
        for row in &input.nodes {
            appender.append_row(duckdb::appender_params_from_iter(row.iter()))?;
        }
    }
    {
        let mut appender = db.appender("scmpp_capture")?;
        for row in &input.captures {
            appender.append_row(duckdb::appender_params_from_iter(row.iter()))?;
        }
    }
    let started = Instant::now();
    db.execute_batch("CHECKPOINT;")?;
    Ok(started.elapsed().as_secs_f64())
}

fn write(engine: &str, input_path: &str, out: &str) -> Result<()> {
    let started = Instant::now();
    let input = read_input(input_path)?;
    let load = started.elapsed().as_secs_f64();
    let _ = std::fs::remove_file(out);
    let _ = std::fs::remove_file(format!("{out}.wal"));
    let started = Instant::now();
    let index = match engine {
        "none" => 0.0,
        "sqlite" => write_sqlite(&input, out)?,
        #[cfg(feature = "duck")]
        "duckdb" => write_duckdb(&input, out, false)?,
        #[cfg(feature = "duck")]
        "duckdb_pk" => write_duckdb(&input, out, true)?,
        other => return Err(format!("unknown engine {other}").into()),
    };
    let wall = started.elapsed().as_secs_f64();
    let dict_rows: usize = input.dicts.iter().map(|(_, rows)| rows.len()).sum();
    let rows = dict_rows + input.nodes.len() + input.captures.len();
    println!(
        "{engine}\t{dict_rows}\t{}\t{}\t{load:.3}\t{wall:.3}\t{index:.3}\t{:.0}",
        input.nodes.len(),
        input.captures.len(),
        rows as f64 / wall.max(1e-9)
    );
    Ok(())
}

/// X1: BATCHES transactions of ROWS rows into x1_row; stdout per batch: batch, commit_unix_ms, open_retries, error.
fn append(engine: &str, out: &str, batches: i64, rows: i64, pause: u64) -> Result<()> {
    let now_ms = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    match engine {
        "sqlite" => {
            let db = rusqlite::Connection::open(out)?;
            db.busy_timeout(Duration::from_secs(5))?;
            db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; CREATE TABLE IF NOT EXISTS x1_row (batch INTEGER NOT NULL, seq INTEGER NOT NULL, value INTEGER NOT NULL);")?;
            for batch in 0..batches {
                let result = (|| -> Result<()> {
                    db.execute_batch("BEGIN IMMEDIATE")?;
                    for chunk in (0..rows).collect::<Vec<_>>().chunks(BATCH) {
                        let mut statement = db.prepare_cached(&insert_sql("x1_row", 3, chunk.len()))?;
                        let params: Vec<i64> = chunk.iter().flat_map(|seq| [batch, *seq, batch * rows + seq]).collect();
                        statement.execute(rusqlite::params_from_iter(params))?;
                    }
                    db.execute_batch("COMMIT")?;
                    Ok(())
                })();
                println!("{batch}\t{}\t0\t{}", now_ms(), result.err().map(|e| e.to_string()).unwrap_or_default());
                std::thread::sleep(Duration::from_millis(pause));
            }
        }
        #[cfg(feature = "duck")]
        "duckdb" | "duckdb_reopen" => {
            let reopen = engine == "duckdb_reopen";
            let open = |retries: &mut i64| -> duckdb::Connection {
                loop {
                    match duckdb::Connection::open(out) {
                        Ok(db) => return db,
                        Err(_) => {
                            *retries += 1;
                            std::thread::sleep(Duration::from_millis(5));
                        }
                    }
                }
            };
            let mut retries = 0;
            let mut held = Some(open(&mut retries));
            held.as_ref().unwrap().execute_batch("CREATE TABLE IF NOT EXISTS x1_row (batch BIGINT NOT NULL, seq BIGINT NOT NULL, value BIGINT NOT NULL);")?;
            if reopen {
                held = None;
            }
            for batch in 0..batches {
                let mut retries = 0;
                let fresh = if held.is_none() { Some(open(&mut retries)) } else { None };
                let db = held.as_ref().or(fresh.as_ref()).unwrap();
                let result = (|| -> Result<()> {
                    let mut appender = db.appender("x1_row")?;
                    for seq in 0..rows {
                        appender.append_row(duckdb::params![batch, seq, batch * rows + seq])?;
                    }
                    appender.flush()?;
                    Ok(())
                })();
                if reopen {
                    db.execute_batch("CHECKPOINT")?;
                }
                println!("{batch}\t{}\t{retries}\t{}", now_ms(), result.err().map(|e| e.to_string()).unwrap_or_default());
                std::thread::sleep(Duration::from_millis(pause));
            }
        }
        other => return Err(format!("unknown engine {other}").into()),
    }
    Ok(())
}

#[cfg(feature = "sql")]
fn sql(path: &str) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let compiled = hafley_scm::scmpp::compile(&language, &text).map_err(|error| error.to_string())?;
    println!("{}", compiled.sql);
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |index: usize| args.get(index).map(String::as_str).ok_or("missing argument; see the header of src/main.rs");
    match arg(1)? {
        "write" => write(arg(2)?, arg(3)?, arg(4)?),
        "append" => append(arg(2)?, arg(3)?, arg(4)?.parse()?, arg(5)?.parse()?, arg(6)?.parse()?),
        #[cfg(feature = "sql")]
        "sql" => sql(arg(2)?),
        other => Err(format!("unknown mode {other}").into()),
    }
}
