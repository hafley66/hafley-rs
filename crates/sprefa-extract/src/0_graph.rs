//! `ryi graph`: one resolve pass landed in the SQLite fact store, then a
//! question asked of it: `--callers`/`--uses` as SQL over the views, the walks
//! (`--from`, `--*-path`) as one first-discovery pass over the plane's edges.
//! @comment-ok: module header, the seam list every bin arm opens with

use crate::cli::GraphArgs;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rusqlite::Connection;
#[cfg(feature = "graph")]
use sprefa_extract::cfg_facts;
use sprefa_extract::{
    newline_offsets, resolve_project_with_raw_tsi, resolve_project_with_tsi_tiers,
    slow_project, slow_project_with_raw, FamilyTag, FlatFact, ResolveArms,
    ResolveRequest, ScipMode, ScipRecords,
};

use crate::sqlite::{grade_sql, line_col, Database, REACH_DEPTH_CAP};
#[path = "0a_graph_target.rs"]
mod target;

const CALLERS_SQL: &str = "SELECT caller_path, caller_name, callee_path, callee_name, \
                         grade, kind, caller_site_start, callee_start FROM ( \
                         SELECT e.*, ROW_NUMBER() OVER ( \
                         PARTITION BY caller_path, caller_site_start, caller_site_end, \
                                      callee_path, callee_start, callee_end \
                         ORDER BY resolution_origin IN ('checker', 'scip') DESC, \
                                  caller_name LIKE 'closure@%' DESC, \
                                  caller_name, kind, resolution_origin) AS site_rank, \
                         CASE WHEN resolution_origin IN ('module_plane', 'checker', 'scip') \
                              THEN '+' WHEN resolution_origin = 'unresolved' \
                              THEN '-' ELSE '~' END AS grade \
                         FROM resolved_edge AS e WHERE callee_name IS ?1) \
                         WHERE site_rank = 1";

const USES_SQL: &str = "SELECT \"user_path\", \"user_name\", \"type_path\", \"type_name\", \
                        \"grade\", \"kind\", \"user_start\", NULL FROM \"uses\" \
                        WHERE \"type_name\" IS ?1";

const EXTERNAL_USES_SQL: &str = "SELECT \"from_path\", \"from_name\", \"type_name\", \"crate_name\", \"reason\", \"kind\" FROM \"external_crate_decline\" WHERE \"type_name\" IS ?1";

/// One resolve pass, landed in the store the views read. `--sqlite` publishes
/// the store; without it the whole thing lives and dies in memory. `--slow`
/// verifies target sites with the language checker.
fn load_store(
    paths: &[PathBuf],
    arm: &Arm<'_>,
    cli: &GraphArgs,
    revision_root: Option<&Path>,
    sqlite: Option<&Path>,
) -> Result<Database, Box<dyn std::error::Error>> {
    let root = revision_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| crate::inputs::root(&cli.inputs));
    let request = ResolveRequest {
        paths,
        arms: arm.arms(),
        scip: ScipMode::from_flags(cli.scip_index.as_deref(), false),
        project_root: revision_root.or(cli.inputs.root.as_deref()),
        scip_records: ScipRecords::default(),
        occurrence_text: false,
        rust_checker: (!cli.slow && cli.rust_checker).then_some(root.as_path()),
        ts_checker: (!cli.slow && cli.ts_checker).then_some(root.as_path()),
        go_checker: (!cli.slow && cli.go_checker).then_some(root.as_path()),
        witness: true,
    };
    let mut database = match sqlite {
        Some(path) => Database::create(path)?,
        None => Database::memory()?,
    };
    let facts = if matches!(arm, Arm::FlowPath(_)) {
        let mut push_raw = |raw: sprefa_extract::RawProjectFact<'_>| {
            if matches!(
                &raw.fact,
                FlatFact::FileRow { .. }
                    | FlatFact::Node { family: FamilyTag::Df, .. }
                    | FlatFact::Edge { family: FamilyTag::Df, .. }
            ) {
                database
                    .source(raw.path, raw.content_id.to_string())
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
                database
                    .bind_row(&raw.fact)
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
            }
            Ok::<(), std::io::Error>(())
        };
        let facts = if cli.slow {
            slow_project_with_raw(
                paths,
                &root,
                cli.scip_index.as_deref(),
                cli.scip_index.is_none(),
                &mut push_raw,
            )?
        } else {
            resolve_project_with_raw_tsi(&request, &mut push_raw)?
        };
        database.clear_source()?;
        facts
    } else if cli.slow {
        if let Some(index) = cli.scip_index.as_deref() {
            slow_project(paths, &root, Some(index), false)?
        } else if matches!(arm, Arm::Callers(_) | Arm::Uses(_)) {
            target::facts(&request, &root, arm.name())?
        } else {
            slow_project(paths, &root, None, true)?
        }
    } else {
        resolve_project_with_tsi_tiers(&request)?
    };
    let _store_span = tracing::info_span!("store.write").entered();
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
                | FlatFact::ExternalCrateDecline { .. }
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
            self.tables.insert(
                path.to_string(),
                content.map(|bytes| newline_offsets(&bytes)),
            );
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
        "flow" => {
            "SELECT \"_row\", \"from_blob\", printf('%d:%d', \"from__start\", \"from__end\"), \
                   \"to_blob\", printf('%d:%d', \"to__start\", \"to__end\"), '~', NULL \
                   FROM \"flow_edge\" \
             UNION ALL \
             SELECT \"_row\", \"_content_id\", printf('%d:%d', \"from__start\", \"from__end\"), \
                    \"_content_id\", printf('%d:%d', \"to__start\", \"to__end\"), '~', NULL \
                    FROM \"edge\" WHERE \"family\" = 'df' AND \"_content_id\" IS NOT NULL"
                .to_string()
        }
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
                if level
                    .get(&edge.dst)
                    .is_some_and(|best| best.witness <= path)
                {
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
            .map(|found| {
                (
                    found.node.clone(),
                    found.witness.clone(),
                    found.origin.clone(),
                )
            })
            .collect();
        found.extend(level);
    }
    let mut rows: Vec<Found> = found.into_values().collect();
    rows.sort_by(|left, right| (left.depth, &left.node).cmp(&(right.depth, &right.node)));
    Ok(rows)
}

/// Every source node whose name is NAME: the walk's seed set. `PATH#NAME`
/// keeps the ones declared in a file whose path ends with PATH (whole
/// components); bare NAME keeps all of them.
fn named_starts(edges: &[PlaneEdge], anchor: &str) -> BTreeSet<Node> {
    let (path, name) = match anchor.split_once('#') {
        Some((path, name)) => (Some(Path::new(path)), name),
        None => (None, anchor),
    };
    edges
        .iter()
        .filter(|edge| edge.src.1.as_deref() == Some(name))
        .filter(|edge| path.is_none_or(|path| Path::new(&edge.src.0).ends_with(path)))
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

/// Normalize a path to the content identity used by flow facts. Revision
/// queries read their scratch tree; digest seeds already name that identity.
fn flow_seed(seed: &str, root: Option<&Path>) -> Result<BTreeSet<Node>, Box<dyn std::error::Error>> {
    let (blob, span) = seed
        .rsplit_once('@')
        .ok_or("flow seed must be PATH@START:END or BLOB@START:END")?;
    let (start, end) = span
        .split_once(':')
        .ok_or("flow seed must be PATH@START:END or BLOB@START:END")?;
    let start: u32 = start.parse()?;
    let end: u32 = end.parse()?;
    if start >= end {
        return Err("flow seed requires START < END (zero-based byte offsets, END exclusive)".into());
    }
    let digest = blob.strip_prefix("blake3:").filter(|hex| {
        hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
    }).or_else(|| blob.strip_prefix("git:").filter(|hex| {
        hex.len() == 40 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
    }));
    let blob = if digest.is_some() {
        blob.to_ascii_lowercase()
    } else {
        let path = Path::new(blob);
        let path = match root {
            Some(root) if !path.is_absolute() => root.join(path),
            _ => path.to_path_buf(),
        };
        let mut bytes = Vec::new();
        fs::File::open(sprefa_extract::io_path(&path))
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|error| format!("flow seed input '{blob}': {error}"))?;
        if end as usize > bytes.len() {
            return Err(format!("flow seed END {end} exceeds input length {}", bytes.len()).into());
        }
        sprefa_extract::content_id_of(&bytes).to_string()
    };
    Ok(BTreeSet::from([(
        blob,
        Some(format!("{start}:{end}")),
    )]))
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

fn emit_rows(
    rows: &[FlatFact],
    output: &mut dyn std::io::Write,
) -> Result<(), Box<dyn std::error::Error>> {
    for row in rows {
        writeln!(output, "{}", serde_json::to_string(row)?)?;
    }
    Ok(())
}

fn emit_summary_line(rows: &[FlatFact], arm: &Arm<'_>, compared: bool) {
    if compared {
        let added = rows
            .iter()
            .filter(
                |row| matches!(row, FlatFact::GraphPathChange { change, .. } if change == "added"),
            )
            .count();
        crate::ops::print_diagnostic(format_args!(
            "{} path changes: {} added, {} removed",
            rows.len(),
            added,
            rows.len() - added
        ));
        return;
    }
    if matches!(arm, Arm::CallPath(_) | Arm::TypePath(_) | Arm::FlowPath(_)) {
        crate::ops::print_diagnostic(format_args!("{} paths", rows.len()));
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
    crate::ops::print_diagnostic(format_args!(
        "{} edges: {} +, {} ~, {} -",
        rows.len(),
        split.plus,
        split.tilde,
        split.minus
    ));
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
    fn name(&self) -> &str {
        match self {
            Arm::Callers(name) | Arm::From(name) | Arm::CallPath(name) | Arm::TypePath(name) => {
                name.rsplit_once('#').map_or(*name, |(_, name)| name)
            }
            Arm::Uses(name) | Arm::FlowPath(name) => name,
        }
    }

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
            Arm::Callers(anchor) => {
                let (path, name) = match anchor.split_once('#') {
                    Some((path, name)) => (Some(path), name),
                    None => (None, *anchor),
                };
                let mut rows = edges(connection, CALLERS_SQL, name, lines)?;
                if let Some(path) = path {
                    let target = fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
                    rows.retain(|row| {
                        let FlatFact::GraphEdge { to_path, .. } = row else {
                            return false;
                        };
                        let candidate = fs::canonicalize(to_path)
                            .unwrap_or_else(|_| PathBuf::from(to_path));
                        candidate == target
                    });
                }
                Ok(rows)
            }
            Arm::Uses(name) => {
                let mut rows = edges(connection, USES_SQL, name, lines)?;
                let table_exists: bool = connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'external_crate_decline')",
                    [],
                    |row| row.get(0),
                )?;
                if table_exists {
                    let mut statement = connection.prepare(EXTERNAL_USES_SQL)?;
                    let declines = statement
                        .query_map([name], |row| {
                            Ok(FlatFact::GraphDecline {
                                from_path: row.get(0)?,
                                from_name: row.get(1)?,
                                type_name: row.get(2)?,
                                crate_name: row.get(3)?,
                                reason: row.get(4)?,
                                kind: row.get(5)?,
                            })
                        })?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    rows.extend(declines);
                }
                rows.sort_by_key(|row| serde_json::to_string(row).expect("graph row serializes"));
                Ok(rows)
            }
            Arm::From(name) => nodes(connection, name, deadline, lines),
            Arm::CallPath(name) => paths(
                connection,
                "call",
                |edges| Ok(named_starts(edges, name)),
                deadline,
            ),
            Arm::TypePath(name) => paths(
                connection,
                "type",
                |edges| Ok(named_starts(edges, name)),
                deadline,
            ),
            Arm::FlowPath(seed) => paths(connection, "flow", |_| flow_seed(seed, lines.root.as_deref()), deadline),
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
                Err(crate::RyiExit::new(3, format!("graph: query exceeded {secs}s")).into())
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
            let database = load_store(paths, arm, cli, Some(scratch), sqlite)?;
            let rows = arm.ask_within(
                database.connection(),
                cli.timeout,
                Some(scratch.to_path_buf()),
            )?;
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

fn changed_paths(before: (&str, &[FlatFact]), after: (&str, &[FlatFact])) -> Vec<FlatFact> {
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
    run_to(cli, &mut std::io::stdout().lock())
}

pub fn run_to(
    cli: GraphArgs,
    output: &mut dyn std::io::Write,
) -> Result<(), Box<dyn std::error::Error>> {
    for (flag, anchor) in [
        ("--callers", &cli.callers),
        ("--from", &cli.from),
        ("--call-path", &cli.call_path),
        ("--type-path", &cli.type_path),
    ] {
        if let Some((path, name)) = anchor.as_deref().and_then(|anchor| anchor.split_once('#')) {
            if path.is_empty() || name.is_empty() || name.contains('#') {
                return Err(format!("{flag} requires NAME or FILE#NAME").into());
            }
        }
    }
    if let Some(anchor) = cli.callers.as_deref() {
        match anchor.split_once('#') {
            None if anchor.contains('.') => {
                return Err("--callers Class.method is unsupported; use FILE#method".into());
            }
            _ => {}
        }
    }
    #[cfg(feature = "graph")]
    if let Some(seed) = cli.slice.as_deref() {
        emit_rows(&slice_at(seed)?, output)?;
        return Ok(());
    }
    #[cfg(not(feature = "graph"))]
    if cli.slice.is_some() {
        return Err("--slice requires the graph feature".into());
    }
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
        let root = fs::canonicalize(sprefa_extract::io_path(
            cli.inputs.root.as_ref().expect("clap requires the root"),
        ))?;
        let selected: Vec<PathBuf> = cli
            .inputs
            .paths
            .iter()
            .map(PathBuf::from)
            .map(|path| {
                if path.is_absolute() {
                    path.strip_prefix(&root)
                        .map(Path::to_path_buf)
                        .map_err(|_| {
                            format!(
                                "graph path {} is outside {}",
                                path.display(),
                                root.display()
                            )
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
                let (other_sha, after) = ask_at(&mut reader, other, &selected, &cli, &arm, None)?;
                changed_paths((&sha, &before), (&other_sha, &after))
            }
            None => before,
        }
    } else {
        let paths = crate::inputs::expand(&cli.inputs)?;
        let database = load_store(&paths, &arm, &cli, None, cli.sqlite.as_deref())?;
        let rows = arm.ask_within(database.connection(), cli.timeout, cli.inputs.root.clone())?;
        database.close()?;
        rows
    };
    emit_rows(&rows, output)?;
    emit_summary_line(&rows, &arm, cli.compare.is_some());
    Ok(())
}

#[cfg(feature = "graph")]
type CfgNodeKey = (u32, u32, String);

/// Return the backward control-dependence closure of the CFG node covering a
/// source byte. Output rows reuse the existing cfg_node record shape.
#[cfg(feature = "graph")]
fn slice_at(seed: &str) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
    let (path, byte) = seed.rsplit_once(':').ok_or("--slice expects PATH:BYTE")?;
    let byte: u32 = byte.parse()?;
    let content = fs::read(sprefa_extract::io_path(Path::new(path)))?;
    let facts = cfg_facts(path, &content);
    let key = |span: sprefa_extract::SpanOut, kind: Option<&str>| {
        (span.start, span.end, kind.unwrap_or_default().to_string())
    };
    let mut nodes = BTreeMap::<CfgNodeKey, FlatFact>::new();
    let mut controls = Vec::<(CfgNodeKey, CfgNodeKey)>::new();
    for fact in facts {
        match fact {
            FlatFact::Node {
                family: FamilyTag::Cfg,
                span,
                kind,
                name,
                ..
            } => {
                nodes.insert(
                    (span.start, span.end, kind.clone()),
                    FlatFact::Node {
                        fact: None,
                        family: FamilyTag::Cfg,
                        span,
                        kind,
                        name,
                        named: None,
                    },
                );
            }
            FlatFact::Edge {
                family: FamilyTag::Cfg,
                kind,
                from,
                from_kind,
                to,
                to_kind,
                ..
            } if kind == "control" => {
                controls.push((key(from, from_kind.as_deref()), key(to, to_kind.as_deref())));
            }
            _ => {}
        }
    }
    let seed_node = nodes
        .keys()
        .filter(|(start, end, kind)| {
            *start <= byte && byte < *end && !matches!(kind.as_str(), "entry" | "exit")
        })
        .min_by_key(|(start, end, _)| end - start)
        .cloned()
        .ok_or_else(|| format!("--slice byte {byte} is outside a CFG statement in {path}"))?;

    let mut incoming = HashMap::<CfgNodeKey, Vec<CfgNodeKey>>::new();
    for (controller, dependent) in controls {
        incoming.entry(dependent).or_default().push(controller);
    }
    let mut selected = BTreeSet::from([seed_node.clone()]);
    let mut pending = vec![seed_node];
    while let Some(dependent) = pending.pop() {
        for controller in incoming.get(&dependent).into_iter().flatten() {
            if selected.insert(controller.clone()) {
                pending.push(controller.clone());
            }
        }
    }
    Ok(selected
        .into_iter()
        .filter_map(|node| nodes.remove(&node))
        .collect())
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

    #[test]
    fn flow_paths_join_local_and_interprocedural_edges_with_stored_witnesses() {
        let mut database = Database::memory().unwrap();
        for (path, blob, family, from, to) in [
            ("a.ts", "blake3:a", "df", 1, 3),
            ("a.ts", "blake3:a", "call", 1, 9),
            ("b.ts", "blake3:b", "df", 5, 7),
        ] {
            database.source(path, blob.to_string()).unwrap();
            database.insert(serde_json::json!({
                "record": "edge", "family": family, "kind": "use",
                "from": {"start": from, "end": from + 1},
                "to": {"start": to, "end": to + 1}
            })).unwrap();
        }
        database.clear_source().unwrap();
        database.insert(serde_json::json!({
            "record": "flow_edge", "family": "flow", "kind": "arg_to_param",
            "from_blob": "blake3:a", "from": {"start": 3, "end": 4},
            "to_blob": "blake3:b", "to": {"start": 5, "end": 6}
        })).unwrap();
        database.flush().unwrap();
        let rows = paths(
            database.connection(),
            "flow",
            |_| Ok(BTreeSet::from([("blake3:a".to_string(), Some("1:2".to_string()))])),
            &Deadline { at: Instant::now() + Duration::from_secs(60) },
        ).unwrap();
        assert_eq!(
            serde_json::to_value(rows).unwrap(),
            serde_json::json!([
                {"record": "graph_path", "plane": "flow", "from_path": "blake3:a", "from_name": "1:2",
                 "to_path": "blake3:a", "to_name": "3:4", "depth": 1, "witness": [1]},
                {"record": "graph_path", "plane": "flow", "from_path": "blake3:a", "from_name": "1:2",
                 "to_path": "blake3:b", "to_name": "5:6", "depth": 2, "witness": [1, 4]},
                {"record": "graph_path", "plane": "flow", "from_path": "blake3:a", "from_name": "1:2",
                 "to_path": "blake3:b", "to_name": "7:8", "depth": 3, "witness": [1, 4, 3]}
            ])
        );
    }
}
