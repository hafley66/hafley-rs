//! `ryi graph`: one resolve pass landed in the SQLite fact store, then a
//! question asked of it: `--callers`/`--uses` as SQL over the views, the walks
//! (`--from`, `--*-path`) as one first-discovery pass over the plane's edges.
//! @comment-ok: module header, the seam list every bin arm opens with

use crate::cli::GraphArgs;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rusqlite::Connection;
use sprefa_extract::{
    newline_offsets, resolve_project_with_tsi_tiers, slow_project, FlatFact, ResolveArms,
    ResolveRequest, ScipMode, ScipRecords,
};

use crate::sqlite::{grade_sql, line_col, Database, REACH_DEPTH_CAP};

const CALLERS_SQL: &str = "SELECT \"caller_path\", \"caller_name\", \"callee_path\", \
                           \"callee_name\", \"grade\", \"kind\", \"caller_site_start\", \
                           \"callee_start\" FROM \"callers\" WHERE \"callee_name\" IS ?1";

const USES_SQL: &str = "SELECT \"user_path\", \"user_name\", \"type_path\", \"type_name\", \
                        \"grade\", \"kind\", \"user_start\", NULL FROM \"uses\" \
                        WHERE \"type_name\" IS ?1";

/// One resolve pass, landed in the store the views read. `--sqlite` publishes
/// the store; without it the whole thing lives and dies in memory. `--slow`
/// lands the SCIP oracle's projection of the same tables instead.
fn load_store(
    paths: &[PathBuf],
    arms: ResolveArms,
    cli: &GraphArgs,
    revision_root: Option<&Path>,
    sqlite: Option<&Path>,
) -> Result<Database, Box<dyn std::error::Error>> {
    let root = revision_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| crate::inputs::root(&cli.inputs));
    let facts = if cli.slow {
        slow_project(paths, &root, cli.scip_index.as_deref(), true)?
    } else {
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
        resolve_project_with_tsi_tiers(&request)?
    };
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

/// The question's wall budget. SQLite statements stop through the
/// connection's interrupt handle; the Rust walk polls `expired`.
struct Deadline {
    at: Instant,
}

impl Deadline {
    fn expired(&self) -> bool {
        Instant::now() >= self.at
    }
}

/// Newline offsets per file, read once. A file that does not read maps to
/// `None`, so its rows keep `line: null` and it costs one probe.
struct Lines {
    root: Option<PathBuf>,
    tables: HashMap<String, Option<Vec<u32>>>,
}

impl Lines {
    fn line(&mut self, path: &str, byte: Option<u32>) -> Option<u32> {
        let byte = byte?;
        if !self.tables.contains_key(path) {
            let content = fs::read(path).ok().or_else(|| {
                self.root
                    .as_ref()
                    .and_then(|root| fs::read(root.join(path)).ok())
            });
            self.tables
                .insert(path.to_string(), content.map(|bytes| newline_offsets(&bytes)));
        }
        self.tables[path]
            .as_ref()
            .map(|offsets| line_col(offsets, byte).0)
    }
}

/// `callers` and `uses` project the same eight columns in the same order:
/// source, target, grade, kind, source byte, target byte. NAME travels as `?1`.
fn edges(
    connection: &Connection,
    sql: &str,
    name: &str,
    lines: &mut Lines,
) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(sql)?;
    let raw = statement
        .query_map([name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<u32>>(6)?,
                row.get::<_, Option<u32>>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut rows: Vec<FlatFact> = raw
        .into_iter()
        .map(
            |(from_path, from_name, to_path, to_name, grade, kind, from_byte, to_byte)| {
                FlatFact::GraphEdge {
                    from_line: lines.line(&from_path, from_byte),
                    to_line: lines.line(&to_path, to_byte),
                    from_path,
                    from_name,
                    to_path,
                    to_name,
                    kind,
                    grade,
                }
            },
        )
        .collect();
    rows.sort_by_key(|edge| serde_json::to_string(edge).expect("graph edge serializes"));
    Ok(rows)
}

/// A graph node: a file and the name declared there (`None` for a site with
/// no enclosing name).
type Node = (String, Option<String>);

/// One edge of a walked plane: its export row, both ends, the grade a
/// discovery through it reports, and the byte its target is declared at.
struct PlaneEdge {
    row: u64,
    src: Node,
    dst: Node,
    grade: String,
    dst_start: Option<u32>,
}

/// The first time the walk reached `node`: at the least depth, and among
/// equal depths along the lexicographically least row-id witness.
struct Found {
    origin: Node,
    node: Node,
    depth: u32,
    witness: Vec<u64>,
    via: usize,
}

/// Every edge of one plane, as `(row, src_path, src_name, dst_path, dst_name,
/// grade, dst_start)`.
fn plane_edges(
    connection: &Connection,
    plane: &str,
) -> Result<Vec<PlaneEdge>, Box<dyn std::error::Error>> {
    let sql = match plane {
        "call" => format!(
            "SELECT \"_row\", \"caller_path\", \"caller_name\", \"callee_path\", \
             \"callee_name\", {}, \"callee_start\" FROM \"resolved_edge\"",
            grade_sql("\"resolution_origin\"")
        ),
        "type" => format!(
            "SELECT \"_row\", \"owner_path\", \"owner_name\", \"target_path\", \
             \"target_name\", {}, NULL FROM \"resolved_type_edge\"",
            grade_sql("\"resolution_origin\"")
        ),
        "flow" => "SELECT \"_row\", \"from_blob\", printf('%d:%d', \"from__start\", \"from__end\"), \
                   \"to_blob\", printf('%d:%d', \"to__start\", \"to__end\"), '~', NULL \
                   FROM \"flow_edge\""
            .to_string(),
        _ => unreachable!("only fixed graph planes reach this query"),
    };
    let mut statement = connection.prepare(&sql)?;
    let rows = statement
        .query_map([], |row| {
            Ok(PlaneEdge {
                row: row.get::<_, i64>(0)? as u64,
                src: (row.get(1)?, row.get(2)?),
                dst: (row.get(3)?, row.get(4)?),
                grade: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                dst_start: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Level-by-level walk from `starts`: each node is kept the first time it is
/// reached, so the work is bounded by the edges, never by the paths.
fn first_discovery(
    edges: &[PlaneEdge],
    starts: BTreeSet<Node>,
    deadline: &Deadline,
) -> Result<Vec<Found>, Box<dyn std::error::Error>> {
    let mut out_of: HashMap<&Node, Vec<usize>> = HashMap::new();
    for (index, edge) in edges.iter().enumerate() {
        out_of.entry(&edge.src).or_default().push(index);
    }
    let mut found: HashMap<Node, Found> = HashMap::new();
    let mut expanded: HashSet<Node> = HashSet::new();
    let mut frontier: Vec<(Node, Vec<u64>, Node)> = starts
        .into_iter()
        .map(|node| (node.clone(), Vec::new(), node))
        .collect();
    for depth in 1..=REACH_DEPTH_CAP {
        let mut level: BTreeMap<Node, Found> = BTreeMap::new();
        for (node, witness, origin) in &frontier {
            if deadline.expired() {
                return Err("graph walk passed its deadline".into());
            }
            if !expanded.insert(node.clone()) {
                continue;
            }
            for &index in out_of.get(node).into_iter().flatten() {
                let edge = &edges[index];
                if found.contains_key(&edge.dst) {
                    continue;
                }
                let mut path = witness.clone();
                path.push(edge.row);
                if level.get(&edge.dst).is_some_and(|best| best.witness <= path) {
                    continue;
                }
                level.insert(
                    edge.dst.clone(),
                    Found {
                        origin: origin.clone(),
                        node: edge.dst.clone(),
                        depth,
                        witness: path,
                        via: index,
                    },
                );
            }
        }
        if level.is_empty() {
            break;
        }
        frontier = level
            .values()
            .map(|found| (found.node.clone(), found.witness.clone(), found.origin.clone()))
            .collect();
        found.extend(level);
    }
    let mut rows: Vec<Found> = found.into_values().collect();
    rows.sort_by(|left, right| (left.depth, &left.node).cmp(&(right.depth, &right.node)));
    Ok(rows)
}

/// Every source node whose name is NAME: the walk's seed set.
fn named_starts(edges: &[PlaneEdge], name: &str) -> BTreeSet<Node> {
    edges
        .iter()
        .filter(|edge| edge.src.1.as_deref() == Some(name))
        .map(|edge| edge.src.clone())
        .collect()
}

/// The reach closure seeded at NAME: one row per node it reaches, at the
/// shortest depth, graded by the edge that discovered it there.
fn nodes(
    connection: &Connection,
    name: &str,
    deadline: &Deadline,
    lines: &mut Lines,
) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
    let edges = plane_edges(connection, "call")?;
    let found = first_discovery(&edges, named_starts(&edges, name), deadline)?;
    Ok(found
        .into_iter()
        .map(|found| {
            let edge = &edges[found.via];
            FlatFact::GraphNode {
                line: lines.line(&found.node.0, edge.dst_start),
                path: found.node.0,
                name: found.node.1,
                depth: found.depth,
                grade: edge.grade.clone(),
            }
        })
        .collect())
}

/// One shortest, edge-row-witnessed path per destination on `plane`.
fn paths(
    connection: &Connection,
    plane: &str,
    starts: impl FnOnce(&[PlaneEdge]) -> Result<BTreeSet<Node>, Box<dyn std::error::Error>>,
    deadline: &Deadline,
) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
    let edges = plane_edges(connection, plane)?;
    let found = first_discovery(&edges, starts(&edges)?, deadline)?;
    Ok(found
        .into_iter()
        .map(|found| FlatFact::GraphPath {
            plane: plane.to_string(),
            from_path: found.origin.0,
            from_name: found.origin.1,
            to_path: found.node.0,
            to_name: found.node.1,
            depth: found.depth,
            witness: found.witness,
        })
        .collect())
}

/// Flow identity is a content digest and byte span. The seed uses the final
/// @ to separate the digest from START:END.
fn flow_seed(seed: &str) -> Result<BTreeSet<Node>, Box<dyn std::error::Error>> {
    let (blob, span) = seed.rsplit_once('@').ok_or("flow seed must be BLOB@START:END")?;
    let (start, end) = span.split_once(':').ok_or("flow seed must be BLOB@START:END")?;
    let start: u32 = start.parse()?;
    let end: u32 = end.parse()?;
    Ok(BTreeSet::from([(blob.to_string(), Some(format!("{start}:{end}")))]))
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

    fn ask(
        &self,
        connection: &Connection,
        deadline: &Deadline,
        lines: &mut Lines,
    ) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
        match self {
            Arm::Callers(name) => edges(connection, CALLERS_SQL, name, lines),
            Arm::Uses(name) => edges(connection, USES_SQL, name, lines),
            Arm::From(name) => nodes(connection, name, deadline, lines),
            Arm::CallPath(name) => {
                paths(connection, "call", |edges| Ok(named_starts(edges, name)), deadline)
            }
            Arm::TypePath(name) => {
                paths(connection, "type", |edges| Ok(named_starts(edges, name)), deadline)
            }
            Arm::FlowPath(seed) => paths(connection, "flow", |_| flow_seed(seed), deadline),
        }
    }

    /// `ask` under `--timeout`: a timer thread interrupts SQLite at the
    /// deadline, and an answer that ran past it exits 3.
    fn ask_within(
        &self,
        connection: &Connection,
        secs: u64,
        root: Option<PathBuf>,
    ) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
        let budget = Duration::from_secs(secs);
        let deadline = Deadline {
            at: Instant::now() + budget,
        };
        let handle = connection.get_interrupt_handle();
        let (done, wait) = mpsc::channel::<()>();
        let timer = std::thread::spawn(move || {
            if let Err(mpsc::RecvTimeoutError::Timeout) = wait.recv_timeout(budget) {
                handle.interrupt();
            }
        });
        let mut lines = Lines {
            root,
            tables: HashMap::new(),
        };
        let answer = self.ask(connection, &deadline, &mut lines);
        let _ = done.send(());
        let _ = timer.join();
        match answer {
            Err(_) if deadline.expired() => {
                // @eprintln-ok: CLI-UX stop, off the fact stream, exit 3.
                eprintln!("graph: query exceeded {secs}s");
                crate::exit(3);
            }
            answer => answer,
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
            let rows = arm.ask_within(database.connection(), cli.timeout, Some(scratch.to_path_buf()))?;
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
        let rows = arm.ask_within(database.connection(), cli.timeout, cli.inputs.root.clone())?;
        database.close()?;
        rows
    };
    emit_rows(&rows)?;
    emit_summary_line(&rows, &arm, cli.compare.is_some());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str) -> Node {
        ("f.ts".to_string(), Some(name.to_string()))
    }

    fn edge(row: u64, src: &str, dst: &str) -> PlaneEdge {
        PlaneEdge {
            row,
            src: node(src),
            dst: node(dst),
            grade: "~".to_string(),
            dst_start: None,
        }
    }

    fn walk(edges: &[PlaneEdge], seed: &str) -> Vec<(String, u32, Vec<u64>)> {
        let open = Deadline {
            at: Instant::now() + Duration::from_secs(60),
        };
        first_discovery(edges, named_starts(edges, seed), &open)
            .unwrap()
            .into_iter()
            .map(|found| (found.node.1.unwrap(), found.depth, found.witness))
            .collect()
    }

    #[test]
    fn a_dense_cyclic_plane_keeps_one_row_per_node_at_its_least_depth() {
        // Every shortcut of a three-node cycle: the old path enumeration grew
        // with the paths, this walk answers each node once.
        let edges = [
            edge(1, "a", "b"),
            edge(2, "b", "c"),
            edge(3, "c", "a"),
            edge(4, "a", "c"),
            edge(5, "c", "b"),
            edge(6, "b", "a"),
        ];
        assert_eq!(
            walk(&edges, "a"),
            [
                ("b".to_string(), 1, vec![1]),
                ("c".to_string(), 1, vec![4]),
                ("a".to_string(), 2, vec![1, 6]),
            ]
        );
    }

    #[test]
    fn equal_depth_routes_keep_the_least_row_witness() {
        let edges = [
            edge(9, "a", "x"),
            edge(2, "a", "y"),
            edge(7, "x", "z"),
            edge(3, "y", "z"),
        ];
        assert_eq!(
            walk(&edges, "a"),
            [
                ("x".to_string(), 1, vec![9]),
                ("y".to_string(), 1, vec![2]),
                ("z".to_string(), 2, vec![2, 3]),
            ]
        );
    }

    #[test]
    fn a_passed_deadline_stops_the_walk() {
        let edges = [edge(1, "a", "b")];
        let past = Deadline { at: Instant::now() };
        assert!(first_discovery(&edges, named_starts(&edges, "a"), &past).is_err());
    }
}
