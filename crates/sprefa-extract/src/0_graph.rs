//! `ryi graph`: one resolve pass landed in the SQLite fact store, then a
//! question asked of it as SQL over the `callers`/`uses`/`reach` views.
//! @comment-ok: module header, the seam list every bin arm opens with

use crate::cli::GraphArgs;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use sprefa_extract::{
    resolve_project_with_tsi_tiers, FlatFact, ResolveArms, ResolveRequest, ScipMode, ScipRecords,
};

use crate::sqlite::{reach_walk_sql, Database};

const CALLERS_SQL: &str = "SELECT \"callee_path\", \"callee_name\", \"caller_path\", \
                           \"caller_name\", \"grade\", \"kind\" FROM \"callers\" \
                           WHERE \"callee_name\" IS ?1";

const USES_SQL: &str = "SELECT \"type_path\", \"type_name\", \"user_path\", \"user_name\", \
                        \"grade\", \"kind\" FROM \"uses\" WHERE \"type_name\" IS ?1";

/// One resolve pass, landed in the store the views read. `--sqlite` publishes
/// the store; without it the whole thing lives and dies in memory.
fn load_store(
    paths: &[PathBuf],
    arms: ResolveArms,
    cli: &GraphArgs,
    revision_root: Option<&Path>,
    sqlite: Option<&Path>,
) -> Result<Database, Box<dyn std::error::Error>> {
    let request = ResolveRequest {
        paths,
        arms,
        scip: ScipMode::from_flags(cli.scip_index.as_deref(), false),
        project_root: revision_root.or(cli.inputs.root.as_deref()),
        scip_records: ScipRecords::default(),
        occurrence_text: false,
        rust_checker: cli.rust_checker.then_some(cli.inputs.root.as_deref()).flatten(),
        ts_checker: cli.ts_checker.then_some(cli.inputs.root.as_deref()).flatten(),
        go_checker: cli.go_checker.then_some(cli.inputs.root.as_deref()).flatten(),
        witness: true,
    };
    let facts = resolve_project_with_tsi_tiers(&request)?;
    let mut database = match sqlite {
        Some(path) => Database::create(path)?,
        None => Database::memory()?,
    };
    for fact in &facts {
        // The graph views read the resolved edges. The TSI envelope makes the
        // syntax and checker type evidence queryable from the same store.
        if !matches!(
            fact,
            FlatFact::ResolvedEdge { .. }
                | FlatFact::ResolvedTypeEdge { .. }
                | FlatFact::Protocol { .. }
                | FlatFact::Run(_)
                | FlatFact::Fact(_)
                | FlatFact::Witness(_)
                | FlatFact::Coverage(_)
                | FlatFact::Diagnostic(_)
                | FlatFact::ResolvedImportRow { .. }
                | FlatFact::FileUnresolvedRow { .. }
                | FlatFact::Unresolved { .. }
                | FlatFact::FlowEdgeOut { .. }
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

/// Flow identity is a content digest and byte span. The seed uses the final
/// @ to separate the digest from START:END.
fn flow_paths(
    connection: &Connection,
    seed: &str,
) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
    let (blob, span) = seed.rsplit_once('@').ok_or("flow seed must be BLOB@START:END")?;
    let (start, end) = span.split_once(':').ok_or("flow seed must be BLOB@START:END")?;
    let start: u32 = start.parse()?;
    let end: u32 = end.parse()?;
    let sql = "WITH RECURSIVE walk(src_path, src_name, dst_path, dst_name, depth, witness, visited) AS (
        SELECT e.from_blob, printf('%d:%d', e.from__start, e.from__end),
               e.to_blob, printf('%d:%d', e.to__start, e.to__end),
               1, json_array(e._row), printf('|%d|', e._row)
          FROM flow_edge AS e
         WHERE e.from_blob = ?1 AND e.from__start = ?2 AND e.from__end = ?3
        UNION ALL
        SELECT w.src_path, w.src_name, e.to_blob,
               printf('%d:%d', e.to__start, e.to__end),
               w.depth + 1, json_insert(w.witness, '$[#]', e._row),
               w.visited || e._row || '|'
          FROM walk AS w JOIN flow_edge AS e
            ON e.from_blob = w.dst_path
           AND printf('%d:%d', e.from__start, e.from__end) = w.dst_name
         WHERE w.depth < 32
           AND instr(w.visited, printf('|%d|', e._row)) = 0
     ), ranked AS (
        SELECT *, row_number() OVER (
            PARTITION BY dst_path, dst_name ORDER BY depth, witness
        ) AS rank FROM walk
     )
     SELECT src_path, src_name, dst_path, dst_name, depth, witness
       FROM ranked WHERE rank = 1 ORDER BY depth, dst_path, dst_name";
    read_paths(connection, sql, (blob, start, end), "flow")
}

/// One shortest, edge-row-witnessed path per destination. The seed is bound;
/// table and column names come from the two fixed graph planes below.
fn paths(
    connection: &Connection,
    plane: &str,
    name: &str,
) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
    let (table, source_path, source_name, target_path, target_name) = match plane {
        "call" => ("resolved_edge", "caller_path", "caller_name", "callee_path", "callee_name"),
        "type" => ("resolved_type_edge", "owner_path", "owner_name", "target_path", "target_name"),
        _ => unreachable!("only fixed graph planes reach this query"),
    };
    let sql = format!(
        "WITH RECURSIVE walk(src_path, src_name, dst_path, dst_name, depth, witness, visited) AS (
            SELECT e.\"{source_path}\", e.\"{source_name}\", e.\"{target_path}\",
                   e.\"{target_name}\", 1, json_array(e.\"_row\"),
                   printf('|%d|', e.\"_row\")
              FROM \"{table}\" AS e WHERE e.\"{source_name}\" IS ?1
            UNION ALL
            SELECT w.src_path, w.src_name, e.\"{target_path}\", e.\"{target_name}\",
                   w.depth + 1, json_insert(w.witness, '$[#]', e.\"_row\"),
                   w.visited || e.\"_row\" || '|'
              FROM walk AS w JOIN \"{table}\" AS e
                ON e.\"{source_path}\" = w.dst_path
               AND e.\"{source_name}\" IS w.dst_name
             WHERE w.depth < 32
               AND instr(w.visited, printf('|%d|', e.\"_row\")) = 0
         ), ranked AS (
            SELECT *, row_number() OVER (
                PARTITION BY dst_path, dst_name ORDER BY depth, witness
            ) AS rank FROM walk
         )
         SELECT src_path, src_name, dst_path, dst_name, depth, witness
           FROM ranked WHERE rank = 1 ORDER BY depth, dst_path, dst_name"
    );
    read_paths(connection, &sql, [name], plane)
}

fn read_paths(
    connection: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
    plane: &str,
) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement
        .query_map(params, |row| {
            let witness: String = row.get(5)?;
            let witness = serde_json::from_str(&witness).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(FlatFact::GraphPath {
                plane: plane.to_string(),
                from_path: row.get(0)?,
                from_name: row.get(1)?,
                to_path: row.get(2)?,
                to_name: row.get(3)?,
                depth: row.get(4)?,
                witness,
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

fn emit_summary_line(rows: &[FlatFact], arm: &Arm<'_>, compared: bool) {
    if compared {
        let added = rows.iter().filter(|row| matches!(row, FlatFact::GraphPathChange { change, .. } if change == "added")).count();
        eprintln!("{} path changes: {} added, {} removed", rows.len(), added, rows.len() - added);
        return;
    }
    if matches!(arm, Arm::CallPath(_) | Arm::TypePath(_) | Arm::FlowPath(_)) {
        eprintln!("{} paths", rows.len());
        return;
    }
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

/// Which resolve arm each question needs, and which view answers it.
enum Arm<'a> {
    Callers(&'a str),
    Uses(&'a str),
    From(&'a str),
    CallPath(&'a str),
    TypePath(&'a str),
    FlowPath(&'a str),
}

impl Arm<'_> {
    fn arms(&self) -> ResolveArms {
        match self {
            Arm::Uses(_) | Arm::TypePath(_) => ResolveArms {
                types: true,
                ..ResolveArms::default()
            },
            Arm::FlowPath(_) => ResolveArms {
                call: true,
                flow: true,
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
            Arm::CallPath(name) => paths(connection, "call", name),
            Arm::TypePath(name) => paths(connection, "type", name),
            Arm::FlowPath(seed) => flow_paths(connection, seed),
        }
    }
}

fn ask_at(
    reader: &mut crate::revision::RevisionReader,
    revision: &str,
    selected: &[PathBuf],
    cli: &GraphArgs,
    arm: &Arm<'_>,
    sqlite: Option<&Path>,
) -> Result<(String, Vec<FlatFact>), Box<dyn std::error::Error>> {
    let (snapshot, rows) = reader.with_revision(
        revision,
        &crate::watch::default_patterns(),
        Some(selected),
        |paths, scratch| {
            let database = load_store(paths, arm.arms(), cli, Some(scratch), sqlite)?;
            let rows = arm.ask(database.connection())?;
            database.close()?;
            Ok(rows)
        },
    )?;
    Ok((snapshot.sha, rows))
}

type PathKey = (String, String, Option<String>, String, Option<String>, u32);

fn path_keys(rows: &[FlatFact]) -> BTreeSet<PathKey> {
    rows.iter()
        .filter_map(|row| match row {
            FlatFact::GraphPath {
                plane,
                from_path,
                from_name,
                to_path,
                to_name,
                depth,
                ..
            } => Some((
                plane.clone(),
                from_path.clone(),
                from_name.clone(),
                to_path.clone(),
                to_name.clone(),
                *depth,
            )),
            _ => None,
        })
        .collect()
}

fn changed_paths(
    before: (&str, &[FlatFact]),
    after: (&str, &[FlatFact]),
) -> Vec<FlatFact> {
    let left = path_keys(before.1);
    let right = path_keys(after.1);
    let change = |key: &PathKey, label: &str, revision: &str| FlatFact::GraphPathChange {
        change: label.to_string(),
        revision: revision.to_string(),
        plane: key.0.clone(),
        from_path: key.1.clone(),
        from_name: key.2.clone(),
        to_path: key.3.clone(),
        to_name: key.4.clone(),
        depth: key.5,
    };
    let mut out: Vec<FlatFact> = left
        .difference(&right)
        .map(|key| change(key, "removed", before.0))
        .collect();
    out.extend(
        right
            .difference(&left)
            .map(|key| change(key, "added", after.0)),
    );
    out
}

pub fn run(cli: GraphArgs) -> Result<(), Box<dyn std::error::Error>> {
    let arm = match (
        &cli.callers,
        &cli.uses,
        &cli.from,
        &cli.call_path,
        &cli.type_path,
        &cli.flow_path,
    ) {
        (Some(name), _, _, _, _, _) => Arm::Callers(name),
        (_, Some(name), _, _, _, _) => Arm::Uses(name),
        (_, _, Some(name), _, _, _) => Arm::From(name),
        (_, _, _, Some(name), _, _) => Arm::CallPath(name),
        (_, _, _, _, Some(name), _) => Arm::TypePath(name),
        (_, _, _, _, _, Some(name)) => Arm::FlowPath(name),
        _ => unreachable!("the clap ArgGroup requires one of the six"),
    };
    let rows = if let Some(revision) = &cli.at {
        let root = fs::canonicalize(
            cli.inputs.root.as_ref().expect("clap requires the root"),
        )?;
        let selected: Vec<PathBuf> = cli
            .inputs
            .paths
            .iter()
            .map(PathBuf::from)
            .map(|path| {
                if path.is_absolute() {
                    path.strip_prefix(&root).map(Path::to_path_buf).map_err(|_| {
                        format!("graph path {} is outside {}", path.display(), root.display())
                    })
                } else if path == Path::new(".") {
                    Ok(PathBuf::new())
                } else {
                    Ok(path)
                }
            })
            .collect::<Result<_, _>>()?;
        let mut reader = crate::revision::RevisionReader::open(&root)?;
        let (sha, before) = ask_at(&mut reader, revision, &selected, &cli, &arm, None)?;
        match &cli.compare {
            Some(other) => {
                if !matches!(arm, Arm::CallPath(_) | Arm::TypePath(_) | Arm::FlowPath(_)) {
                    return Err("--compare requires --call-path, --type-path or --flow-path".into());
                }
                let (other_sha, after) =
                    ask_at(&mut reader, other, &selected, &cli, &arm, None)?;
                changed_paths((&sha, &before), (&other_sha, &after))
            }
            None => before,
        }
    } else {
        let paths = crate::inputs::expand(&cli.inputs)?;
        let database = load_store(&paths, arm.arms(), &cli, None, cli.sqlite.as_deref())?;
        let rows = arm.ask(database.connection())?;
        database.close()?;
        rows
    };
    emit_rows(&rows)?;
    emit_summary_line(&rows, &arm, cli.compare.is_some());
    Ok(())
}
