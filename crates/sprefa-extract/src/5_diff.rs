//! `extract diff`: the fact delta between two commits, one shot. Both sides are
//! snapshotted through soopy, every blob is read through `git cat-file` (never a
//! checkout), and the resolved rows are set-differenced on span-free keys.

use crate::cli::DiffArgs;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::Serialize;

use sprefa_extract::{
    resolve_project, FlatFact, ResolveArms, ResolveRequest, ScipMode, ScipRecords,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Span {
    start: u32,
    end: u32,
}

struct Options {
    root: PathBuf,
    from: String,
    to: String,
    patterns: Vec<soopy::Pattern>,
    arms: ResolveArms,
    sqlite: Option<PathBuf>,
}

/// One revision's snapshot: the full sha, the path-to-blob-oid map, and the
/// facts the fast path resolved over every file at that revision.
struct Side {
    sha: String,
    files: BTreeMap<String, String>,
    facts: Vec<FlatFact>,
}

pub fn run(args: DiffArgs) -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::from_args(args)?;
    let mut reader = crate::revision::RevisionReader::open(&options.root)?;
    let a = resolve_at(&mut reader, &options, &options.from)?;
    let b = resolve_at(&mut reader, &options, &options.to)?;

    let mut counts = Counts::default();
    let mut rows = file_rows(&a, &b, &mut counts);
    rows.extend(edge_rows(&a, &b, &mut counts));
    rows.extend(type_edge_rows(&a, &b, &mut counts));
    rows.extend(import_rows(&a, &b, &mut counts));
    rows.extend(unresolved_rows(&a, &b, &mut counts));

    // The header counts every relation, so it is built last and sorted first.
    let mut sortable: Vec<(u8, String, String, DiffRow)> = vec![(
        0,
        String::new(),
        String::new(),
        DiffRow::Run(run_row(&a, &b, &counts)),
    )];
    sortable.extend(
        rows.into_iter()
            .map(|row| (row.rank(), row.sort_path().to_string(), row.sort_key(), row)),
    );
    sortable.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });

    match &options.sqlite {
        Some(path) => write_sqlite(path, &sortable),
        None => write_jsonl(&sortable),
    }
}

fn resolve_at(
    reader: &mut crate::revision::RevisionReader,
    options: &Options,
    revision: &str,
) -> Result<Side, Box<dyn std::error::Error>> {
    let (snapshot, facts) = reader.with_revision(
        revision,
        &options.patterns,
        None,
        |paths, _| Ok(resolve_project(&resolve_request(paths, options))?),
    )?;
    Ok(Side {
        sha: snapshot.sha,
        files: snapshot.files,
        facts,
    })
}

fn resolve_request<'a>(paths: &'a [PathBuf], options: &Options) -> ResolveRequest<'a> {
    ResolveRequest {
        paths,
        arms: options.arms,
        scip: ScipMode::Off,
        project_root: None,
        scip_records: ScipRecords::all(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        go_checker: None,
        witness: false,
    }
}

impl Options {
    fn from_args(args: DiffArgs) -> Result<Self, Box<dyn std::error::Error>> {
        let patterns = if args.patterns.is_empty() {
            crate::watch::default_patterns()
        } else {
            args.patterns.into_iter().map(|glob| soopy::Pattern(glob.into())).collect()
        };
        Ok(Options {
            root: args.root.map_or_else(crate::inputs::git_root_of_cwd, Ok)?,
            from: args.from,
            to: args.to,
            patterns,
            arms: parse_arms(&args.arms)?,
            sqlite: args.sqlite,
        })
    }
}

fn parse_arms(families: &[String]) -> Result<ResolveArms, Box<dyn std::error::Error>> {
    if families.is_empty() {
        return Ok(ResolveArms {
            call: true,
            types: true,
            flow: false,
        });
    }
    let mut arms = ResolveArms {
        call: false,
        types: false,
        flow: false,
    };
    for family in families {
        match family.as_str() {
            "call" => arms.call = true,
            "type" | "types" => arms.types = true,
            unknown => {
                return Err(
                    format!("--arms {unknown}: use call or type").into(),
                )
            }
        }
    }
    Ok(arms)
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
struct EdgeRow {
    record: &'static str,
    relation: &'static str,
    change: &'static str,
    caller_path: String,
    caller_name: Option<String>,
    callee_path: String,
    callee_name: Option<String>,
    kind: String,
    from_origin: Option<String>,
    to_origin: Option<String>,
    span: Option<SpanJson>,
}

#[derive(Serialize)]
struct TypeEdgeRow {
    record: &'static str,
    relation: &'static str,
    change: &'static str,
    owner_path: String,
    owner_name: Option<String>,
    target_path: String,
    target_name: Option<String>,
    kind: String,
    from_origin: Option<String>,
    to_origin: Option<String>,
    span: Option<SpanJson>,
}

#[derive(Serialize)]
struct ImportRow {
    record: &'static str,
    relation: &'static str,
    change: &'static str,
    src_path: String,
    name: String,
    local: String,
    target_path: String,
    target_name: Option<String>,
    hops: u32,
}

/// One shape for both `diff_unresolved` relations: `unresolved` call sites carry
/// `path`/`detail`, `file_unresolved` rows carry `src_path`/`module`.
#[derive(Serialize)]
struct UnresolvedRow {
    record: &'static str,
    relation: &'static str,
    change: &'static str,
    path: Option<String>,
    src_path: Option<String>,
    module: Option<String>,
    detail: Option<String>,
    reason: String,
    span: Option<SpanJson>,
}

#[derive(Serialize)]
struct SpanJson {
    start: u32,
    end: u32,
}

impl From<Span> for SpanJson {
    fn from(span: Span) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum DiffRow {
    Run(RunRow),
    File(FileRow),
    Edge(EdgeRow),
    TypeEdge(TypeEdgeRow),
    Import(ImportRow),
    Unresolved(UnresolvedRow),
}

impl DiffRow {
    /// The header is rank 0; the rest follow the brief's file-first order, which
    /// is also the order the counts object lists them in.
    fn rank(&self) -> u8 {
        match self {
            DiffRow::Run(_) => 0,
            DiffRow::File(_) => 1,
            DiffRow::Edge(_) => 2,
            DiffRow::TypeEdge(_) => 3,
            DiffRow::Import(_) => 4,
            DiffRow::Unresolved(_) => 5,
        }
    }

    fn sort_path(&self) -> &str {
        match self {
            DiffRow::Run(_) => "",
            DiffRow::File(row) => &row.path,
            DiffRow::Edge(row) => &row.caller_path,
            DiffRow::TypeEdge(row) => &row.owner_path,
            DiffRow::Import(row) => &row.src_path,
            DiffRow::Unresolved(row) => row
                .path
                .as_deref()
                .or(row.src_path.as_deref())
                .unwrap_or(""),
        }
    }

    fn sort_key(&self) -> String {
        match self {
            DiffRow::Run(_) => String::new(),
            DiffRow::File(row) => row.path.clone(),
            DiffRow::Edge(row) => named_key(&[
                &row.caller_path,
                text(&row.caller_name),
                &row.callee_path,
                text(&row.callee_name),
                &row.kind,
                text(&row.from_origin),
                text(&row.to_origin),
            ]),
            DiffRow::TypeEdge(row) => named_key(&[
                &row.owner_path,
                text(&row.owner_name),
                &row.target_path,
                text(&row.target_name),
                &row.kind,
                text(&row.from_origin),
                text(&row.to_origin),
            ]),
            DiffRow::Import(row) => named_key(&[
                &row.src_path,
                &row.name,
                &row.target_path,
                text(&row.target_name),
            ]),
            DiffRow::Unresolved(row) => match row.relation {
                "file_unresolved" => named_key(&[
                    row.src_path.as_deref().unwrap_or(""),
                    row.module.as_deref().unwrap_or(""),
                    &row.reason,
                ]),
                _ => named_key(&[
                    row.path.as_deref().unwrap_or(""),
                    row.detail.as_deref().unwrap_or(""),
                    &row.reason,
                ]),
            },
        }
    }
}

fn text(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

fn named_key(fields: &[&str]) -> String {
    fields.join("\u{0}")
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

trait DiffFact {
    fn base_key(&self) -> String;
    fn origin(&self) -> Option<&str>;
    fn span(&self) -> Option<Span>;
}

#[derive(Clone)]
struct EdgeFact {
    caller_path: String,
    caller_name: Option<String>,
    callee_path: String,
    callee_name: Option<String>,
    kind: String,
    origin: String,
    span: Option<Span>,
}

impl DiffFact for EdgeFact {
    fn base_key(&self) -> String {
        named_key(&[
            &self.caller_path,
            text(&self.caller_name),
            &self.callee_path,
            text(&self.callee_name),
            &self.kind,
        ])
    }
    fn origin(&self) -> Option<&str> {
        Some(&self.origin)
    }
    fn span(&self) -> Option<Span> {
        self.span
    }
}

#[derive(Clone)]
struct TypeEdgeFact {
    owner_path: String,
    owner_name: Option<String>,
    target_path: String,
    target_name: Option<String>,
    kind: String,
    origin: String,
    span: Option<Span>,
}

impl DiffFact for TypeEdgeFact {
    fn base_key(&self) -> String {
        named_key(&[
            &self.owner_path,
            text(&self.owner_name),
            &self.target_path,
            text(&self.target_name),
            &self.kind,
        ])
    }
    fn origin(&self) -> Option<&str> {
        Some(&self.origin)
    }
    fn span(&self) -> Option<Span> {
        self.span
    }
}

#[derive(Clone)]
struct ImportFact {
    src_path: String,
    name: String,
    local: String,
    target_path: String,
    target_name: Option<String>,
    hops: u32,
}

impl DiffFact for ImportFact {
    fn base_key(&self) -> String {
        named_key(&[
            &self.src_path,
            &self.name,
            &self.target_path,
            text(&self.target_name),
        ])
    }
    fn origin(&self) -> Option<&str> {
        None
    }
    fn span(&self) -> Option<Span> {
        None
    }
}

#[derive(Clone)]
struct UnresolvedFact {
    relation: &'static str,
    path: Option<String>,
    src_path: Option<String>,
    module: Option<String>,
    reason: String,
    detail: Option<String>,
    span: Option<Span>,
}

impl DiffFact for UnresolvedFact {
    fn base_key(&self) -> String {
        match self.relation {
            "file_unresolved" => named_key(&[
                self.src_path.as_deref().unwrap_or(""),
                self.module.as_deref().unwrap_or(""),
                &self.reason,
            ]),
            _ => named_key(&[
                self.path.as_deref().unwrap_or(""),
                self.detail.as_deref().unwrap_or(""),
                &self.reason,
            ]),
        }
    }
    fn origin(&self) -> Option<&str> {
        None
    }
    fn span(&self) -> Option<Span> {
        self.span
    }
}

struct FactDelta<F> {
    change: Change,
    from_origin: Option<String>,
    to_origin: Option<String>,
    fact_a: Option<F>,
    fact_b: Option<F>,
}

struct Keyed<F> {
    fact: F,
}

/// The set difference on span-free keys. Two rows sharing a base key but not its
/// origin pair into one `origin_changed`; the unpaired remainder is added/removed.
fn difference<F: DiffFact + Clone>(a: Vec<F>, b: Vec<F>) -> Vec<FactDelta<F>> {
    let left = index(a);
    let right = index(b);
    let mut deltas = Vec::new();
    for (base, origins) in &left {
        let Some(other) = right.get(base) else {
            for (origin, keyed) in origins {
                deltas.push(FactDelta {
                    change: Change::Removed,
                    from_origin: origin_text(origin),
                    to_origin: None,
                    fact_a: Some(keyed.fact.clone()),
                    fact_b: None,
                });
            }
            continue;
        };
        let a_only: Vec<&String> = origins
            .keys()
            .filter(|origin| !other.contains_key(*origin))
            .collect();
        let b_only: Vec<&String> = other
            .keys()
            .filter(|origin| !origins.contains_key(*origin))
            .collect();
        let paired = a_only.len().min(b_only.len());
        for index in 0..paired {
            deltas.push(FactDelta {
                change: Change::OriginChanged,
                from_origin: origin_text(a_only[index]),
                to_origin: origin_text(b_only[index]),
                fact_a: Some(origins[a_only[index]].fact.clone()),
                fact_b: Some(other[b_only[index]].fact.clone()),
            });
        }
        for origin in &a_only[paired..] {
            deltas.push(FactDelta {
                change: Change::Removed,
                from_origin: origin_text(origin),
                to_origin: None,
                fact_a: Some(origins[*origin].fact.clone()),
                fact_b: None,
            });
        }
        for origin in &b_only[paired..] {
            deltas.push(FactDelta {
                change: Change::Added,
                from_origin: None,
                to_origin: origin_text(origin),
                fact_a: None,
                fact_b: Some(other[*origin].fact.clone()),
            });
        }
    }
    for (base, origins) in &right {
        if left.contains_key(base) {
            continue;
        }
        for (origin, keyed) in origins {
            deltas.push(FactDelta {
                change: Change::Added,
                from_origin: None,
                to_origin: origin_text(origin),
                fact_a: None,
                fact_b: Some(keyed.fact.clone()),
            });
        }
    }
    deltas
}

/// Group by base key then by origin; sorting first keeps the lowest span as the
/// representative. `None` origin keys as the empty string, which no leg emits.
fn index<F: DiffFact>(mut facts: Vec<F>) -> BTreeMap<String, BTreeMap<String, Keyed<F>>> {
    facts.sort_by_key(|fact| {
        (
            fact.base_key(),
            fact.origin().unwrap_or("").to_string(),
            fact.span(),
        )
    });
    let mut map: BTreeMap<String, BTreeMap<String, Keyed<F>>> = BTreeMap::new();
    for fact in facts {
        let base = fact.base_key();
        let origin = fact.origin().unwrap_or("").to_string();
        map.entry(base)
            .or_default()
            .entry(origin)
            .or_insert(Keyed { fact });
    }
    map
}

fn origin_text(origin: &str) -> Option<String> {
    (!origin.is_empty()).then(|| origin.to_string())
}

fn edge_facts(side: &Side) -> Vec<EdgeFact> {
    side.facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::ResolvedEdge {
                caller_path,
                caller_name,
                callee_path,
                callee_name,
                caller_site_start,
                caller_site_end,
                kind,
                resolution_origin,
                ..
            } => Some(EdgeFact {
                caller_path: caller_path.clone(),
                caller_name: caller_name.clone(),
                callee_path: callee_path.clone(),
                callee_name: callee_name.clone(),
                kind: kind.clone(),
                origin: resolution_origin.clone(),
                span: Some(Span {
                    start: *caller_site_start,
                    end: *caller_site_end,
                }),
            }),
            _ => None,
        })
        .collect()
}

fn type_edge_facts(side: &Side) -> Vec<TypeEdgeFact> {
    side.facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::ResolvedTypeEdge {
                owner_path,
                owner_name,
                owner_start,
                owner_end,
                target_path,
                target_name,
                kind,
                resolution_origin,
                ..
            } => Some(TypeEdgeFact {
                owner_path: owner_path.clone(),
                owner_name: owner_name.clone(),
                target_path: target_path.clone(),
                target_name: target_name.clone(),
                kind: kind.clone(),
                origin: resolution_origin.clone(),
                span: Some(Span {
                    start: *owner_start,
                    end: *owner_end,
                }),
            }),
            _ => None,
        })
        .collect()
}

fn import_facts(side: &Side) -> Vec<ImportFact> {
    side.facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::ResolvedImportRow {
                src_path,
                name,
                local,
                target_path,
                target_name,
                hops,
                ..
            } => Some(ImportFact {
                src_path: src_path.clone(),
                name: name.clone(),
                local: local.clone(),
                target_path: target_path.clone(),
                target_name: target_name.clone(),
                hops: *hops,
            }),
            _ => None,
        })
        .collect()
}

fn unresolved_facts(side: &Side) -> Vec<UnresolvedFact> {
    side.facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::FileUnresolvedRow {
                src_path,
                module,
                reason,
            } => Some(UnresolvedFact {
                relation: "file_unresolved",
                path: None,
                src_path: Some(src_path.clone()),
                module: Some(module.clone()),
                reason: reason.clone(),
                detail: None,
                span: None,
            }),
            FlatFact::Unresolved {
                path,
                span,
                reason,
                detail,
                ..
            } => Some(UnresolvedFact {
                relation: "unresolved",
                path: path.clone(),
                src_path: None,
                module: None,
                reason: reason.clone(),
                detail: Some(detail.clone()),
                span: Some(Span {
                    start: span.start,
                    end: span.end,
                }),
            }),
            _ => None,
        })
        .collect()
}

fn side_span<F: DiffFact>(delta: &FactDelta<F>) -> Option<SpanJson> {
    delta
        .fact_b
        .as_ref()
        .and_then(|fact| fact.span())
        .or_else(|| delta.fact_a.as_ref().and_then(|fact| fact.span()))
        .map(SpanJson::from)
}

fn edge_rows(a: &Side, b: &Side, counts: &mut Counts) -> Vec<DiffRow> {
    difference(edge_facts(a), edge_facts(b))
        .into_iter()
        .map(|delta| {
            counts.resolved_edge.add(delta.change);
            let fact = delta.fact_b.clone().or_else(|| delta.fact_a.clone());
            let fact = fact.expect("every delta carries a side");
            let span = side_span(&delta);
            DiffRow::Edge(EdgeRow {
                record: "diff_edge",
                relation: "resolved_edge",
                change: delta.change.as_str(),
                caller_path: fact.caller_path,
                caller_name: fact.caller_name,
                callee_path: fact.callee_path,
                callee_name: fact.callee_name,
                kind: fact.kind,
                from_origin: delta.from_origin,
                to_origin: delta.to_origin,
                span,
            })
        })
        .collect()
}

fn type_edge_rows(a: &Side, b: &Side, counts: &mut Counts) -> Vec<DiffRow> {
    difference(type_edge_facts(a), type_edge_facts(b))
        .into_iter()
        .map(|delta| {
            counts.resolved_type_edge.add(delta.change);
            let fact = delta.fact_b.clone().or_else(|| delta.fact_a.clone());
            let fact = fact.expect("every delta carries a side");
            let span = side_span(&delta);
            DiffRow::TypeEdge(TypeEdgeRow {
                record: "diff_type_edge",
                relation: "resolved_type_edge",
                change: delta.change.as_str(),
                owner_path: fact.owner_path,
                owner_name: fact.owner_name,
                target_path: fact.target_path,
                target_name: fact.target_name,
                kind: fact.kind,
                from_origin: delta.from_origin,
                to_origin: delta.to_origin,
                span,
            })
        })
        .collect()
}

fn import_rows(a: &Side, b: &Side, counts: &mut Counts) -> Vec<DiffRow> {
    difference(import_facts(a), import_facts(b))
        .into_iter()
        .map(|delta| {
            counts.resolved_import.add(delta.change);
            let fact = delta.fact_b.clone().or_else(|| delta.fact_a.clone());
            let fact = fact.expect("every delta carries a side");
            DiffRow::Import(ImportRow {
                record: "diff_import",
                relation: "resolved_import",
                change: delta.change.as_str(),
                src_path: fact.src_path,
                name: fact.name,
                local: fact.local,
                target_path: fact.target_path,
                target_name: fact.target_name,
                hops: fact.hops,
            })
        })
        .collect()
}

fn unresolved_rows(a: &Side, b: &Side, counts: &mut Counts) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    for delta in difference(unresolved_facts(a), unresolved_facts(b)) {
        let fact = delta.fact_b.clone().or_else(|| delta.fact_a.clone());
        let fact = fact.expect("every delta carries a side");
        let span = side_span(&delta);
        match fact.relation {
            "file_unresolved" => counts.file_unresolved.add(delta.change),
            _ => counts.unresolved.add(delta.change),
        }
        rows.push(DiffRow::Unresolved(UnresolvedRow {
            record: "diff_unresolved",
            relation: fact.relation,
            change: delta.change.as_str(),
            path: fact.path,
            src_path: fact.src_path,
            module: fact.module,
            detail: fact.detail,
            reason: fact.reason,
            span,
        }));
    }
    rows
}

fn write_jsonl(rows: &[(u8, String, String, DiffRow)]) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = std::io::stdout();
    let mut output = std::io::BufWriter::with_capacity(256 * 1024, stdout.lock());
    for (_, _, _, row) in rows {
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
    rows: &[(u8, String, String, DiffRow)],
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
        let mut statements = Statements {
            run: connection.prepare(INSERT_RUN)?,
            file: connection.prepare(INSERT_FILE)?,
            edge: connection.prepare(INSERT_EDGE)?,
            type_edge: connection.prepare(INSERT_TYPE_EDGE)?,
            import: connection.prepare(INSERT_IMPORT)?,
            unresolved: connection.prepare(INSERT_UNRESOLVED)?,
        };
        for (ordinal, (_, _, _, row)) in rows.iter().enumerate() {
            insert_row(&mut statements, row, ordinal as i64 + 1)?;
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

struct Statements<'a> {
    run: rusqlite::Statement<'a>,
    file: rusqlite::Statement<'a>,
    edge: rusqlite::Statement<'a>,
    type_edge: rusqlite::Statement<'a>,
    import: rusqlite::Statement<'a>,
    unresolved: rusqlite::Statement<'a>,
}

fn insert_row(
    statements: &mut Statements<'_>,
    row: &DiffRow,
    ordinal: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    match row {
        DiffRow::Run(row) => {
            statements.run.execute(params![
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
            statements.file.execute(params![
                row.record,
                row.change,
                row.path,
                row.from_digest,
                row.to_digest,
                ordinal
            ])?;
        }
        DiffRow::Edge(row) => {
            let (start, end) = span_bounds(&row.span);
            statements.edge.execute(params![
                row.record,
                row.relation,
                row.change,
                row.caller_path,
                row.caller_name,
                row.callee_path,
                row.callee_name,
                row.kind,
                row.from_origin,
                row.to_origin,
                start,
                end,
                ordinal
            ])?;
        }
        DiffRow::TypeEdge(row) => {
            let (start, end) = span_bounds(&row.span);
            statements.type_edge.execute(params![
                row.record,
                row.relation,
                row.change,
                row.owner_path,
                row.owner_name,
                row.target_path,
                row.target_name,
                row.kind,
                row.from_origin,
                row.to_origin,
                start,
                end,
                ordinal
            ])?;
        }
        DiffRow::Import(row) => {
            statements.import.execute(params![
                row.record,
                row.relation,
                row.change,
                row.src_path,
                row.name,
                row.local,
                row.target_path,
                row.target_name,
                row.hops as i64,
                ordinal
            ])?;
        }
        DiffRow::Unresolved(row) => {
            let (start, end) = span_bounds(&row.span);
            statements.unresolved.execute(params![
                row.record,
                row.relation,
                row.change,
                row.path,
                row.src_path,
                row.module,
                row.detail,
                row.reason,
                start,
                end,
                ordinal
            ])?;
        }
    }
    Ok(())
}

fn span_bounds(span: &Option<SpanJson>) -> (Option<i64>, Option<i64>) {
    match span {
        Some(span) => (Some(i64::from(span.start)), Some(i64::from(span.end))),
        None => (None, None),
    }
}

const DIFF_DDL: &str = "\
BEGIN IMMEDIATE;
CREATE TABLE diff_run (
    record TEXT NOT NULL, from_sha TEXT NOT NULL, to_sha TEXT NOT NULL,
    files_a INTEGER NOT NULL, files_b INTEGER NOT NULL,
    changed_blobs INTEGER NOT NULL, counts TEXT NOT NULL, _row INTEGER NOT NULL);
CREATE TABLE diff_file (
    record TEXT NOT NULL, change TEXT NOT NULL, path TEXT NOT NULL,
    from_digest TEXT, to_digest TEXT, _row INTEGER NOT NULL);
CREATE TABLE diff_edge (
    record TEXT NOT NULL, relation TEXT NOT NULL, change TEXT NOT NULL,
    caller_path TEXT NOT NULL, caller_name TEXT, callee_path TEXT NOT NULL,
    callee_name TEXT, kind TEXT NOT NULL, from_origin TEXT, to_origin TEXT,
    span__start INTEGER, span__end INTEGER, _row INTEGER NOT NULL);
CREATE TABLE diff_type_edge (
    record TEXT NOT NULL, relation TEXT NOT NULL, change TEXT NOT NULL,
    owner_path TEXT NOT NULL, owner_name TEXT, target_path TEXT NOT NULL,
    target_name TEXT, kind TEXT NOT NULL, from_origin TEXT, to_origin TEXT,
    span__start INTEGER, span__end INTEGER, _row INTEGER NOT NULL);
CREATE TABLE diff_import (
    record TEXT NOT NULL, relation TEXT NOT NULL, change TEXT NOT NULL,
    src_path TEXT NOT NULL, name TEXT NOT NULL, local TEXT NOT NULL,
    target_path TEXT NOT NULL, target_name TEXT, hops INTEGER NOT NULL,
    _row INTEGER NOT NULL);
CREATE TABLE diff_unresolved (
    record TEXT NOT NULL, relation TEXT NOT NULL, change TEXT NOT NULL,
    path TEXT, src_path TEXT, module TEXT, detail TEXT, reason TEXT NOT NULL,
    span__start INTEGER, span__end INTEGER, _row INTEGER NOT NULL);";

const INSERT_RUN: &str = "\
INSERT INTO diff_run
    (record, from_sha, to_sha, files_a, files_b, changed_blobs, counts, _row)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

const INSERT_FILE: &str = "\
INSERT INTO diff_file (record, change, path, from_digest, to_digest, _row)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

const INSERT_EDGE: &str = "\
INSERT INTO diff_edge
    (record, relation, change, caller_path, caller_name, callee_path, callee_name,
     kind, from_origin, to_origin, span__start, span__end, _row)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)";

const INSERT_TYPE_EDGE: &str = "\
INSERT INTO diff_type_edge
    (record, relation, change, owner_path, owner_name, target_path, target_name,
     kind, from_origin, to_origin, span__start, span__end, _row)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)";

const INSERT_IMPORT: &str = "\
INSERT INTO diff_import
    (record, relation, change, src_path, name, local, target_path, target_name, hops, _row)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

const INSERT_UNRESOLVED: &str = "\
INSERT INTO diff_unresolved
    (record, relation, change, path, src_path, module, detail, reason, span__start,
     span__end, _row)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)";
