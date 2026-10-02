//! The one path a checker answer takes into `resolved_edge` rows: the target
//! definition, the caller definition, then a rewrite of the site's rows or a new row.

use std::collections::{BTreeMap, HashMap};

use crate::{closure_name, ContentId, FamilyTag, FlatFact, RawProjectFact};

/// One call or type definition, spans nesting.
struct Def {
    start: u32,
    end: u32,
    name: Option<String>,
    call: bool,
}

/// One file's definitions sorted by (start, outer first), with each one's
/// innermost enclosing definition.
struct FileDefs {
    blob: ContentId,
    defs: Vec<Def>,
    parent: Vec<Option<usize>>,
}

/// The definition a checker named: its file, span and name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckerTarget {
    pub path: String,
    pub start: u32,
    pub end: u32,
    pub name: Option<String>,
}

/// One checker answer for one call site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckerEdge {
    pub source: String,
    pub site_start: u32,
    pub site_end: u32,
    pub target: CheckerTarget,
}

/// Call and type definitions per file, read off the raw extraction stream.
#[derive(Default)]
pub struct CheckerDefs {
    files: BTreeMap<String, FileDefs>,
    sealed: bool,
}

impl CheckerDefs {
    pub fn capture(&mut self, raw: &RawProjectFact<'_>) {
        if let FlatFact::Node { family: family @ (FamilyTag::Call | FamilyTag::Type), span, name, .. } = &raw.fact {
            self.push(raw.path, raw.content_id, span.start, span.end, name.clone(), *family == FamilyTag::Call);
        }
    }

    pub fn push(&mut self, path: &str, blob: &ContentId, start: u32, end: u32, name: Option<String>, call: bool) {
        self.sealed = false;
        let file = self.files.entry(path.to_string()).or_insert_with(|| FileDefs {
            blob: blob.clone(),
            defs: Vec::new(),
            parent: Vec::new(),
        });
        file.defs.push(Def { start, end, name, call });
    }

    /// Sort each file once and link every definition to its enclosing one.
    pub fn seal(&mut self) {
        if self.sealed {
            return;
        }
        for file in self.files.values_mut() {
            file.defs.sort_by_key(|def| (def.start, std::cmp::Reverse(def.end), !def.call));
            file.parent = vec![None; file.defs.len()];
            let mut open: Vec<usize> = Vec::new();
            for index in 0..file.defs.len() {
                let _step = tracing::trace_span!("checker.defs.link").entered();
                while open.last().is_some_and(|&top| file.defs[top].end < file.defs[index].end) {
                    open.pop();
                }
                file.parent[index] = open.last().copied();
                open.push(index);
            }
        }
        self.sealed = true;
    }

    /// Definitions enclosing `[start, end)` in `path`, innermost first.
    fn enclosing(&self, path: &str, start: u32, end: u32) -> impl Iterator<Item = (&FileDefs, &Def)> {
        debug_assert!(self.sealed, "CheckerDefs::seal runs before a lookup");
        let file = self.files.get(path);
        let mut at = file.and_then(|file| {
            file.defs.partition_point(|def| def.start <= start).checked_sub(1)
        });
        std::iter::from_fn(move || {
            let file = file?;
            while let Some(index) = at {
                let _step = tracing::trace_span!("checker.defs.step").entered();
                at = file.parent[index];
                let def = &file.defs[index];
                if def.start <= start && end <= def.end {
                    return Some((file, def));
                }
            }
            None
        })
    }

    /// The innermost named callable at `at` in `path`, else the innermost named
    /// type; `name` narrows both to that spelling.
    pub fn target(&self, path: &str, at: u32, name: Option<&str>) -> Option<CheckerTarget> {
        let wanted = |def: &Def| {
            def.name.as_deref().is_some_and(|found| name.is_none_or(|name| name == found))
        };
        let (mut call, mut other) = (None, None);
        for (_, def) in self.enclosing(path, at, at + 1) {
            if !wanted(def) {
                continue;
            }
            if def.call {
                call = Some(def);
                break;
            }
            other = other.or(Some(def));
        }
        call.or(other).map(|def| CheckerTarget {
            path: path.to_string(),
            start: def.start,
            end: def.end,
            name: def.name.clone(),
        })
    }

    /// The innermost callable covering a site; an unnamed one is its closure name.
    pub fn caller(&self, path: &str, start: u32, end: u32) -> Option<String> {
        self.enclosing(path, start, end)
            .find(|(_, def)| def.call)
            .map(|(file, def)| def.name.clone().unwrap_or_else(|| closure_name(&file.blob, def.start)))
    }

    /// Point every fast row at a checked site to its checker target; a site with
    /// no fast row gains one, attributed to its innermost callable.
    pub fn write(&self, facts: &mut Vec<FlatFact>, edges: impl IntoIterator<Item = CheckerEdge>) {
        let mut by_site: HashMap<(String, u32, u32), Vec<usize>> = HashMap::new();
        for (row, fact) in facts.iter().enumerate() {
            if let FlatFact::ResolvedEdge { caller_path, caller_site_start, caller_site_end, .. } = fact {
                by_site.entry((caller_path.clone(), *caller_site_start, *caller_site_end)).or_default().push(row);
            }
        }
        for edge in edges {
            let _site = tracing::trace_span!("checker.edge.site").entered();
            let key = (edge.source, edge.site_start, edge.site_end);
            let target = edge.target;
            if let Some(rows) = by_site.get(&key).filter(|rows| !rows.is_empty()) {
                for &row in rows {
                    let _row = tracing::trace_span!("checker.edge.row").entered();
                    if let FlatFact::ResolvedEdge {
                        callee_path, callee_name, callee_start, callee_end, kind, resolution_origin, ..
                    } = &mut facts[row] {
                        *callee_path = target.path.clone();
                        *callee_name = target.name.clone();
                        *callee_start = target.start;
                        *callee_end = target.end;
                        *kind = "checker_resolve".to_string();
                        *resolution_origin = "checker".to_string();
                    }
                }
                continue;
            }
            let caller_name = self.caller(&key.0, key.1, key.2);
            by_site.entry(key.clone()).or_default().push(facts.len());
            facts.push(FlatFact::ResolvedEdge {
                fact: None,
                caller_path: key.0,
                caller_name,
                caller_site_start: key.1,
                caller_site_end: key.2,
                callee_path: target.path,
                callee_name: target.name,
                callee_start: target.start,
                callee_end: target.end,
                kind: "checker_resolve".to_string(),
                resolution_origin: "checker".to_string(),
            });
        }
    }
}
