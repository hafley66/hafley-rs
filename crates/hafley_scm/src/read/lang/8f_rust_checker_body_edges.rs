//! The per-body call provider: one body in, the edges out of it. A `WalkSession`
//! owns the warm rust-analyzer host; inference runs only for the body asked about.

use super::*;
use std::sync::{Arc, Mutex};

use ra_ap_hir::DefWithBody;
use ra_ap_syntax::ast::HasArgList;
use ra_ap_syntax::WalkEvent;

use super::super::rust_checker_session::CheckerWorkspace;

/// One end of an edge. `path` is the supplied path, empty for a target outside
/// the session's files; `start` is the declaration name's byte.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WalkNode {
    pub path: String,
    pub name: String,
    pub start: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EdgeKind {
    /// `f(..)`, `a::b(..)`, `S { .. }`.
    Call,
    /// `recv.m(..)`, dispatched by inference of the calling body alone.
    Method,
    /// A project fn item handed as a value to an extern call.
    Passed,
    /// A project ADT handed to an extern call whose generic bound names trait
    /// T: the project's `impl T for ADT` methods.
    TraitImpl,
}

/// One edge out of a body. `to_body` is set for a project function: the body a
/// caller may ask about next.
#[derive(Clone, Debug)]
pub struct BodyEdge {
    pub to: WalkNode,
    pub to_body: Option<DefWithBody>,
    pub kind: EdgeKind,
    pub site_start: u32,
    pub site_end: u32,
}

#[derive(Clone, Debug)]
pub struct BodyEdges {
    pub from: WalkNode,
    pub edges: Vec<BodyEdge>,
}

/// The Slow-tier host plus the supplied files it answers paths for.
pub struct WalkSession {
    workspace: Arc<Mutex<CheckerWorkspace>>,
    /// Supplied files by rust-analyzer id: the project edges descend into.
    supplied: HashMap<ra_ap_ide::FileId, String>,
    navs: Mutex<HashMap<ModuleDef, Option<WalkNode>>>,
    pub load: Duration,
}

impl WalkSession {
    pub fn open(root: &Path, files: &[(String, PathBuf)], budget: Duration) -> Result<WalkSession, CheckerError> {
        let (workspace, load) = super::super::rust_checker_session::checker_workspace(
            root,
            super::super::rust_checker::Tier::Slow,
            files,
            budget,
        )?;
        let wanted: HashMap<PathBuf, &str> = files
            .iter()
            .map(|(supplied, absolute)| {
                let key = std::fs::canonicalize(absolute).unwrap_or_else(|_| absolute.clone());
                (key, supplied.as_str())
            })
            .collect();
        let mut supplied = HashMap::new();
        for (vfs_id, vfs_path) in workspace.lock().unwrap().vfs.iter() {
            let Some(absolute) = vfs_path.as_path() else {
                continue;
            };
            let text = absolute.to_string();
            let key = std::fs::canonicalize(&text).unwrap_or_else(|_| PathBuf::from(&text));
            if let Some(path) = wanted.get(&key) {
                supplied.insert(ra_ap_ide::FileId::from_raw(vfs_id.index()), (*path).to_string());
            }
        }
        Ok(WalkSession {
            workspace,
            supplied,
            navs: Mutex::new(HashMap::new()),
            load,
        })
    }

    /// The crate closure of `paths`: one def map per crate, built in parallel
    /// rather than on the first body's thread.
    pub fn prime(&self, paths: &[&str]) {
        let workspace = self.workspace.lock().unwrap();
        let ids: HashMap<String, ra_ap_ide::FileId> =
            self.supplied.iter().map(|(id, path)| (path.clone(), *id)).collect();
        super::target::prime_crate_closure(
            workspace.host.raw_database(),
            &ids,
            paths,
            &[],
            crate::read::project::extract_pool().current_num_threads(),
        );
    }

    /// Every function named `name` the file's own syntax declares; `None` when
    /// the file is unsupplied or owns no module in the crate graph.
    pub fn seeds(&self, path: &str, name: &str) -> Option<Vec<DefWithBody>> {
        let file = *self.supplied.iter().find(|(_, supplied)| supplied.as_str() == path)?.0;
        self.with_sema(|sema| {
            sema.file_to_module_defs(file).next()?;
            Some(
                sema.parse_guess_edition(file)
                    .syntax()
                    .descendants()
                    .filter_map(ast::Fn::cast)
                    .filter(|item| item.name().is_some_and(|written| written.text() == name))
                    .filter_map(|item| sema.to_def(&item))
                    .map(DefWithBody::from)
                    .collect(),
            )
        })
    }

    /// The provider: resolve every call site of one body. `None` when the body
    /// has no syntax (a required trait method, a derive) or no navigation target.
    pub fn body_edges(&self, body: DefWithBody) -> Option<BodyEdges> {
        self.with_sema(|sema| {
            let mut navs = self.navs.lock().unwrap();
            let mut resolve = Resolve {
                sema,
                supplied: &self.supplied,
                navs: &mut navs,
                edges: Vec::new(),
            };
            let from = resolve.node(module_def(body))?;
            let root = body_syntax(sema, body)?;
            let _span = tracing::debug_span!("rust_walk.body").entered();
            resolve.sites(&root);
            Some(BodyEdges { from, edges: resolve.edges })
        })
    }

    fn with_sema<T>(&self, work: impl FnOnce(&Semantics<'_, RootDatabase>) -> T) -> T {
        let workspace = self.workspace.lock().unwrap();
        let db = workspace.host.raw_database();
        attach_db(db, || work(&Semantics::new(db)))
    }
}

fn module_def(body: DefWithBody) -> ModuleDef {
    match body {
        DefWithBody::Function(item) => ModuleDef::Function(item),
        DefWithBody::Static(item) => ModuleDef::Static(item),
        DefWithBody::Const(item) => ModuleDef::Const(item),
        DefWithBody::EnumVariant(item) => ModuleDef::EnumVariant(item),
    }
}

/// The body's own syntax, rooted in the semantics cache so its sites resolve.
fn body_syntax(sema: &Semantics<'_, RootDatabase>, body: DefWithBody) -> Option<ra_ap_syntax::SyntaxNode> {
    match body {
        DefWithBody::Function(item) => Some(sema.source(item)?.value.body()?.syntax().clone()),
        DefWithBody::Static(item) => Some(sema.source(item)?.value.body()?.syntax().clone()),
        DefWithBody::Const(item) => Some(sema.source(item)?.value.body()?.syntax().clone()),
        DefWithBody::EnumVariant(item) => Some(sema.source(item)?.value.const_arg()?.syntax().clone()),
    }
}

struct Resolve<'s, 'db> {
    sema: &'s Semantics<'db, RootDatabase>,
    supplied: &'s HashMap<ra_ap_ide::FileId, String>,
    navs: &'s mut HashMap<ModuleDef, Option<WalkNode>>,
    edges: Vec<BodyEdge>,
}

impl Resolve<'_, '_> {
    fn node(&mut self, def: ModuleDef) -> Option<WalkNode> {
        if let Some(known) = self.navs.get(&def) {
            return known.clone();
        }
        let node = nav_of(self.sema, def).map(|nav| match self.supplied.get(&nav.file_id) {
            Some(path) => WalkNode {
                path: path.clone(),
                name: nav.name.as_str().to_string(),
                start: u32::from(nav.focus_range.unwrap_or(nav.full_range).start()),
            },
            None => WalkNode {
                path: String::new(),
                name: qualified(self.sema, def).unwrap_or_else(|| nav.name.as_str().to_string()),
                start: 0,
            },
        });
        self.navs.insert(def, node.clone());
        node
    }

    /// Declared in a supplied file of a workspace-member crate.
    fn in_project(&mut self, def: ModuleDef) -> bool {
        let db = self.sema.db;
        let local = def.module(db).is_some_and(|module| module.krate(db).origin(db).is_local());
        local && self.node(def).is_some_and(|node| !node.path.is_empty())
    }

    fn push(&mut self, def: ModuleDef, kind: EdgeKind, site: ra_ap_syntax::TextRange) {
        let Some(to) = self.node(def) else {
            return;
        };
        let to_body = match def {
            ModuleDef::Function(item) if !to.path.is_empty() => Some(DefWithBody::Function(item)),
            _ => None,
        };
        tracing::debug!(target: "rust_walk.edge", to_path = %to.path, to_name = %to.name, kind = ?kind);
        self.edges.push(BodyEdge {
            to,
            to_body,
            kind,
            site_start: u32::from(site.start()),
            site_end: u32::from(site.end()),
        });
    }

    /// Nested `fn` items are their own bodies; closures belong to the body that
    /// writes them. Macro invocations are not expanded.
    fn sites(&mut self, root: &ra_ap_syntax::SyntaxNode) {
        let mut preorder = root.preorder();
        while let Some(event) = preorder.next() {
            let WalkEvent::Enter(node) = event else {
                continue;
            };
            if ast::Fn::can_cast(node.kind()) {
                preorder.skip_subtree();
                continue;
            }
            if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
                let (Some(name), Some(function)) = (call.name_ref(), self.sema.resolve_method_call(&call)) else {
                    continue;
                };
                self.callee(ModuleDef::Function(function), EdgeKind::Method, name.syntax().text_range(), call.arg_list());
                continue;
            }
            let path = if let Some(call) = ast::CallExpr::cast(node.clone()) {
                match call.expr() {
                    Some(ast::Expr::PathExpr(expr)) => expr.path().map(|path| (path, call.arg_list())),
                    _ => None,
                }
            } else {
                ast::RecordExpr::cast(node).and_then(|record| record.path()).map(|path| (path, None))
            };
            let Some((path, args)) = path else {
                continue;
            };
            let Some(name) = path.segment().and_then(|segment| segment.name_ref()) else {
                continue;
            };
            let Some(PathResolution::Def(def)) = self.sema.resolve_path(&path) else {
                continue;
            };
            if matches!(def, ModuleDef::Module(_) | ModuleDef::BuiltinType(_)) {
                continue;
            }
            self.callee(def, EdgeKind::Call, name.syntax().text_range(), args);
        }
    }

    /// One resolved site. An extern callee's arguments are read for `Passed`
    /// and `TraitImpl` edges.
    fn callee(&mut self, def: ModuleDef, kind: EdgeKind, site: ra_ap_syntax::TextRange, args: Option<ast::ArgList>) {
        let project = self.in_project(def);
        self.push(def, kind, site);
        let (false, ModuleDef::Function(function), Some(args)) = (project, def, args) else {
            return;
        };
        let db = self.sema.db;
        let bounds: Vec<Trait> = GenericDef::from(function)
            .type_or_const_params(db)
            .into_iter()
            .filter_map(|param| param.as_type_param(db))
            .flat_map(|param| param.trait_bounds(db))
            .collect();
        for arg in args.args() {
            let arg_site = arg.syntax().text_range();
            if let ast::Expr::PathExpr(expr) = &arg {
                if let Some(PathResolution::Def(item @ ModuleDef::Function(_))) =
                    expr.path().and_then(|path| self.sema.resolve_path(&path))
                {
                    if self.in_project(item) {
                        self.push(item, EdgeKind::Passed, arg_site);
                    }
                }
            }
            if bounds.is_empty() {
                continue;
            }
            let Some(adt) = self.sema.type_of_expr(&arg).and_then(|ty| ty.original.strip_references().as_adt()) else {
                continue;
            };
            if !self.in_project(ModuleDef::Adt(adt)) {
                continue;
            }
            for item in Impl::all_for_type(db, adt.ty(db)) {
                if !item.trait_(db).is_some_and(|contract| bounds.contains(&contract)) {
                    continue;
                }
                for assoc in item.items(db) {
                    if let AssocItem::Function(method) = assoc {
                        if self.in_project(ModuleDef::Function(method)) {
                            self.push(ModuleDef::Function(method), EdgeKind::TraitImpl, arg_site);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_body_in_its_edges_out() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn seed() -> u8 { let v = vec![1u8]; v.len() as u8 + helper() }\nfn helper() -> u8 { other() }\nfn other() -> u8 { 2 }\n",
        )
        .unwrap();
        let files = vec![("src/lib.rs".to_string(), root.join("src/lib.rs"))];
        let session = WalkSession::open(&root, &files, Duration::from_secs(120)).unwrap();
        let seed = session.seeds("src/lib.rs", "seed").unwrap();
        let out = session.body_edges(seed[0]).unwrap();
        let edges: Vec<(String, EdgeKind, bool)> = out
            .edges
            .iter()
            .map(|edge| (edge.to.name.clone(), edge.kind, edge.to_body.is_some()))
            .collect();
        assert_eq!(out.from.name, "seed");
        assert_eq!(
            edges,
            [
                ("alloc::vec::Vec::len".to_string(), EdgeKind::Method, false),
                ("helper".to_string(), EdgeKind::Call, true),
            ]
        );
        assert_eq!(session.seeds("src/lib.rs", "absent"), Some(Vec::new()));
    }
}
