//! scm++ store rows, engine neutral: dictionaries, one preorder node walk per file, capture rows.
//! `Rows` is the seam a row writer implements; the SQLite writer is `1b_scmpp_sqlite.rs`.
use hafley_scm::scmpp::{Compiled, ROOT};
use rustc_hash::{FxHashMap, FxHashSet};
use tree_sitter::{QueryCursor, StreamingIterator, Tree};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// One dictionary per string domain; a row holds the id, the dictionary holds the text once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dict {
    Path,
    Kind,
    Field,
    Capture,
    Text,
}

/// One tree-sitter node, named or anonymous, numbered in preorder within its file.
pub struct NodeRow {
    pub file: i64,
    pub pre: i64,
    /// `pre` of the last node in this node's subtree.
    pub last: i64,
    /// `pre` of the parent; -1 at the root.
    pub parent: i64,
    pub depth: i64,
    /// Index among the parent's named children; `None` for anonymous nodes and the root.
    pub sib: Option<i64>,
    /// Index among all the parent's children.
    pub idx: i64,
    pub kind: i64,
    /// 0 when the node sits in no field.
    pub field: i64,
    pub start: i64,
    pub end: i64,
    pub named: bool,
}

/// One capture of one match; `node` is the captured node's `pre`.
pub struct CaptureRow {
    pub file: i64,
    pub pattern: i64,
    /// Per-file ordinal across every flat pattern.
    pub r#match: i64,
    pub capture: i64,
    pub node: i64,
    pub start: i64,
    pub end: i64,
    /// `None` for the `@__root` capture.
    pub text: Option<i64>,
}

/// The row-writing seam: dictionary entries arrive before any row that names them.
pub trait Rows {
    fn dict(&mut self, dict: Dict, id: i64, text: &str) -> Result<()>;
    fn node(&mut self, row: &NodeRow) -> Result<()>;
    fn capture(&mut self, row: &CaptureRow) -> Result<()>;
}

#[derive(Default)]
struct Interner {
    ids: FxHashMap<Box<str>, i64>,
}

impl Interner {
    /// Ids count up from 1 in first-seen order; a new text is written once.
    fn id(&mut self, dict: Dict, text: &str, rows: &mut impl Rows) -> Result<i64> {
        if let Some(id) = self.ids.get(text) {
            return Ok(*id);
        }
        let id = self.ids.len() as i64 + 1;
        self.ids.insert(text.into(), id);
        rows.dict(dict, id, text)?;
        Ok(id)
    }
}

/// One per `query --scmpp` run.
#[derive(Default)]
pub struct Store {
    path: Interner,
    kind: Interner,
    field: Interner,
    capture: Interner,
    text: Interner,
    /// Files whose node rows are written; a path given twice writes them once.
    noded: FxHashSet<i64>,
}

/// The SQL reads nodes only through a relation; nested levels hang off the root's
/// relations, so a root without one means a plan without one.
pub fn reads_nodes(compiled: &Compiled) -> bool {
    !compiled.plan.rels.is_empty()
}

/// A capture held until the walk numbers its node.
struct Pending {
    pattern: i64,
    r#match: i64,
    capture: i64,
    node: usize,
    start: i64,
    end: i64,
    text: Option<i64>,
}

impl Store {
    /// Interns the plan's capture names and fields first, so entry `i` gets the id `i + 1` the SQL names;
    /// a second call with the same lists finds every id in place.
    fn seed(&mut self, compiled: &Compiled, rows: &mut impl Rows) -> Result<()> {
        for (dict, interner, names) in [
            (Dict::Capture, &mut self.capture, &compiled.captures),
            (Dict::Field, &mut self.field, &compiled.fields),
        ] {
            for (index, name) in names.iter().enumerate() {
                let id = interner.id(dict, name, rows)?;
                if id != index as i64 + 1 {
                    return Err(format!(
                        "scm++: {dict:?} dictionary holds `{name}` at id {id}, the SQL names it {}",
                        index + 1
                    )
                    .into());
                }
            }
        }
        Ok(())
    }

    /// Capture rows of every flat pattern over one file (`match` = per-file ordinal), and its
    /// node rows when the plan reads nodes; captured nodes get their `pre` from the same walk.
    pub fn write_file(
        &mut self,
        rows: &mut impl Rows,
        compiled: &Compiled,
        path: &str,
        src: &[u8],
        tree: &Tree,
    ) -> Result<()> {
        self.seed(compiled, rows)?;
        let file = self.path.id(Dict::Path, path, rows)?;
        let mut pending = Vec::new();
        let mut ordinal = 0i64;
        for pattern in &compiled.patterns {
            let names = pattern.query.capture_names();
            let mut ids = Vec::with_capacity(names.len());
            for name in names {
                ids.push(self.capture.id(Dict::Capture, name, rows)?);
            }
            let root = names
                .iter()
                .position(|name| *name == ROOT)
                .expect("flat pattern root") as u32;
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&pattern.query, tree.root_node(), src);
            while let Some(found) = matches.next() {
                if !found.captures().iter().any(|capture| capture.index == root) {
                    continue;
                }
                for capture in found.captures() {
                    let node = capture.node;
                    let text = if capture.index == root {
                        None
                    } else {
                        let text = String::from_utf8_lossy(&src[node.byte_range()]);
                        Some(self.text.id(Dict::Text, &text, rows)?)
                    };
                    pending.push(Pending {
                        pattern: pattern.id as i64,
                        r#match: ordinal,
                        capture: ids[capture.index as usize],
                        node: node.id(),
                        start: node.start_byte() as i64,
                        end: node.end_byte() as i64,
                        text,
                    });
                }
                ordinal += 1;
            }
            drop(matches);
            if cursor.did_exceed_match_limit() {
                return Err(format!(
                    "scm++ level {}: tree-sitter match limit exceeded",
                    pattern.id
                )
                .into());
            }
        }
        let wanted: FxHashSet<usize> = pending.iter().map(|capture| capture.node).collect();
        let write_nodes = reads_nodes(compiled) && self.noded.insert(file);
        let mut pre_of = FxHashMap::default();
        let mut nodes = Vec::new();
        self.walk(rows, tree, file, write_nodes, |id, pre| {
            if wanted.contains(&id) {
                pre_of.insert(id, pre);
            }
        }, &mut nodes)?;
        for row in &nodes {
            let _row = tracing::trace_span!("scmpp_cst_row").entered();
            rows.node(row)?;
        }
        for capture in pending {
            let _row = tracing::trace_span!("scmpp_capture_row").entered();
            rows.capture(&CaptureRow {
                file,
                pattern: capture.pattern,
                r#match: capture.r#match,
                capture: capture.capture,
                node: pre_of[&capture.node],
                start: capture.start,
                end: capture.end,
                text: capture.text,
            })?;
        }
        Ok(())
    }

    /// Every node, named and anonymous, in preorder; `number` sees each node's id and `pre`.
    /// With `write_nodes` the rows land in `nodes`, `last` set from the subtree.
    fn walk(
        &mut self,
        rows: &mut impl Rows,
        tree: &Tree,
        file: i64,
        write_nodes: bool,
        mut number: impl FnMut(usize, i64),
        nodes: &mut Vec<NodeRow>,
    ) -> Result<()> {
        let language = tree.language();
        let mut kinds = FxHashMap::<u16, i64>::default();
        let mut fields = FxHashMap::<u16, i64>::default();
        let mut next = 0i64;
        let mut failed = None;
        hafley_scm::cst::walk_streaming(tree, |node, parent: Option<(i64, i64)>, slot| {
            let pre = next;
            next += 1;
            number(node.id(), pre);
            let depth = parent.map_or(0, |(_, depth)| depth + 1);
            if !write_nodes || failed.is_some() {
                return (pre, depth);
            }
            let mut row = || -> Result<NodeRow> {
                let kind = match kinds.get(&node.kind_id()) {
                    Some(id) => *id,
                    None => {
                        let id = self.kind.id(Dict::Kind, node.kind(), rows)?;
                        kinds.insert(node.kind_id(), id);
                        id
                    }
                };
                let field = match slot.field {
                    None => 0,
                    Some(field) => match fields.get(&field) {
                        Some(id) => *id,
                        None => {
                            let name = language
                                .field_name_for_id(field)
                                .ok_or_else(|| format!("tree-sitter field id {field} has no name"))?;
                            let id = self.field.id(Dict::Field, name, rows)?;
                            fields.insert(field, id);
                            id
                        }
                    },
                };
                Ok(NodeRow {
                    file,
                    pre,
                    last: pre,
                    parent: parent.map_or(-1, |(parent, _)| parent),
                    depth,
                    sib: slot.named_index.map(i64::from),
                    idx: slot.index as i64,
                    kind,
                    field,
                    start: node.start_byte() as i64,
                    end: node.end_byte() as i64,
                    named: node.is_named(),
                })
            };
            match row() {
                Ok(row) => nodes.push(row),
                Err(error) => failed = Some(error),
            }
            (pre, depth)
        });
        if let Some(error) = failed {
            return Err(error);
        }
        // Preorder puts every child after its parent: one reverse pass carries `last` up.
        for index in (1..nodes.len()).rev() {
            let (parent, last) = (nodes[index].parent as usize, nodes[index].last);
            nodes[parent].last = nodes[parent].last.max(last);
        }
        Ok(())
    }
}
