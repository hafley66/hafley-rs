//! `ryi graph`: one resolve pass landed in the SQLite fact store, then a
//! question asked of it as SQL over the `callers`/`uses`/`reach` views.
//! @comment-ok: module header, the seam list every bin arm opens with

use std::fs;
use std::path::{Path, PathBuf};

use clap::{ArgGroup, Parser};
use rusqlite::Connection;
use sprefa_extract::lang::source_for;
use sprefa_extract::{resolve_project, FlatFact, ResolveArms, ResolveRequest, ScipMode, ScipRecords};

use crate::sqlite::{reach_walk_sql, Database};

/// The file `--state DIR` leaves behind, the one a caller opens by hand.
const STATE_DB: &str = "graph.db";

const CALLERS_SQL: &str = "SELECT \"callee_path\", \"callee_name\", \"caller_path\", \
                           \"caller_name\", \"grade\", \"kind\" FROM \"callers\" \
                           WHERE \"callee_name\" IS ?1";

const USES_SQL: &str = "SELECT \"type_path\", \"type_name\", \"user_path\", \"user_name\", \
                        \"grade\", \"kind\" FROM \"uses\" WHERE \"type_name\" IS ?1";

/// One resolve pass, landed in the store the views read. `--state` publishes
/// the store; without it the whole thing lives and dies in memory.
fn load_store(
    paths: &[PathBuf],
    arms: ResolveArms,
    state: Option<&Path>,
) -> Result<Database, Box<dyn std::error::Error>> {
    let request = ResolveRequest {
        paths,
        arms,
        scip: ScipMode::Off,
        project_root: None,
        scip_records: ScipRecords::default(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        go_checker: None,
        witness: false,
    };
    let facts = resolve_project(&request)?;
    let mut database = match state {
        Some(directory) => {
            fs::create_dir_all(directory)?;
            Database::create(&directory.join(STATE_DB))?
        }
        None => Database::memory()?,
    };
    for fact in &facts {
        // Only the two tables the views read: a record the schema has no table
        // for would refuse the insert, and none of them answer these questions.
        if !matches!(
            fact,
            FlatFact::ResolvedEdge { .. } | FlatFact::ResolvedTypeEdge { .. }
        ) {
            continue;
        }
        database.insert(serde_json::to_value(fact)?)?;
    }
    database.flush()?;
    Ok(database)
}

/// `callers` and `uses` project the same six columns in the same order, so one
/// reader serves both. NAME travels as `?1`, never spliced into the SQL.
fn edges(
    connection: &Connection,
    sql: &str,
    name: &str,
) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(sql)?;
    let mut rows = statement
        .query_map([name], |row| {
            Ok(FlatFact::GraphEdge {
                from_path: row.get(0)?,
                from_name: row.get(1)?,
                to_path: row.get(2)?,
                to_name: row.get(3)?,
                kind: row.get(5)?,
                grade: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.sort_by_key(|edge| serde_json::to_string(edge).expect("graph edge serializes"));
    Ok(rows)
}

/// The reach closure seeded at NAME: one row per node it reaches, at the
/// shortest depth, graded by the edge that discovered it there.
fn nodes(
    connection: &Connection,
    name: &str,
) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
    let sql = format!(
        "{} SELECT \"dst_path\", \"dst_name\", min(\"depth\"), \"grade\" FROM \"walk\" \
         GROUP BY \"dst_path\", \"dst_name\" ORDER BY 3, 1, 2",
        reach_walk_sql("e.\"caller_name\" IS ?1")
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement
        .query_map([name], |row| {
            Ok(FlatFact::GraphNode {
                path: row.get(0)?,
                name: row.get(1)?,
                depth: row.get(2)?,
                grade: row.get(3)?,
                line: None,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[derive(Default)]
struct GradeSplit {
    plus: u32,
    tilde: u32,
    minus: u32,
}

impl GradeSplit {
    fn bump(&mut self, grade: &str) {
        match grade {
            "+" => self.plus += 1,
            "-" => self.minus += 1,
            _ => self.tilde += 1,
        }
    }
}

fn emit_rows(rows: &[FlatFact]) -> Result<(), Box<dyn std::error::Error>> {
    for row in rows {
        println!("{}", serde_json::to_string(row)?);
    }
    Ok(())
}

fn emit_summary_line(rows: &[FlatFact]) {
    let mut split = GradeSplit::default();
    for row in rows {
        match row {
            FlatFact::GraphEdge { grade, .. } | FlatFact::GraphNode { grade, .. } => {
                split.bump(grade)
            }
            _ => {}
        }
    }
    eprintln!(
        "{} edges: {} +, {} ~, {} -",
        rows.len(),
        split.plus,
        split.tilde,
        split.minus
    );
}

/// Every file under `paths` the roster claims. No suffix is spelled here, so a
/// language the roster gains is walked by this verb the same day.
fn expand_paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    let mut pending = paths.to_vec();
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else if source_for(&path.to_string_lossy()).is_some() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// The views and the out-of-scope list, stated in help so a caller reads them
/// before the run rather than after.
const SCOPE: &str = "Every arm is a SQL view over the fact store, not a traversal in Rust. With \
                     --state DIR the store is published as DIR/graph.db (a new path each run; an \
                     existing one is refused), so the same question re-asks by hand: sqlite3 \
                     DIR/graph.db 'SELECT * FROM callers WHERE callee_name = ''deep'''. The three \
                     views are callers(callee_path, callee_name, caller_path, caller_name, grade, \
                     kind) over resolved_edge, uses(type_path, type_name, user_path, user_name, \
                     grade, kind) over resolved_type_edge, and reach(src_path, src_name, \
                     dst_path, dst_name, depth), the recursive closure of resolved_edge capped at \
                     32 hops.\n\nExactly one of --callers, --uses and --from is required. Out of \
                     scope, each its own issue: a persistent cross-run graph index (dl8 owns it), \
                     a maintained liveness or dead-code view, and grading a reach hop by anything \
                     but the edge that discovered it.";

#[derive(Parser)]
#[command(
    name = "ryi graph",
    about = "ask one question of the resolved call and type graph of a corpus",
    after_help = SCOPE
)]
#[command(group(ArgGroup::new("arm").required(true).args(["callers", "uses", "from"])))]
pub struct GraphCli {
    /// Files and directories. A directory is walked; every path the language
    /// roster claims is read, and nothing else.
    #[arg(required = true, value_name = "PATH")]
    paths: Vec<PathBuf>,
    /// Who calls NAME: one row per resolved call edge landing on it.
    #[arg(long, value_name = "NAME")]
    callers: Option<String>,
    /// Who references the type NAME: one row per referencing declaration.
    #[arg(long, value_name = "NAME")]
    uses: Option<String>,
    /// What NAME reaches along resolved call edges, transitively.
    #[arg(long, value_name = "NAME")]
    from: Option<String>,
    /// Publish the fact store as DIR/graph.db instead of keeping it in memory.
    #[arg(long, value_name = "DIR")]
    state: Option<PathBuf>,
    /// Drop the stderr summary line; stdout is JSONL either way.
    #[arg(long)]
    json: bool,
}

/// Which resolve arm each question needs, and which view answers it.
enum Arm<'a> {
    Callers(&'a str),
    Uses(&'a str),
    From(&'a str),
}

impl Arm<'_> {
    fn arms(&self) -> ResolveArms {
        match self {
            Arm::Uses(_) => ResolveArms {
                types: true,
                ..ResolveArms::default()
            },
            _ => ResolveArms {
                call: true,
                ..ResolveArms::default()
            },
        }
    }

    fn ask(&self, connection: &Connection) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
        match self {
            Arm::Callers(name) => edges(connection, CALLERS_SQL, name),
            Arm::Uses(name) => edges(connection, USES_SQL, name),
            Arm::From(name) => nodes(connection, name),
        }
    }
}

pub fn run<I>(args: I) -> Result<(), Box<dyn std::error::Error>>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    // `exit` rather than a returned error: `--help` is an `Err` to clap, and
    // only clap's own exit prints it to stdout with status 0.
    let cli = match GraphCli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };
    let arm = match (&cli.callers, &cli.uses, &cli.from) {
        (Some(name), _, _) => Arm::Callers(name),
        (_, Some(name), _) => Arm::Uses(name),
        (_, _, Some(name)) => Arm::From(name),
        _ => unreachable!("the clap ArgGroup requires one of the three"),
    };
    let paths = expand_paths(&cli.paths)?;
    let database = load_store(&paths, arm.arms(), cli.state.as_deref())?;
    let rows = arm.ask(database.connection())?;
    database.close()?;
    emit_rows(&rows)?;
    if !cli.json {
        emit_summary_line(&rows);
    }
    Ok(())
}
