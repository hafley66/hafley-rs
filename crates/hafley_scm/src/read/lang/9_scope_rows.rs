//! Scopes as anonymous `tsi.product`s, closures as `tsi.callable`s over them.
//! `scope/*.scm` answers every language question; this reader names none.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::{Arc, Mutex, OnceLock};

use crate::read::tsi::{Arg, FactOut};

mod generated {
    include!("scope_scm_generated.rs");
}
use generated::{ScopeRow, ScopeSchema};

/// The label of a nested scope's edge to the scope holding it. `^` starts no
/// Rust or TypeScript identifier, so no binding collides with it.
pub const PARENT_LABEL: &str = "^parent";
/// The `tsi.input` position of a callable's env product.
pub const ENV_INPUT: i64 = -1;
/// The `tsi.input` position of a method's receiver product.
pub const SELF_INPUT: i64 = -2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScopeLang {
    Rust,
    TypeScript,
    Tsx,
}

impl ScopeLang {
    /// The grammar a path parses under; None when no scope query covers it.
    pub fn of_path(path: &str) -> Option<Self> {
        let ext = path.rsplit_once('.').map(|(_, ext)| ext)?;
        match ext {
            "rs" => Some(Self::Rust),
            "ts" | "mts" | "cts" | "js" | "mjs" | "cjs" => Some(Self::TypeScript),
            "tsx" | "jsx" => Some(Self::Tsx),
            _ => None,
        }
    }

    fn atom(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript | Self::Tsx => "ts",
        }
    }

    fn query_text(self) -> &'static str {
        match self {
            Self::Rust => include_str!("scope/rust.scm"),
            Self::TypeScript | Self::Tsx => include_str!("scope/ts.scm"),
        }
    }

    fn language(self) -> Option<tree_sitter::Language> {
        match self {
            #[cfg(feature = "rust")]
            Self::Rust => Some(tree_sitter::Language::new(tree_sitter_rust::LANGUAGE)),
            #[cfg(feature = "typescript")]
            Self::TypeScript => Some(tree_sitter::Language::new(
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            )),
            #[cfg(feature = "typescript")]
            Self::Tsx => Some(tree_sitter::Language::new(
                tree_sitter_typescript::LANGUAGE_TSX,
            )),
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }
}

type Compiled = Arc<(hafley_scm::QueryExt, ScopeSchema)>;
static QUERIES: OnceLock<Mutex<HashMap<ScopeLang, Compiled>>> = OnceLock::new();

/// The bundled query, compiled once per language. A query that does not
/// compile is a build defect, so it panics rather than dropping rows.
fn compiled(lang: ScopeLang, language: &tree_sitter::Language) -> Compiled {
    let mut cache = QUERIES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("scope query cache is not poisoned");
    Arc::clone(cache.entry(lang).or_insert_with(|| {
        let query = hafley_scm::build(language, lang.query_text())
            .unwrap_or_else(|error| panic!("scope query for {lang:?}: {error:?}"));
        let schema = ScopeSchema::of(&query);
        Arc::new((query, schema))
    }))
}

/// Parse `src` and append its scope rows to `tsi`.
pub fn append(lang: ScopeLang, src: &[u8], tsi: &mut Vec<FactOut>) {
    let Some(language) = lang.language() else {
        return;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return;
    }
    if let Some(tree) = parser.parse(src, None) {
        append_from_tree(lang, &tree, src, tsi);
    }
}

/// Append the scope rows of an already parsed `tree` to `tsi`, numbering ids
/// and ordinals past the ones `tsi` holds and reusing its named types.
pub fn append_from_tree(lang: ScopeLang, tree: &tree_sitter::Tree, src: &[u8], tsi: &mut Vec<FactOut>) {
    let Some(language) = lang.language() else {
        return;
    };
    let compiled = compiled(lang, &language);
    let (query, schema) = (&compiled.0, &compiled.1);
    let mut arena = hafley_scm::MatchArena::default();
    if hafley_scm::run(query, "", src, tree, u32::MAX, &mut arena).is_err() {
        return;
    }
    let rows = Rows::read(query, schema, &arena, src);
    let mut out = Out::seeded(tsi, lang.atom());
    Graph::build(rows, &mut out).emit(&mut out);
    tsi.extend(out.facts);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeKind {
    Module,
    Block,
    Function,
    Closure,
    Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeclKind {
    Binding,
    Cell,
    Param,
    SelfParam,
    Item,
    Write,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallableKind {
    Closure,
    Function,
    Method,
    Item,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Capture {
    Borrow,
    Move,
    Nothing,
    Cell,
}

/// Declared weakest first: two rows on one span keep the stronger mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum UseMode {
    Read,
    Argument,
    Exclusive,
    SelfRef,
    Frame,
    Call,
}

struct DeclRow {
    span: Range<u32>,
    name: String,
    ty: Option<(Range<u32>, String)>,
    value: Option<Range<u32>>,
    kind: DeclKind,
}

struct CallableRow {
    span: Range<u32>,
    kind: CallableKind,
    capture: Capture,
    name: Option<String>,
    output: Option<(Range<u32>, String)>,
    is_async: bool,
}

struct UseRow {
    span: Range<u32>,
    name: String,
    mode: UseMode,
}

/// The emitted rows, typed and deduplicated, with no ids yet.
struct Rows {
    scopes: Vec<(Range<u32>, ScopeKind)>,
    decls: Vec<DeclRow>,
    callables: Vec<CallableRow>,
    uses: Vec<UseRow>,
}

impl Rows {
    fn read(
        query: &hafley_scm::QueryExt,
        schema: &ScopeSchema,
        arena: &hafley_scm::MatchArena,
        src: &[u8],
    ) -> Self {
        let bytes = |value: &hafley_scm::EmittedValue| value.bytes().cloned().unwrap_or(0..0);
        let text = |value: &hafley_scm::EmittedValue| value.text(src, query).unwrap_or_default().to_string();

        let mut scopes: BTreeMap<(u32, std::cmp::Reverse<u32>), ScopeKind> = BTreeMap::new();
        for row in ScopeRow::open(schema, arena) {
            let span = bytes(row.span());
            let kind = match row.kind().map(text).as_deref() {
                Some("module") => ScopeKind::Module,
                Some("function") => ScopeKind::Function,
                Some("closure") => ScopeKind::Closure,
                Some("type") => ScopeKind::Type,
                _ => ScopeKind::Block,
            };
            scopes.entry((span.start, std::cmp::Reverse(span.end))).or_insert(kind);
        }

        let mut decls: BTreeMap<(u32, u8), DeclRow> = BTreeMap::new();
        for row in ScopeRow::decl(schema, arena) {
            let span = bytes(row.span());
            let kind = match row.kind().map(text).as_deref() {
                Some("cell") => DeclKind::Cell,
                Some("param") => DeclKind::Param,
                Some("self") => DeclKind::SelfParam,
                Some("item") => DeclKind::Item,
                Some("write") => DeclKind::Write,
                _ => DeclKind::Binding,
            };
            decls.entry((span.start, kind as u8)).or_insert(DeclRow {
                name: row.name().map(text).unwrap_or_default(),
                ty: row.type_().map(|ty| (bytes(ty), text(ty))),
                value: row.value().map(bytes),
                span,
                kind,
            });
        }

        let mut callables: BTreeMap<(u32, std::cmp::Reverse<u32>), CallableRow> = BTreeMap::new();
        for row in ScopeRow::callable(schema, arena) {
            let span = bytes(row.span());
            let kind = match row.kind().map(text).as_deref() {
                Some("function") => CallableKind::Function,
                Some("method") => CallableKind::Method,
                Some("item") => CallableKind::Item,
                _ => CallableKind::Closure,
            };
            let capture = match row.capture().map(text).as_deref() {
                Some("move") => Capture::Move,
                Some("none") => Capture::Nothing,
                Some("cell") => Capture::Cell,
                _ => Capture::Borrow,
            };
            let entry = callables
                .entry((span.start, std::cmp::Reverse(span.end)))
                .or_insert(CallableRow {
                    span,
                    kind,
                    capture,
                    name: None,
                    output: None,
                    is_async: false,
                });
            if capture == Capture::Move {
                entry.capture = Capture::Move;
            }
            entry.name = entry.name.take().or_else(|| row.name().map(text));
        }
        for row in ScopeRow::output(schema, arena) {
            let span = bytes(row.span());
            if let (Some(callable), Some(ty)) = (
                callables.get_mut(&(span.start, std::cmp::Reverse(span.end))),
                row.type_(),
            ) {
                callable.output = Some((bytes(ty), text(ty)));
            }
        }
        for row in ScopeRow::is_async(schema, arena) {
            let span = bytes(row.span());
            if let Some(callable) = callables.get_mut(&(span.start, std::cmp::Reverse(span.end))) {
                callable.is_async = true;
            }
        }

        let mut uses: BTreeMap<u32, UseRow> = BTreeMap::new();
        for row in ScopeRow::uses(schema, arena) {
            let span = bytes(row.span());
            let mode = match row.mode().map(text).as_deref() {
                Some("exclusive") => UseMode::Exclusive,
                Some("call") => UseMode::Call,
                Some("argument") => UseMode::Argument,
                Some("self") => UseMode::SelfRef,
                Some("frame") => UseMode::Frame,
                _ => UseMode::Read,
            };
            let name = row.name().map(text).unwrap_or_default();
            let entry = uses.entry(span.start).or_insert(UseRow {
                span: span.clone(),
                name: name.clone(),
                mode,
            });
            if mode > entry.mode {
                *entry = UseRow { span, name, mode };
            }
        }

        Self {
            scopes: scopes
                .into_iter()
                .map(|((start, std::cmp::Reverse(end)), kind)| (start..end, kind))
                .collect(),
            decls: decls.into_values().collect(),
            callables: callables.into_values().collect(),
            uses: uses.into_values().collect(),
        }
    }
}

struct Scope {
    span: Range<u32>,
    kind: ScopeKind,
    parent: Option<usize>,
    product: u32,
}

struct Decl {
    row: DeclRow,
    owner: usize,
    /// The declaration a write versions; a declaration's own index otherwise.
    cell: usize,
    edge: u32,
    target: u32,
}

struct Callable {
    row: CallableRow,
    id: u32,
    env: u32,
}

/// One env field before it has ids: what it captures and how.
struct Field {
    label: String,
    position: u32,
    target: u32,
    source: u32,
    mode: &'static str,
}

struct Graph {
    scopes: Vec<Scope>,
    decls: Vec<Decl>,
    callables: Vec<Callable>,
    uses: Vec<UseRow>,
}

fn within(inner: &Range<u32>, outer: &Range<u32>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

impl Graph {
    fn build(rows: Rows, out: &mut Out) -> Self {
        let mut scopes: Vec<Scope> = Vec::with_capacity(rows.scopes.len());
        let mut open: Vec<usize> = Vec::new();
        for (span, kind) in rows.scopes {
            while let Some(&top) = open.last() {
                if within(&span, &scopes[top].span) {
                    break;
                }
                open.pop();
            }
            let product = out.anonymous(&span);
            out.fact("tsi.product", vec![Arg::Id(product)]);
            scopes.push(Scope {
                span,
                kind,
                parent: open.last().copied(),
                product,
            });
            open.push(scopes.len() - 1);
        }
        let mut graph = Self {
            scopes,
            decls: Vec::new(),
            callables: Vec::new(),
            uses: Vec::new(),
        };

        for row in rows.callables {
            let id = out.anonymous(&row.span);
            out.fact("tsi.callable", vec![Arg::Id(id)]);
            let env = out.anonymous(&row.span);
            out.fact("tsi.product", vec![Arg::Id(env)]);
            out.fact("tsi.input", vec![Arg::Id(id), Arg::Int(ENV_INPUT), Arg::Id(env)]);
            graph.callables.push(Callable { row, id, env });
        }

        let (writes, declared): (Vec<DeclRow>, Vec<DeclRow>) =
            rows.decls.into_iter().partition(|row| row.kind == DeclKind::Write);
        for row in declared {
            let Some(owner) = graph.owner_of(&row) else {
                continue;
            };
            let target = graph.target_of(&row, owner, out);
            let index = graph.decls.len();
            graph.decls.push(Decl {
                row,
                owner,
                cell: index,
                edge: 0,
                target,
            });
        }
        // Writes land after every declaration, so a declaration's index is final.
        for row in writes {
            let Some(scope) = graph.innermost(&row.span) else {
                continue;
            };
            let Some(cell) = graph.resolve(&row.name, row.span.start, scope) else {
                continue;
            };
            let (owner, target) = (graph.decls[cell].owner, graph.decls[cell].target);
            graph.decls.push(Decl {
                row,
                owner,
                cell,
                edge: 0,
                target,
            });
        }
        let declarers: std::collections::HashSet<u32> = graph
            .decls
            .iter()
            .filter(|decl| decl.row.kind != DeclKind::Write)
            .map(|decl| decl.row.span.start)
            .collect();
        graph.uses = rows
            .uses
            .into_iter()
            .filter(|use_row| !declarers.contains(&use_row.span.start))
            .collect();
        graph
    }

    /// The deepest scope holding `span`.
    fn innermost(&self, span: &Range<u32>) -> Option<usize> {
        self.scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| within(span, &scope.span))
            .max_by_key(|(at, _)| *at)
            .map(|(at, _)| at)
    }

    /// The deepest scope holding `span` and not equal to it.
    fn holding(&self, span: &Range<u32>) -> Option<usize> {
        self.scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| within(span, &scope.span) && scope.span != *span)
            .max_by_key(|(at, _)| *at)
            .map(|(at, _)| at)
    }

    fn owner_of(&self, row: &DeclRow) -> Option<usize> {
        match row.kind {
            DeclKind::Item => self.holding(row.value.as_ref().unwrap_or(&row.span)),
            DeclKind::Cell => {
                let mut at = self.innermost(&row.span)?;
                while !matches!(
                    self.scopes[at].kind,
                    ScopeKind::Function | ScopeKind::Closure | ScopeKind::Module
                ) {
                    at = self.scopes[at].parent?;
                }
                Some(at)
            }
            _ => self.innermost(&row.span),
        }
    }

    fn target_of(&self, row: &DeclRow, owner: usize, out: &mut Out) -> u32 {
        if let Some(value) = &row.value {
            if let Some(callable) = self.callables.iter().find(|callable| callable.row.span == *value) {
                return callable.id;
            }
        }
        if let Some((span, text)) = &row.ty {
            return out.named(text, span);
        }
        match row.kind {
            DeclKind::SelfParam => self
                .enclosing_type(owner)
                .map(|at| self.scopes[at].product)
                .unwrap_or_else(|| out.anonymous(&row.span)),
            DeclKind::Item => out.named(&row.name, &row.span),
            _ => out.anonymous(&row.span),
        }
    }

    fn enclosing_type(&self, from: usize) -> Option<usize> {
        let mut at = Some(from);
        while let Some(scope) = at {
            if self.scopes[scope].kind == ScopeKind::Type {
                return Some(scope);
            }
            at = self.scopes[scope].parent;
        }
        None
    }

    /// The declaration `name` reaches at byte `at` from `scope` outward: the
    /// latest version declared before `at`, or a hoisted cell or item.
    fn resolve(&self, name: &str, at: u32, scope: usize) -> Option<usize> {
        let mut current = Some(scope);
        while let Some(scope) = current {
            let visible: Vec<usize> = self
                .decls
                .iter()
                .enumerate()
                .filter(|(_, decl)| {
                    decl.owner == scope && decl.row.name == name && decl.row.kind != DeclKind::Write
                })
                .filter(|(_, decl)| visible_from(&decl.row) <= at)
                .map(|(index, _)| index)
                .collect();
            if let Some(&cell) = visible
                .iter()
                .find(|&&index| self.decls[index].row.kind == DeclKind::Cell)
            {
                return Some(cell);
            }
            if let Some(&last) = visible.last() {
                return Some(last);
            }
            current = self.scopes[scope].parent;
        }
        None
    }

    /// The innermost callable of `kinds` holding `span`.
    fn binder(&self, span: &Range<u32>, wanted: impl Fn(&Callable) -> bool) -> Option<usize> {
        self.callables
            .iter()
            .enumerate()
            .filter(|(_, callable)| within(span, &callable.row.span) && wanted(callable))
            .max_by_key(|(_, callable)| callable.row.span.start)
            .map(|(at, _)| at)
    }

    fn emit(mut self, out: &mut Out) {
        for at in 0..self.scopes.len() {
            if let Some(parent) = self.scopes[at].parent {
                let (child, holder, start) =
                    (self.scopes[at].product, self.scopes[parent].product, self.scopes[at].span.start);
                out.edge(child, PARENT_LABEL, holder, start);
            }
        }
        for decl in &mut self.decls {
            let owner = self.scopes[decl.owner].product;
            decl.edge = out.edge(owner, &decl.row.name, decl.target, decl.row.span.start);
        }
        for callable in &self.callables {
            if callable.row.kind == CallableKind::Method {
                if let Some(ty) = self.holding(&callable.row.span).and_then(|at| self.enclosing_type(at)) {
                    out.fact(
                        "tsi.input",
                        vec![Arg::Id(callable.id), Arg::Int(SELF_INPUT), Arg::Id(self.scopes[ty].product)],
                    );
                }
            }
            if let Some((span, text)) = &callable.row.output {
                let ty = out.named(text, span);
                out.fact("tsi.output", vec![Arg::Id(callable.id), Arg::Int(0), Arg::Id(ty)]);
            }
            let own = self.scopes.iter().position(|scope| scope.span == callable.row.span);
            let params = self
                .decls
                .iter()
                .filter(|decl| Some(decl.owner) == own && decl.row.kind == DeclKind::Param);
            for (position, param) in params.enumerate() {
                out.fact(
                    "tsi.input",
                    vec![Arg::Id(callable.id), Arg::Int(position as i64), Arg::Id(param.target)],
                );
            }
        }
        for at in 0..self.callables.len() {
            for field in self.env_of(at, out) {
                let env = self.callables[at].env;
                let edge = out.edge(env, &field.label, field.target, field.position);
                out.fact(
                    "tsi.capture",
                    vec![Arg::Id(edge), Arg::Id(field.source), Arg::Atom(field.mode.to_string())],
                );
            }
        }
        self.calls_and_escapes(out);
    }

    /// Every field `at`'s env holds, keyed by label, in first-use order.
    fn env_of(&self, at: usize, out: &mut Out) -> Vec<Field> {
        let callable = &self.callables[at];
        if callable.row.capture == Capture::Nothing {
            return Vec::new();
        }
        let span = &callable.row.span;
        let mut fields: Vec<Field> = Vec::new();
        let mut by_label: HashMap<String, usize> = HashMap::new();
        let mut put = |field: Field, fields: &mut Vec<Field>| match by_label.get(&field.label) {
            Some(&index) => {
                if field.mode == "exclusive" && fields[index].mode == "shared" {
                    fields[index].mode = "exclusive";
                }
            }
            None => {
                by_label.insert(field.label.clone(), fields.len());
                fields.push(field);
            }
        };
        for use_row in self.uses.iter().filter(|use_row| within(&use_row.span, span)) {
            match use_row.mode {
                UseMode::SelfRef => {
                    let Some(binder) = self.binder(&use_row.span, |other| {
                        matches!(other.row.kind, CallableKind::Function | CallableKind::Method)
                    }) else {
                        continue;
                    };
                    let bound = &self.callables[binder];
                    let crosses = binder == at && bound.row.kind == CallableKind::Method
                        || binder != at && within(span, &bound.row.span);
                    if !crosses {
                        continue;
                    }
                    let receiver = (bound.row.kind == CallableKind::Method)
                        .then(|| self.holding(&bound.row.span).and_then(|scope| self.enclosing_type(scope)))
                        .flatten()
                        .map(|scope| self.scopes[scope].product);
                    let target = receiver.unwrap_or_else(|| out.anonymous(&use_row.span));
                    put(
                        Field {
                            label: use_row.name.clone(),
                            position: use_row.span.start,
                            target,
                            source: target,
                            mode: "self",
                        },
                        &mut fields,
                    );
                }
                UseMode::Frame => {
                    let binder = if use_row.name == "await" {
                        self.binder(&use_row.span, |other| other.row.is_async)
                    } else {
                        self.binder(&use_row.span, |other| {
                            matches!(other.row.kind, CallableKind::Function | CallableKind::Method)
                        })
                    };
                    let crosses = match binder {
                        Some(binder) => binder != at && within(span, &self.callables[binder].row.span),
                        None => use_row.name == "await",
                    };
                    if !crosses {
                        continue;
                    }
                    let source = binder.map_or(callable.id, |binder| self.callables[binder].id);
                    let target = out.anonymous(&use_row.span);
                    put(
                        Field {
                            label: use_row.name.clone(),
                            position: use_row.span.start,
                            target,
                            source,
                            mode: "frame",
                        },
                        &mut fields,
                    );
                }
                _ => {
                    let Some(scope) = self.innermost(&use_row.span) else {
                        continue;
                    };
                    let Some(found) = self.resolve(&use_row.name, use_row.span.start, scope) else {
                        continue;
                    };
                    let decl = &self.decls[found];
                    let owner = &self.scopes[decl.owner];
                    if decl.row.kind == DeclKind::Item
                        || owner.kind == ScopeKind::Module
                        || within(&owner.span, span)
                    {
                        continue;
                    }
                    let field = match callable.row.capture {
                        Capture::Cell => Field {
                            label: decl.row.name.clone(),
                            position: decl.row.span.start,
                            target: self.cell_type(found, out),
                            source: decl.edge,
                            mode: "cell",
                        },
                        _ => Field {
                            label: decl.row.name.clone(),
                            position: decl.row.span.start,
                            target: decl.target,
                            source: decl.edge,
                            mode: match (callable.row.capture, decl.row.kind, use_row.mode) {
                                (_, DeclKind::SelfParam, _) => "self",
                                (Capture::Move, _, _) => "owned",
                                (_, _, UseMode::Exclusive) => "exclusive",
                                _ => "shared",
                            },
                        },
                    };
                    put(field, &mut fields);
                }
            }
        }
        fields
    }

    /// A cell's type is the union of its versions' types.
    fn cell_type(&self, cell: usize, out: &mut Out) -> u32 {
        let mut members: Vec<u32> = Vec::new();
        for decl in self.decls.iter().filter(|decl| decl.cell == cell) {
            if !members.contains(&decl.target) {
                members.push(decl.target);
            }
        }
        if members.len() == 1 {
            return members[0];
        }
        let sum = out.anonymous(&self.decls[cell].row.span);
        out.fact("tsi.sum", vec![Arg::Id(sum)]);
        for (position, member) in members.into_iter().enumerate() {
            out.edge(sum, &position.to_string(), member, position as u32);
        }
        sum
    }

    /// A call through a binding that holds a callable is `tsi.called`; any
    /// other mention of it is the callable escaping as a `tsi.value`.
    fn calls_and_escapes(&self, out: &mut Out) {
        for use_row in &self.uses {
            let Some(scope) = self.innermost(&use_row.span) else {
                continue;
            };
            let Some(found) = self.resolve(&use_row.name, use_row.span.start, scope) else {
                continue;
            };
            let target = self.decls[found].target;
            if !self.callables.iter().any(|callable| callable.id == target) {
                continue;
            }
            if use_row.mode == UseMode::Call {
                let result = out.anonymous(&use_row.span);
                let list = out.bare();
                out.fact("tsi.called", vec![Arg::Id(result), Arg::Id(target), Arg::Id(list)]);
            } else {
                let value = out.bare();
                out.fact("tsi.value", vec![Arg::Id(value), Arg::Id(target)]);
                out.origin(value, &use_row.span);
            }
        }
    }
}

/// The byte a declaration becomes visible at: a `let` after its initializer,
/// everything else across its whole scope.
fn visible_from(row: &DeclRow) -> u32 {
    match row.kind {
        DeclKind::Binding => row
            .value
            .as_ref()
            .map_or(row.span.end, |value| value.end.max(row.span.end)),
        _ => 0,
    }
}

/// The rows this reader adds, numbered past the ones already in the stream.
struct Out {
    facts: Vec<FactOut>,
    next_fact: u32,
    next_id: u32,
    lang: &'static str,
    named: HashMap<String, u32>,
}

impl Out {
    fn seeded(existing: &[FactOut], lang: &'static str) -> Self {
        let mut next_id = 0;
        let mut named = HashMap::new();
        for fact in existing {
            for arg in &fact.args {
                if let Arg::Id(id) = arg {
                    next_id = next_id.max(id + 1);
                }
            }
            match (fact.relation.as_str(), fact.args.as_slice()) {
                ("tsi.name", [Arg::Id(id), Arg::Text(text)]) => {
                    named.entry(text.clone()).or_insert(*id);
                }
                ("tsi.primitive", [Arg::Id(id), Arg::Atom(class)]) => {
                    let text = if class == "unit" { "()" } else { class.as_str() };
                    named.entry(text.to_string()).or_insert(*id);
                }
                _ => {}
            }
        }
        Self {
            facts: Vec::new(),
            next_fact: existing.iter().map(|fact| fact.fact + 1).max().unwrap_or(0),
            next_id,
            lang,
            named,
        }
    }

    fn fact(&mut self, relation: &'static str, args: Vec<Arg>) {
        debug_assert!(
            crate::read::tsi::registry::check(relation, &args).is_ok(),
            "{relation}: {:?}",
            crate::read::tsi::registry::check(relation, &args)
        );
        self.facts.push(FactOut {
            fact: self.next_fact,
            relation: relation.to_string(),
            args,
        });
        self.next_fact += 1;
    }

    fn bare(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn origin(&mut self, id: u32, span: &Range<u32>) {
        self.fact(
            "tsi.origin",
            vec![
                Arg::Id(id),
                Arg::Atom(self.lang.to_string()),
                Arg::Span(String::new(), span.start, span.end),
            ],
        );
    }

    fn anonymous(&mut self, span: &Range<u32>) -> u32 {
        let id = self.bare();
        self.fact("tsi.type", vec![Arg::Id(id)]);
        self.origin(id, span);
        id
    }

    fn named(&mut self, text: &str, span: &Range<u32>) -> u32 {
        if let Some(&id) = self.named.get(text) {
            return id;
        }
        let id = self.anonymous(span);
        self.fact("tsi.name", vec![Arg::Id(id), Arg::Text(text.to_string())]);
        self.named.insert(text.to_string(), id);
        id
    }

    fn edge(&mut self, owner: u32, label: &str, target: u32, position: u32) -> u32 {
        let edge = self.bare();
        self.fact(
            "tsi.edge",
            vec![
                Arg::Id(edge),
                Arg::Id(owner),
                Arg::Text(label.to_string()),
                Arg::Id(target),
                Arg::Int(position as i64),
            ],
        );
        edge
    }
}
