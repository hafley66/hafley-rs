//! Cross-crate `extract move`: the module tree, paths, visibility and manifests
//! follow a file into another Cargo package; a dependency cycle is a plan error.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use syn::spanned::Spanned;

use super::super::rust::{build_line_starts, syn_span};
use super::super::rust_modules::CargoManifest;
use super::{
    crate_roots, insert_decls, is_manifest, line_after, natural_paths, relocate_scan, slice,
    whole_lines, FileScan, RelocateEdit, RelocatePlan, Relocation, SegRun, MOD_PATH,
    MOD_RELOCATE_OUT, WIDEN_VIS,
};
use crate::manifests::{fold_package_edges, Manifest, ManifestKind};
use crate::move_cx::{dirname, join_rel, relative_between, stem, MoveCx};
use crate::project::extract_pool;
use crate::types::{LangKind, Span};
use crate::wire::FlatFact;
use crate::edit_seams::ImportRefKind;

/// A manifest edit: one dependency line (or table) a package gains.
pub const CARGO_DEP: ImportRefKind = ImportRefKind::Ext(LangKind {
    lang: "rust",
    tag: "cargo_dep",
});

/// Crate names every crate reaches with no manifest entry.
const SYSROOT: [&str; 5] = ["std", "core", "alloc", "proc_macro", "test"];

// ── packages ────────────────────────────────────────────────────────────────

/// One Cargo package in the corpus.
struct Package {
    dir: String,
    manifest: String,
    /// `[package] name` as a dependency table spells it.
    name: String,
    /// The identifier its library answers to in a path.
    ident: String,
    /// The library root, when the package has one in the corpus.
    lib: Option<String>,
    text: String,
}

fn packages(cx: &MoveCx) -> Vec<Package> {
    cx.files()
        .iter()
        .filter(|rel| is_manifest(rel))
        .filter_map(|manifest| {
            let text = cx.text(manifest)?;
            let parsed = CargoManifest::parse(&text)?;
            let name = parsed.package_name()?;
            let ident = parsed.ident()?;
            let dir = dirname(manifest).to_string();
            let lib = join_rel(&dir, &parsed.lib_path());
            Some(Package {
                lib: cx.contains(&lib).then_some(lib),
                dir,
                manifest: manifest.clone(),
                name,
                ident,
                text,
            })
        })
        .collect()
}

/// The deepest package directory holding `rel`.
fn package_of<'a>(packages: &'a [Package], rel: &str) -> Option<&'a Package> {
    packages
        .iter()
        .filter(|package| package.dir.is_empty() || rel.starts_with(&format!("{}/", package.dir)))
        .max_by_key(|package| package.dir.len())
}

/// The Cargo package holding `rel`: (dir, package name, crate ident, dependency keys).
pub fn cargo_package(
    cx: &MoveCx,
    rel: &str,
) -> Option<(String, String, String, BTreeSet<String>)> {
    let packages = packages(cx);
    let package = package_of(&packages, rel)?;
    let deps = dep_specs(&package.text)
        .into_iter()
        .map(|spec| spec.key)
        .collect();
    Some((
        package.dir.clone(),
        package.name.clone(),
        package.ident.clone(),
        deps,
    ))
}

/// Whether this batch carries any Rust file across a package boundary.
pub(super) fn active(cx: &MoveCx) -> bool {
    let moved: Vec<(&String, &String)> = cx
        .moved()
        .iter()
        .filter(|(old, _)| old.ends_with(".rs"))
        .collect();
    if moved.is_empty() {
        return false;
    }
    let packages = packages(cx);
    moved.iter().any(|(old, new)| {
        match (package_of(&packages, old), package_of(&packages, new)) {
            (Some(from), Some(to)) => from.dir != to.dir,
            _ => false,
        }
    })
}

// ── the per-file scan cross mode adds ───────────────────────────────────────

/// One name a `use` tree binds: the idents as written with their spans, the
/// `as` alias, and whether it ends in a glob.
struct UseLeaf {
    idents: Vec<String>,
    spans: Vec<Span>,
    alias: Option<String>,
    glob: bool,
}

/// One `use` item, from its visibility to its semicolon.
struct UseDecl {
    span: Span,
    vis: String,
    /// The inline `mod` blocks enclosing it, outermost first.
    chain: Vec<String>,
    /// Written inside a fn body (its bindings scope that body only).
    in_fn: bool,
    grouped: bool,
    leaves: Vec<UseLeaf>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisKind {
    Item,
    Method,
    Field,
}

/// One declaration whose visibility this move may widen to `pub`.
struct VisItem {
    chain: Vec<String>,
    /// The impl self type for a method, the struct for a field.
    owner: Option<String>,
    name: String,
    kind: VisKind,
    /// Already `pub`: nothing to write.
    public: bool,
    /// The bytes that become `pub`: a restricted visibility, or a zero-length
    /// insertion ahead of the item's first token.
    span: Span,
    text: String,
}

struct Extras {
    uses: Vec<UseDecl>,
    vis: Vec<VisItem>,
}

struct Scanned {
    rel: String,
    text: String,
    scan: FileScan,
    runs: Vec<SegRun>,
    extras: Extras,
}

fn extras(text: &str) -> Option<Extras> {
    let parsed = syn::parse_file(text).ok()?;
    let line_starts = build_line_starts(text);
    let mut walk = ExtraWalk {
        source: text,
        line_starts: &line_starts,
        chain: Vec::new(),
        fn_depth: 0,
        out: Extras {
            uses: Vec::new(),
            vis: Vec::new(),
        },
    };
    syn::visit::Visit::visit_file(&mut walk, &parsed);
    Some(walk.out)
}

struct ExtraWalk<'a> {
    source: &'a str,
    line_starts: &'a [u32],
    chain: Vec<String>,
    fn_depth: usize,
    out: Extras,
}

impl ExtraWalk<'_> {
    fn span_of(&self, span: proc_macro2::Span) -> Span {
        syn_span(self.line_starts, span)
    }

    /// The first offset past `attrs` and whitespace: where a `pub ` insertion
    /// lands for a private item.
    fn after_attrs(&self, item: proc_macro2::Span, attrs: &[syn::Attribute]) -> u32 {
        let bytes = self.source.as_bytes();
        let mut offset = self.span_of(item).start as usize;
        for attr in attrs {
            let end = self.span_of(attr.span());
            offset = offset.max(end.start as usize + end.len as usize);
        }
        while offset < self.source.len() && bytes[offset].is_ascii_whitespace() {
            offset += 1;
        }
        offset as u32
    }

    #[allow(clippy::too_many_arguments)]
    fn push_vis(
        &mut self,
        vis: &syn::Visibility,
        whole: proc_macro2::Span,
        attrs: &[syn::Attribute],
        name: String,
        owner: Option<String>,
        kind: VisKind,
        first_word: &str,
    ) {
        if self.fn_depth > 0 {
            return;
        }
        let (public, span, text) = match vis {
            syn::Visibility::Public(_) => (true, Span::anchor(0), String::new()),
            syn::Visibility::Restricted(_) => {
                let span = self.span_of(vis.span());
                match slice(self.source, span) {
                    Some(written) if written.starts_with("pub") => {
                        (false, span, "pub".to_string())
                    }
                    _ => return,
                }
            }
            syn::Visibility::Inherited => {
                let at = self.after_attrs(whole, attrs);
                let honest = self
                    .source
                    .get(at as usize..)
                    .is_some_and(|tail| first_word.is_empty() || tail.starts_with(first_word));
                if !honest {
                    return;
                }
                (false, Span::anchor(at), "pub ".to_string())
            }
        };
        self.out.vis.push(VisItem {
            chain: self.chain.clone(),
            owner,
            name,
            kind,
            public,
            span,
            text,
        });
    }
}

impl<'ast> syn::visit::Visit<'ast> for ExtraWalk<'_> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.push_vis(
            &node.vis,
            node.span(),
            &node.attrs,
            node.ident.to_string(),
            None,
            VisKind::Item,
            "",
        );
        if let Some((_, items)) = &node.content {
            self.chain.push(node.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.chain.pop();
        }
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.push_vis(
            &node.vis,
            node.span(),
            &node.attrs,
            node.sig.ident.to_string(),
            None,
            VisKind::Item,
            "",
        );
        self.fn_depth += 1;
        syn::visit::visit_item_fn(self, node);
        self.fn_depth -= 1;
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        let name = node.ident.to_string();
        self.push_vis(&node.vis, node.span(), &node.attrs, name.clone(), None, VisKind::Item, "");
        if let syn::Fields::Named(fields) = &node.fields {
            for field in &fields.named {
                let Some(ident) = &field.ident else { continue };
                self.push_vis(
                    &field.vis,
                    field.span(),
                    &field.attrs,
                    ident.to_string(),
                    Some(name.clone()),
                    VisKind::Field,
                    &ident.to_string(),
                );
            }
        }
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.push_vis(&node.vis, node.span(), &node.attrs, node.ident.to_string(), None, VisKind::Item, "");
    }

    fn visit_item_union(&mut self, node: &'ast syn::ItemUnion) {
        self.push_vis(&node.vis, node.span(), &node.attrs, node.ident.to_string(), None, VisKind::Item, "");
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.push_vis(&node.vis, node.span(), &node.attrs, node.ident.to_string(), None, VisKind::Item, "");
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.push_vis(&node.vis, node.span(), &node.attrs, node.ident.to_string(), None, VisKind::Item, "");
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.push_vis(&node.vis, node.span(), &node.attrs, node.ident.to_string(), None, VisKind::Item, "");
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.push_vis(&node.vis, node.span(), &node.attrs, node.ident.to_string(), None, VisKind::Item, "");
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        // A trait impl's methods carry the trait's visibility, never their own.
        if node.trait_.is_none() && self.fn_depth == 0 {
            let owner = match &*node.self_ty {
                syn::Type::Path(path) => path.path.segments.last().map(|seg| seg.ident.to_string()),
                _ => None,
            };
            for item in &node.items {
                if let syn::ImplItem::Fn(method) = item {
                    self.push_vis(
                        &method.vis,
                        method.span(),
                        &method.attrs,
                        method.sig.ident.to_string(),
                        owner.clone(),
                        VisKind::Method,
                        "",
                    );
                }
            }
        }
        self.fn_depth += 1;
        syn::visit::visit_item_impl(self, node);
        self.fn_depth -= 1;
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        if node.leading_colon.is_some() {
            return;
        }
        let vis = match &node.vis {
            syn::Visibility::Inherited => String::new(),
            written => slice(self.source, self.span_of(written.span()))
                .filter(|text| text.starts_with("pub"))
                .map(|text| format!("{text} "))
                .unwrap_or_else(|| "pub ".to_string()),
        };
        let start = match &node.vis {
            syn::Visibility::Inherited => self.span_of(node.use_token.span).start,
            written => self.span_of(written.span()).start,
        };
        let semi = self.span_of(node.semi_token.span);
        let span = Span {
            start,
            len: semi.start + semi.len - start,
        };
        let mut leaves = Vec::new();
        let mut grouped = false;
        use_leaves(
            &node.tree,
            self.line_starts,
            &mut Vec::new(),
            &mut leaves,
            &mut grouped,
        );
        self.out.uses.push(UseDecl {
            span,
            vis,
            chain: self.chain.clone(),
            in_fn: self.fn_depth > 0,
            grouped,
            leaves,
        });
    }
}

fn use_leaves(
    tree: &syn::UseTree,
    line_starts: &[u32],
    prefix: &mut Vec<(String, Span)>,
    out: &mut Vec<UseLeaf>,
    grouped: &mut bool,
) {
    let mut emit = |prefix: &Vec<(String, Span)>,
                    leaf: Option<(String, Span)>,
                    alias: Option<String>,
                    glob: bool| {
        let mut idents: Vec<String> = prefix.iter().map(|(ident, _)| ident.clone()).collect();
        let mut spans: Vec<Span> = prefix.iter().map(|(_, span)| *span).collect();
        if let Some((ident, span)) = leaf {
            idents.push(ident);
            spans.push(span);
        }
        out.push(UseLeaf {
            idents,
            spans,
            alias,
            glob,
        });
    };
    match tree {
        syn::UseTree::Path(segment) => {
            prefix.push((
                segment.ident.to_string(),
                syn_span(line_starts, segment.ident.span()),
            ));
            use_leaves(&segment.tree, line_starts, prefix, out, grouped);
            prefix.pop();
        }
        syn::UseTree::Group(group) => {
            *grouped = true;
            for member in &group.items {
                use_leaves(member, line_starts, prefix, out, grouped);
            }
        }
        syn::UseTree::Name(leaf) => emit(
            prefix,
            Some((leaf.ident.to_string(), syn_span(line_starts, leaf.ident.span()))),
            None,
            false,
        ),
        syn::UseTree::Rename(leaf) => emit(
            prefix,
            Some((leaf.ident.to_string(), syn_span(line_starts, leaf.ident.span()))),
            Some(leaf.rename.to_string()),
            false,
        ),
        syn::UseTree::Glob(_) => emit(prefix, None, None, true),
    }
}

// ── the module tree ─────────────────────────────────────────────────────────

/// Where one file sits in its crate: the crate root, the module path, whether
/// it owns its directory for child decls, and the decl that declares it.
#[derive(Clone)]
struct Node {
    root: String,
    path: Vec<String>,
    owns_dir: bool,
    decl: Option<(String, usize)>,
}

/// The directory a file's natural child decls resolve against.
fn child_dir(rel: &str, owns_dir: bool) -> String {
    match owns_dir {
        true => dirname(rel).to_string(),
        false => join_rel(dirname(rel), &stem(rel)),
    }
}

/// Every file reachable from a crate root through `mod` decls. A file loaded by
/// `#[path]` owns its directory the way a `mod.rs` does (rustc's rule).
fn module_tree(roots: &[String], scans: &BTreeMap<String, &Scanned>) -> BTreeMap<String, Node> {
    let mut tree: BTreeMap<String, Node> = BTreeMap::new();
    for root in roots {
        if tree.contains_key(root) || !scans.contains_key(root) {
            continue;
        }
        tree.insert(
            root.clone(),
            Node {
                root: root.clone(),
                path: Vec::new(),
                owns_dir: true,
                decl: None,
            },
        );
        let mut pending = vec![root.clone()];
        while let Some(rel) = pending.pop() {
            let Some(scanned) = scans.get(&rel) else { continue };
            let node = tree[&rel].clone();
            for (index, decl) in scanned.scan.decls.iter().enumerate() {
                let base = decl
                    .chain
                    .iter()
                    .fold(child_dir(&rel, node.owns_dir), |dir, block| join_rel(&dir, block));
                let target = match &decl.attr {
                    Some((_, value)) if decl.chain.is_empty() => {
                        Some(join_rel(dirname(&rel), value))
                    }
                    Some((_, value)) => Some(join_rel(&base, value)),
                    None => natural_paths(&base, &decl.name)
                        .into_iter()
                        .find(|candidate| scans.contains_key(candidate)),
                };
                let Some(target) = target.filter(|target| scans.contains_key(target)) else {
                    continue;
                };
                if tree.contains_key(&target) {
                    continue;
                }
                let mut path = node.path.clone();
                path.extend(decl.chain.iter().cloned());
                path.push(decl.name.clone());
                tree.insert(
                    target.clone(),
                    Node {
                        root: node.root.clone(),
                        path,
                        owns_dir: decl.attr.is_some() || stem(&target) == "mod",
                        decl: Some((rel.clone(), index)),
                    },
                );
                pending.push(target);
            }
        }
    }
    tree
}

// ── path resolution ─────────────────────────────────────────────────────────

/// A written path's reading: its crate, the absolute path of every ident, the
/// idents the qualifier ate, and the length of the qualifier's own base path.
#[derive(Clone)]
struct Reading {
    root: String,
    abs: Vec<String>,
    eaten: usize,
    base: usize,
    via_use: bool,
}

/// What a file sees: its crate, its module, its child modules, its `use`
/// bindings, and the library idents of every package.
struct Scope<'a> {
    root: &'a str,
    here: &'a [String],
    children: &'a BTreeSet<String>,
    bindings: &'a BTreeMap<String, (String, Vec<String>)>,
    idents: &'a BTreeMap<String, String>,
}

fn read_path(
    scope: &Scope,
    idents: &[String],
    chain: &[String],
    relative_ok: bool,
    from_use: bool,
) -> Option<Reading> {
    let first = idents.first()?.as_str();
    let here: Vec<String> = scope.here.iter().chain(chain.iter()).cloned().collect();
    let reading = |root: &str, base: Vec<String>, eaten: usize, via_use: bool| {
        let base_len = base.len();
        let mut abs = base;
        abs.extend(idents[eaten..].iter().cloned());
        Reading {
            root: root.to_string(),
            abs,
            eaten,
            base: base_len,
            via_use,
        }
    };
    match first {
        "crate" => Some(reading(scope.root, Vec::new(), 1, false)),
        "self" if relative_ok => Some(reading(scope.root, here, 1, false)),
        "super" if relative_ok => {
            let steps = idents.iter().take_while(|ident| *ident == "super").count();
            let base = here.get(..here.len().checked_sub(steps)?)?.to_vec();
            Some(reading(scope.root, base, steps, false))
        }
        "self" | "super" | "Self" => None,
        name if relative_ok && chain.is_empty() && scope.children.contains(name) => {
            Some(reading(scope.root, here, 0, false))
        }
        name if !from_use && relative_ok && chain.is_empty() => {
            if let Some((root, bound)) = scope.bindings.get(name) {
                return Some(reading(root, bound.clone(), 1, true));
            }
            let root = scope.idents.get(name).filter(|root| *root != scope.root)?;
            Some(reading(root, Vec::new(), 1, false))
        }
        name => {
            // A crate never names itself by its package ident.
            let root = scope.idents.get(name).filter(|root| *root != scope.root)?;
            Some(reading(root, Vec::new(), 1, false))
        }
    }
}

// ── the plan ────────────────────────────────────────────────────────────────

/// One moved module: where it was, where it lands, and how its decl travels.
struct Moved {
    old: String,
    new: String,
    name: String,
    old_root: String,
    old_path: Vec<String>,
    new_root: String,
    new_path: Vec<String>,
    /// The decl leaves its parent (true) or travels inside it (false).
    lift: bool,
    /// Pre-move rel of the file the decl lands in, and its post-move rel.
    parent_pre: String,
    parent_after: String,
    aim: String,
    vis: String,
    decl_item: Option<(String, Span, String)>,
}

pub(super) fn build(cx: &MoveCx) -> RelocatePlan {
    let mut plan = RelocatePlan::default();
    let packages = packages(cx);
    let roots_set = crate_roots(cx);
    let scanned_vec = scan_all(cx);
    let scans: BTreeMap<String, &Scanned> = scanned_vec
        .iter()
        .map(|scanned| (scanned.rel.clone(), scanned))
        .collect();
    let mut roots: Vec<String> = roots_set.iter().cloned().collect();
    roots.sort_by_key(|root| (!root.ends_with("src/lib.rs"), root.clone()));
    let tree = module_tree(&roots, &scans);

    let idents: BTreeMap<String, String> = packages
        .iter()
        .filter_map(|package| Some((package.ident.clone(), package.lib.clone()?)))
        .collect();
    let lib_package: BTreeMap<&str, &Package> = packages
        .iter()
        .filter_map(|package| Some((package.lib.as_deref()?, package)))
        .collect();

    let moved = match place_moves(cx, &tree, &scans, &mut plan) {
        Some(moved) => moved,
        None => return plan,
    };
    if !plan.errors.is_empty() {
        return plan;
    }
    let by_old: BTreeMap<&str, &Moved> = moved.iter().map(|one| (one.old.as_str(), one)).collect();
    let prefix_map: BTreeMap<(String, Vec<String>), (String, Vec<String>)> = moved
        .iter()
        .map(|one| {
            (
                (one.old_root.clone(), one.old_path.clone()),
                (one.new_root.clone(), one.new_path.clone()),
            )
        })
        .collect();
    let map_path = |root: &str, path: &[String]| -> (String, Vec<String>) {
        for cut in (1..=path.len()).rev() {
            if let Some((new_root, new_path)) =
                prefix_map.get(&(root.to_string(), path[..cut].to_vec()))
            {
                let mut out = new_path.clone();
                out.extend(path[cut..].iter().cloned());
                return (new_root.clone(), out);
            }
        }
        (root.to_string(), path.to_vec())
    };

    let is_moved = |root: &str, path: &[String]| {
        prefix_map.contains_key(&(root.to_string(), path.to_vec()))
    };

    // Every file's position once the batch lands, and the reverse index.
    let after_of = |rel: &str| -> Option<(String, Vec<String>)> {
        match by_old.get(rel) {
            Some(one) => Some((one.new_root.clone(), one.new_path.clone())),
            None => tree.get(rel).map(|node| (node.root.clone(), node.path.clone())),
        }
    };
    let mut file_at: BTreeMap<(String, Vec<String>), String> = BTreeMap::new();
    for rel in tree.keys() {
        if let Some(position) = after_of(rel) {
            file_at.insert(position, rel.clone());
        }
    }

    let lifted: BTreeSet<(String, String)> = moved
        .iter()
        .filter(|one| one.lift)
        .map(|one| (one.decl_item.as_ref().map(|d| d.0.clone()).unwrap_or_default(), one.name.clone()))
        .collect();

    // Children each file declares, before and after.
    let children_before: BTreeMap<&str, BTreeSet<String>> = scans
        .iter()
        .map(|(rel, scanned)| {
            (
                rel.as_str(),
                scanned
                    .scan
                    .decls
                    .iter()
                    .filter(|decl| decl.chain.is_empty())
                    .map(|decl| decl.name.clone())
                    .collect(),
            )
        })
        .collect();
    let mut children_after: BTreeMap<String, BTreeSet<String>> = children_before
        .iter()
        .map(|(rel, names)| {
            (
                rel.to_string(),
                names
                    .iter()
                    .filter(|name| !lifted.contains(&(rel.to_string(), (*name).clone())))
                    .cloned()
                    .collect(),
            )
        })
        .collect();
    for one in moved.iter().filter(|one| one.lift) {
        children_after
            .entry(one.parent_pre.clone())
            .or_default()
            .insert(one.name.clone());
    }

    let empty_bindings: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    let mut crossings: Vec<Crossing> = Vec::new();
    let mut externals: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (rel, scanned) in &scans {
        let Some(node) = tree.get(rel) else { continue };
        let Some((after_root, after_here)) = after_of(rel) else { continue };
        let moving = by_old.contains_key(rel.as_str());
        let before_children = &children_before[rel.as_str()];
        let after_children = &children_after[rel];
        let bindings = bindings_of(scanned, node, before_children, &idents);
        let before = Scope {
            root: &node.root,
            here: &node.path,
            children: before_children,
            bindings: &bindings,
            idents: &idents,
        };
        let after = Scope {
            root: &after_root,
            here: &after_here,
            children: after_children,
            bindings: &empty_bindings,
            idents: &idents,
        };
        let text = &scanned.text;

        // `use` items: a leaf edit, or the whole item rewritten when it groups.
        for decl in &scanned.extras.uses {
            let relative_ok = !decl.in_fn || decl.chain.is_empty();
            let mut outcomes: Vec<Option<(usize, String)>> = Vec::new();
            for leaf in &decl.leaves {
                let reading = read_path(&before, &leaf.idents, &decl.chain, relative_ok, true);
                let outcome = reading.as_ref().and_then(|reading| {
                    note_crossing(
                        &mut crossings,
                        rel,
                        &after_root,
                        moving,
                        reading,
                        &map_path,
                    );
                    respell_reading(
                        reading,
                        &leaf.idents,
                        &decl.chain,
                        true,
                        moving,
                        &after,
                        &map_path,
                        &is_moved,
                        &lib_package,
                        &mut plan.errors,
                        rel,
                    )
                });
                if reading.is_none() && moving {
                    if let Some(first) = leaf.idents.first() {
                        externals.entry(rel.clone()).or_default().insert(first.clone());
                    }
                }
                outcomes.push(outcome);
            }
            if outcomes.iter().all(Option::is_none) {
                continue;
            }
            if decl.grouped || decl.leaves.iter().zip(&outcomes).any(|(leaf, outcome)| {
                outcome.as_ref().is_some_and(|(cut, _)| !contiguous(text, &leaf.spans[..*cut]))
            }) {
                let replacement = rewrite_use(decl, &outcomes, text);
                plan.edits.insert(
                    (rel.clone(), decl.span.start),
                    RelocateEdit {
                        importer: rel.clone(),
                        span: decl.span,
                        text: slice(text, decl.span).unwrap_or_default(),
                        target: rel.clone(),
                        kind: MOD_PATH,
                        replacement,
                        receipt: None,
                    },
                );
                continue;
            }
            for (leaf, outcome) in decl.leaves.iter().zip(outcomes) {
                let Some((cut, replacement)) = outcome else { continue };
                push_run_edit(&mut plan, rel, text, &leaf.spans[..cut], replacement);
            }
        }

        // Expression and type paths.
        for run in scanned.runs.iter().filter(|run| !run.from_use) {
            let relative_ok = !run.in_block;
            // `pub(crate)` / `pub(super)` walk as a bare qualifier path.
            if run.idents.iter().all(|ident| matches!(ident.as_str(), "crate" | "self" | "super")) {
                continue;
            }
            let Some(reading) = read_path(&before, &run.idents, &[], relative_ok, false) else {
                if moving && !run.in_block {
                    if let Some(first) = run.idents.first() {
                        if run.idents.len() > 1 {
                            externals.entry(rel.clone()).or_default().insert(first.clone());
                        }
                    }
                }
                continue;
            };
            note_crossing(&mut crossings, rel, &after_root, moving, &reading, &map_path);
            if reading.via_use {
                continue;
            }
            let Some((cut, replacement)) = respell_reading(
                &reading,
                &run.idents,
                &[],
                false,
                moving,
                &after,
                &map_path,
                &is_moved,
                &lib_package,
                &mut plan.errors,
                rel,
            ) else {
                continue;
            };
            if !contiguous(text, &run.spans[..cut]) {
                continue;
            }
            push_run_edit(&mut plan, rel, text, &run.spans[..cut], replacement);
        }
    }

    // Lifted decls leave their parents; the new parents gain theirs.
    let mut relocations: BTreeMap<String, Relocation> = BTreeMap::new();
    for one in moved.iter().filter(|one| one.lift) {
        let Some((parent, span, text)) = &one.decl_item else { continue };
        plan.edits.insert(
            (parent.clone(), span.start),
            RelocateEdit {
                importer: parent.clone(),
                span: *span,
                text: text.clone(),
                target: one.old.clone(),
                kind: MOD_RELOCATE_OUT,
                replacement: String::new(),
                receipt: None,
            },
        );
        relocations.insert(
            one.old.clone(),
            Relocation {
                name: one.name.clone(),
                old_path: one.old_path.clone(),
                new_path: one.new_path.clone(),
                old_parent: parent.clone(),
                new_parent: one.parent_pre.clone(),
                decl: *span,
                decl_text: text.clone(),
                vis: one.vis.clone(),
                aim: one.aim.clone(),
            },
        );
        plan.relocated.insert(one.old.clone());
    }

    // Visibility: every module on the way to a crossing target, and the items
    // the crossing names, become `pub`.
    let mut widened: BTreeSet<(String, u32)> = BTreeSet::new();
    let mut touched_targets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut needs: BTreeMap<(String, String, bool), BTreeSet<String>> = BTreeMap::new();
    for crossing in &crossings {
        let Some(module_len) = (1..=crossing.path.len())
            .rev()
            .find(|len| file_at.contains_key(&(crossing.root.clone(), crossing.path[..*len].to_vec())))
            .or_else(|| file_at.contains_key(&(crossing.root.clone(), Vec::new())).then_some(0))
        else {
            continue;
        };
        for len in 1..=module_len {
            let Some(file) = file_at.get(&(crossing.root.clone(), crossing.path[..len].to_vec()))
            else {
                continue;
            };
            if let Some(reloc) = relocations.get_mut(file) {
                reloc.vis = "pub".to_string();
                continue;
            }
            let Some(node) = tree.get(file) else { continue };
            let Some((parent, _)) = &node.decl else { continue };
            let name = crossing.path[len - 1].clone();
            widen_named(
                &mut plan,
                &mut widened,
                &scans,
                parent,
                &node.path[..node.path.len() - 1]
                    .iter()
                    .skip(tree.get(parent).map(|p| p.path.len()).unwrap_or(0))
                    .cloned()
                    .collect::<Vec<_>>(),
                &name,
            );
        }
        let Some(file) = file_at.get(&(crossing.root.clone(), crossing.path[..module_len].to_vec()))
        else {
            continue;
        };
        let rest = &crossing.path[module_len..];
        for index in 0..rest.len() {
            widen_named(&mut plan, &mut widened, &scans, file, &rest[..index], &rest[index]);
            if index + 1 < rest.len() {
                widen_member(&mut plan, &mut widened, &scans, file, &rest[index], &rest[index + 1]);
            }
        }
        touched_targets
            .entry(file.clone())
            .or_default()
            .insert(crossing.from.clone());

        // The package edge the crossing needs.
        let from_after = cx.after(&crossing.from).to_string();
        let (Some(from_pkg), Some(to_pkg)) = (
            package_of(&packages, &from_after),
            lib_package.get(crossing.root.as_str()),
        ) else {
            continue;
        };
        if from_pkg.dir != to_pkg.dir {
            let dev = dev_file(&from_after, &crossing.from_root);
            needs
                .entry((from_pkg.manifest.clone(), to_pkg.manifest.clone(), dev))
                .or_default()
                .insert(format!(
                    "{} uses {}",
                    crossing.from,
                    std::iter::once(to_pkg.ident.clone())
                        .chain(crossing.path.iter().cloned())
                        .collect::<Vec<_>>()
                        .join("::")
                ));
        }
    }
    for (file, from) in &touched_targets {
        let Some(target) = scans.get(file) else { continue };
        let texts: Vec<&str> = from
            .iter()
            .filter_map(|rel| scans.get(rel).map(|scanned| scanned.text.as_str()))
            .collect();
        for item in &target.extras.vis {
            if item.public || item.kind == VisKind::Item {
                continue;
            }
            let dotted = format!(".{}", item.name);
            let colons = format!("::{}", item.name);
            if texts
                .iter()
                .any(|text| text.contains(&dotted) || text.contains(&colons))
            {
                widen_item(&mut plan, &mut widened, target, item);
            }
        }
    }

    // Manifests: package edges, then the external crates moved code names.
    let mut inserts: BTreeMap<(String, u32), (Vec<String>, Vec<String>)> = BTreeMap::new();
    let mut added: Vec<(String, String, String)> = Vec::new();
    for ((from_manifest, to_manifest, dev), uses) in &needs {
        let evidence = uses
            .iter()
            .next()
            .and_then(|first| first.split(" uses ").next())
            .unwrap_or_default()
            .to_string();
        let from = packages.iter().find(|p| &p.manifest == from_manifest);
        let to = packages.iter().find(|p| &p.manifest == to_manifest);
        let (Some(from), Some(to)) = (from, to) else { continue };
        let specs = dep_specs(&from.text);
        if specs.iter().any(|spec| spec.key == to.name || spec.key.replace('-', "_") == to.ident) {
            continue;
        }
        // Copy the spec an origin package already writes, else a path dep.
        let copied = moved
            .iter()
            .filter(|one| cx.after(&one.old) != one.old)
            .find_map(|one| {
                let origin = package_of(&packages, &one.old)?;
                let spec = dep_specs(&origin.text)
                    .into_iter()
                    .find(|spec| spec.key == to.name && spec.inline)?;
                Some(reaim_spec(&spec.text, &origin.dir, &from.dir))
            });
        let line = copied.unwrap_or_else(|| {
            format!(
                "{} = {{ path = \"{}\" }}",
                to.name,
                relative_between(&from.dir, &to.dir)
            )
        });
        let section = if *dev { "dev-dependencies" } else { "dependencies" };
        stage_dep(&mut inserts, from, section, &line, &evidence);
        if !dev {
            let shown: Vec<&str> = uses.iter().take(12).map(String::as_str).collect();
            let more = uses.len().saturating_sub(shown.len());
            let mut listed = shown.join("; ");
            if more > 0 {
                listed.push_str(&format!("; and {more} more"));
            }
            added.push((from.manifest.clone(), to.manifest.clone(), listed));
        }
    }
    for (rel, names) in &externals {
        let Some(one) = by_old.get(rel.as_str()) else { continue };
        let (Some(origin), Some(dest)) =
            (package_of(&packages, &one.old), package_of(&packages, &one.new))
        else {
            continue;
        };
        if origin.dir == dest.dir {
            continue;
        }
        let origin_specs = dep_specs(&origin.text);
        let dest_specs = dep_specs(&dest.text);
        for name in names {
            if SYSROOT.contains(&name.as_str()) || idents.contains_key(name) {
                continue;
            }
            let Some(spec) = origin_specs
                .iter()
                .find(|spec| spec.key.replace('-', "_") == *name && spec.section == "dependencies")
            else {
                continue;
            };
            if dest_specs.iter().any(|held| held.key == spec.key) {
                continue;
            }
            let line = reaim_spec(&spec.text, &origin.dir, &dest.dir);
            stage_dep(&mut inserts, dest, "dependencies", &line, rel);
        }
    }
    for ((manifest, offset), (lines, receipts)) in inserts {
        let mut body = String::new();
        for line in &lines {
            body.push_str(line);
            body.push('\n');
        }
        plan.edits.insert(
            (manifest.clone(), offset),
            RelocateEdit {
                importer: manifest.clone(),
                span: Span::anchor(offset),
                text: String::new(),
                target: manifest.clone(),
                kind: CARGO_DEP,
                replacement: body,
                receipt: Some(receipts.join("\n")),
            },
        );
    }

    if let Some(cycle) = dependency_cycle(cx, &packages, &added) {
        plan.errors.push(cycle);
        return plan;
    }

    insert_decls(cx, &relocations, &BTreeSet::new(), &mut plan);
    for one in &moved {
        let from = package_of(&packages, &one.old).map(|p| p.name.as_str()).unwrap_or("?");
        let to = package_of(&packages, &one.new).map(|p| p.name.as_str()).unwrap_or("?");
        if from != to {
            if let Some(edit) = plan.edits.values_mut().find(|edit| {
                edit.kind == MOD_RELOCATE_OUT && edit.target == one.old && edit.receipt.is_none()
            }) {
                edit.receipt = Some(format!("cross-crate {}: {from} -> {to}", one.old));
            }
        }
    }
    plan
}

/// One reference whose reading crosses a crate boundary once the batch lands.
struct Crossing {
    from: String,
    from_root: String,
    root: String,
    path: Vec<String>,
}

fn note_crossing(
    crossings: &mut Vec<Crossing>,
    rel: &str,
    after_root: &str,
    moving: bool,
    reading: &Reading,
    map_path: &dyn Fn(&str, &[String]) -> (String, Vec<String>),
) {
    let (root, path) = map_path(&reading.root, &reading.abs);
    let changed = root != reading.root || path != reading.abs;
    if (moving || changed) && root != after_root {
        crossings.push(Crossing {
            from: rel.to_string(),
            from_root: after_root.to_string(),
            root,
            path,
        });
    }
}

/// The edit a reading needs from the file's post-move scope: how many written
/// idents it covers and what they become, or None when they still read right.
#[allow(clippy::too_many_arguments)]
fn respell_reading(
    reading: &Reading,
    idents: &[String],
    chain: &[String],
    from_use: bool,
    moving: bool,
    after: &Scope,
    map_path: &dyn Fn(&str, &[String]) -> (String, Vec<String>),
    is_moved: &dyn Fn(&str, &[String]) -> bool,
    lib_package: &BTreeMap<&str, &Package>,
    errors: &mut Vec<String>,
    rel: &str,
) -> Option<(usize, String)> {
    if reading.via_use {
        return None;
    }
    // The longest moved-module prefix of the reading, never shorter than what
    // the qualifier already spells.
    let moved_len = (1..=reading.abs.len())
        .rev()
        .find(|len| is_moved(&reading.root, &reading.abs[..*len]))
        .unwrap_or(0);
    if moved_len == 0 && !moving {
        return None;
    }
    let mut span_len = reading.base.max(moved_len);
    let mut cut = reading.eaten + span_len - reading.base;
    if cut == 0 {
        cut = 1;
        span_len = reading.base + 1 - reading.eaten;
    }
    if cut > idents.len() || (!from_use && cut == idents.len()) {
        return None;
    }
    let (want_root, want_path) = map_path(&reading.root, &reading.abs[..span_len]);
    let still = read_path(after, &idents[..cut], chain, true, from_use)
        .map(|read| (read.root, read.abs));
    if still.as_ref() == Some(&(want_root.clone(), want_path.clone())) {
        return None;
    }
    let spelled = if want_root == after.root {
        std::iter::once("crate".to_string())
            .chain(want_path.iter().cloned())
            .collect::<Vec<_>>()
            .join("::")
    } else {
        match lib_package.get(want_root.as_str()) {
            Some(package) => std::iter::once(package.ident.clone())
                .chain(want_path.iter().cloned())
                .collect::<Vec<_>>()
                .join("::"),
            None => {
                errors.push(format!(
                    "{rel}: `{}` would name a module of {want_root}, which is not a library crate",
                    idents.join("::")
                ));
                return None;
            }
        }
    };
    Some((cut, spelled))
}

/// Whether the idents at `spans` are joined by `::` alone.
fn contiguous(text: &str, spans: &[Span]) -> bool {
    spans.windows(2).all(|pair| {
        let end = (pair[0].start + pair[0].len) as usize;
        text.get(end..pair[1].start as usize)
            .is_some_and(|between| between.trim() == "::")
    })
}

fn push_run_edit(plan: &mut RelocatePlan, rel: &str, text: &str, spans: &[Span], replacement: String) {
    let (Some(first), Some(last)) = (spans.first(), spans.last()) else {
        return;
    };
    let span = Span {
        start: first.start,
        len: last.start + last.len - first.start,
    };
    let Some(written) = slice(text, span) else { return };
    if written == replacement {
        return;
    }
    plan.edits.insert(
        (rel.to_string(), span.start),
        RelocateEdit {
            importer: rel.to_string(),
            span,
            text: written,
            target: rel.to_string(),
            kind: MOD_PATH,
            replacement,
            receipt: None,
        },
    );
}

/// A `use` item rewritten one leaf per line, each leaf re-spelled or kept.
fn rewrite_use(decl: &UseDecl, outcomes: &[Option<(usize, String)>], text: &str) -> String {
    let indent: String = {
        let start = decl.span.start as usize;
        let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
        text[line_start..start]
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .collect()
    };
    // Leaves that land under one prefix share one braced line again.
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for (leaf, outcome) in decl.leaves.iter().zip(outcomes) {
        let mut parts: Vec<String> = match outcome {
            Some((cut, spelled)) => std::iter::once(spelled.clone())
                .chain(leaf.idents[*cut..].iter().cloned())
                .collect(),
            None => leaf.idents.clone(),
        };
        let mut tail = match (leaf.glob, parts.len() > 1) {
            (true, _) => "*".to_string(),
            (false, true) => parts.pop().unwrap_or_default(),
            (false, false) => String::new(),
        };
        if let Some(alias) = &leaf.alias {
            tail.push_str(&format!(" as {alias}"));
        }
        let prefix = parts.join("::");
        match groups.iter_mut().find(|(held, _)| *held == prefix) {
            Some((_, tails)) => tails.push(tail),
            None => groups.push((prefix, vec![tail])),
        }
    }
    let lines: Vec<String> = groups
        .into_iter()
        .map(|(prefix, tails)| {
            let path = match tails.as_slice() {
                [only] if only == "self" => prefix,
                [only] if only.is_empty() => prefix,
                [only] => format!("{prefix}::{only}"),
                many => format!("{prefix}::{{{}}}", many.join(", ")),
            };
            format!("{}use {path};", decl.vis)
        })
        .collect();
    lines.join(&format!("\n{indent}"))
}

/// The `use` bindings a file's top level makes, read against its own scope.
fn bindings_of(
    scanned: &Scanned,
    node: &Node,
    children: &BTreeSet<String>,
    idents: &BTreeMap<String, String>,
) -> BTreeMap<String, (String, Vec<String>)> {
    let empty = BTreeMap::new();
    let scope = Scope {
        root: &node.root,
        here: &node.path,
        children,
        bindings: &empty,
        idents,
    };
    let mut out = BTreeMap::new();
    for decl in scanned
        .extras
        .uses
        .iter()
        .filter(|decl| decl.chain.is_empty() && !decl.in_fn)
    {
        for leaf in decl.leaves.iter().filter(|leaf| !leaf.glob) {
            let Some(reading) = read_path(&scope, &leaf.idents, &[], true, true) else {
                continue;
            };
            let mut abs = reading.abs.clone();
            if abs.last().map(String::as_str) == Some("self") {
                abs.pop();
            }
            let name = leaf
                .alias
                .clone()
                .or_else(|| abs.last().cloned())
                .unwrap_or_default();
            if name.is_empty() || name == "_" {
                continue;
            }
            out.insert(name, (reading.root, abs));
        }
    }
    out
}

/// Whether a file compiles in a test, bench or example target of its package.
fn dev_file(rel: &str, root: &str) -> bool {
    let dir = dirname(root);
    ["tests", "benches", "examples"]
        .iter()
        .any(|kind| dir.ends_with(&format!("/{kind}")) || dir == *kind)
        || rel.contains("/tests/")
}

// ── visibility ──────────────────────────────────────────────────────────────

fn widen_item(
    plan: &mut RelocatePlan,
    widened: &mut BTreeSet<(String, u32)>,
    file: &Scanned,
    item: &VisItem,
) {
    if item.public || !widened.insert((file.rel.clone(), item.span.start)) {
        return;
    }
    let line = 1 + file.text.as_bytes()[..item.span.start as usize]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count();
    plan.edits.insert(
        (file.rel.clone(), item.span.start),
        RelocateEdit {
            importer: file.rel.clone(),
            span: item.span,
            text: slice(&file.text, item.span).unwrap_or_default(),
            target: file.rel.clone(),
            kind: WIDEN_VIS,
            replacement: item.text.clone(),
            receipt: Some(format!("widen {}:{line} {} -> pub", file.rel, item.name)),
        },
    );
}

fn widen_named(
    plan: &mut RelocatePlan,
    widened: &mut BTreeSet<(String, u32)>,
    scans: &BTreeMap<String, &Scanned>,
    file: &str,
    chain: &[String],
    name: &str,
) {
    let Some(scanned) = scans.get(file) else { return };
    let found: Vec<&VisItem> = scanned
        .extras
        .vis
        .iter()
        .filter(|item| {
            item.kind == VisKind::Item && item.name == name && item.chain == chain
        })
        .collect();
    for item in found {
        widen_item(plan, widened, scanned, item);
    }
}

fn widen_member(
    plan: &mut RelocatePlan,
    widened: &mut BTreeSet<(String, u32)>,
    scans: &BTreeMap<String, &Scanned>,
    file: &str,
    owner: &str,
    name: &str,
) {
    let Some(scanned) = scans.get(file) else { return };
    let found: Vec<&VisItem> = scanned
        .extras
        .vis
        .iter()
        .filter(|item| {
            item.kind != VisKind::Item
                && item.name == name
                && item.owner.as_deref() == Some(owner)
        })
        .collect();
    for item in found {
        widen_item(plan, widened, scanned, item);
    }
}

// ── placing the moved modules ───────────────────────────────────────────────

/// Each moved file's landing: the parent module that owns its destination
/// directory once the batch lands, and whether its decl leaves the old parent.
fn place_moves(
    cx: &MoveCx,
    tree: &BTreeMap<String, Node>,
    scans: &BTreeMap<String, &Scanned>,
    plan: &mut RelocatePlan,
) -> Option<Vec<Moved>> {
    // Directory -> (pre-move rel, post-move rel, crate root, module path).
    let mut owners: BTreeMap<String, (String, String, String, Vec<String>)> = BTreeMap::new();
    for (rel, node) in tree {
        if cx.destination(rel).is_some() {
            continue;
        }
        owners
            .entry(child_dir(rel, node.owns_dir))
            .or_insert_with(|| (rel.clone(), rel.clone(), node.root.clone(), node.path.clone()));
    }
    let mut pending: Vec<(&String, &String)> = cx
        .moved()
        .iter()
        .filter(|(old, _)| old.ends_with(".rs") && tree.contains_key(*old))
        .collect();
    let mut placed: Vec<Moved> = Vec::new();
    loop {
        let before = pending.len();
        let mut rest = Vec::new();
        for (old, new) in pending {
            let dest_dir = dirname(new).to_string();
            let Some((parent_pre, parent_after, root, parent_path)) = owners.get(&dest_dir).cloned()
            else {
                rest.push((old, new));
                continue;
            };
            let node = &tree[old];
            let Some((decl_parent, index)) = &node.decl else {
                plan.errors.push(format!("{old} is a crate root; a move cannot carry one across crates"));
                continue;
            };
            let Some(decl) = scans.get(decl_parent).and_then(|scanned| scanned.scan.decls.get(*index))
            else {
                continue;
            };
            let natural = natural_paths(&dest_dir, &decl.name).contains(new);
            let lift = *decl_parent != parent_pre;
            if lift && !decl.chain.is_empty() {
                plan.errors.push(format!(
                    "{old}: its `mod {}` sits in an inline block of {decl_parent}; move it by hand",
                    decl.name
                ));
                continue;
            }
            let aim = match (lift, natural) {
                (true, false) => format!(
                    "#[path = \"{}\"] ",
                    relative_between(dirname(&parent_after), new)
                ),
                _ => String::new(),
            };
            let owns_dir = stem(new) == "mod" || decl.attr.is_some() || !natural;
            let mut new_path = parent_path.clone();
            new_path.push(decl.name.clone());
            owners
                .entry(child_dir(new, owns_dir))
                .or_insert_with(|| ((*old).clone(), (*new).clone(), root.clone(), new_path.clone()));
            let decl_item = scans.get(decl_parent).and_then(|scanned| {
                whole_lines(&scanned.text, decl.item)
                    .map(|(span, text)| (decl_parent.clone(), span, text))
            });
            placed.push(Moved {
                old: (*old).clone(),
                new: (*new).clone(),
                name: decl.name.clone(),
                old_root: node.root.clone(),
                old_path: node.path.clone(),
                new_root: root,
                new_path,
                lift,
                parent_pre,
                parent_after,
                aim,
                vis: decl.vis.clone(),
                decl_item,
            });
        }
        if rest.is_empty() {
            break;
        }
        if rest.len() == before {
            for (old, new) in rest {
                plan.errors.push(format!(
                    "{old} -> {new} has no parent module file: no module owns {} once the batch lands",
                    dirname(new)
                ));
            }
            break;
        }
        pending = rest;
    }
    // A module carries its subtree: every child it declares moves with it.
    for one in &placed {
        for (rel, node) in tree {
            if node.decl.as_ref().is_some_and(|(parent, _)| parent == &one.old)
                && cx.destination(rel).is_none()
            {
                plan.errors.push(format!(
                    "{} declares `mod {}` ({rel}), which this batch does not move; add it to the batch",
                    one.old,
                    node.path.last().cloned().unwrap_or_default()
                ));
            }
        }
    }
    Some(placed)
}

fn scan_all(cx: &MoveCx) -> Vec<Scanned> {
    let base = relocate_scan(cx);
    extract_pool().install(|| {
        base.into_par_iter()
            .filter_map(|(rel, text, scan, runs)| {
                let extras = extras(&text)?;
                Some(Scanned {
                    rel,
                    text,
                    scan,
                    runs,
                    extras,
                })
            })
            .collect()
    })
}

// ── manifests ───────────────────────────────────────────────────────────────

/// One dependency a manifest declares: the table it sits in, its key, and the
/// text of the pair (or the whole `[dependencies.key]` table).
struct DepSpec {
    section: String,
    key: String,
    text: String,
    inline: bool,
}

/// Every dependency pair in a manifest's three dependency tables.
fn dep_specs(text: &str) -> Vec<DepSpec> {
    let mut out = Vec::new();
    for table in toml_tables(text) {
        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            if table.header == section {
                for (key, span) in &table.pairs {
                    out.push(DepSpec {
                        section: section.to_string(),
                        key: key.clone(),
                        text: slice(text, *span).unwrap_or_default(),
                        inline: true,
                    });
                }
            } else if let Some(key) = table.header.strip_prefix(&format!("{section}.")) {
                out.push(DepSpec {
                    section: section.to_string(),
                    key: key.trim_matches('"').to_string(),
                    text: slice(text, table.span).unwrap_or_default(),
                    inline: false,
                });
            }
        }
    }
    out
}

struct TomlTable {
    header: String,
    span: Span,
    pairs: Vec<(String, Span)>,
}

fn toml_tables(text: &str) -> Vec<TomlTable> {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_toml_ng::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(text, None) else {
        return Vec::new();
    };
    let source = text.as_bytes();
    let root = tree.root_node();
    let mut cursor = root.walk();
    let mut out = Vec::new();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "table" {
            continue;
        }
        let mut inner = child.walk();
        let mut header = None;
        let mut pairs = Vec::new();
        for part in child.named_children(&mut inner) {
            match part.kind() {
                "bare_key" | "dotted_key" | "quoted_key" if header.is_none() => {
                    header = Some(
                        String::from_utf8_lossy(&source[part.start_byte()..part.end_byte()])
                            .replace(' ', ""),
                    );
                }
                "pair" => {
                    let Some(key) = part.named_child(0) else { continue };
                    let key = String::from_utf8_lossy(&source[key.start_byte()..key.end_byte()])
                        .trim_matches('"')
                        .to_string();
                    pairs.push((
                        key,
                        Span {
                            start: part.start_byte() as u32,
                            len: (part.end_byte() - part.start_byte()) as u32,
                        },
                    ));
                }
                _ => {}
            }
        }
        let Some(header) = header else { continue };
        out.push(TomlTable {
            header,
            span: Span {
                start: child.start_byte() as u32,
                len: (child.end_byte() - child.start_byte()) as u32,
            },
            pairs,
        });
    }
    out
}

/// A copied spec's `path = ".."` re-aimed from `from_dir` to `to_dir`, and its
/// `optional = true` dropped (the new package declares no feature for it).
fn reaim_spec(spec: &str, from_dir: &str, to_dir: &str) -> String {
    let mut out = spec.to_string();
    for (needle, replacement) in [
        (", optional = true", ""),
        ("optional = true, ", ""),
        ("optional = true", ""),
    ] {
        out = out.replace(needle, replacement);
    }
    if let Some(at) = out.find("path") {
        let tail = &out[at..];
        if let (Some(open), true) = (tail.find('"'), tail[..tail.find('"').unwrap_or(0)].contains('=')) {
            let value_start = at + open + 1;
            if let Some(close) = out[value_start..].find('"') {
                let written = out[value_start..value_start + close].to_string();
                let target = join_rel(from_dir, &written);
                let aimed = relative_between(to_dir, &target);
                out.replace_range(value_start..value_start + close, &aimed);
            }
        }
    }
    out
}

/// Queue one dependency line under `section` of `package`'s manifest.
fn stage_dep(
    inserts: &mut BTreeMap<(String, u32), (Vec<String>, Vec<String>)>,
    package: &Package,
    section: &str,
    line: &str,
    evidence: &str,
) {
    let tables = toml_tables(&package.text);
    let (offset, header) = match tables.iter().find(|table| table.header == section) {
        Some(table) => {
            let end = table
                .pairs
                .last()
                .map(|(_, span)| (span.start + span.len) as usize)
                .unwrap_or((table.span.start + table.span.len) as usize);
            (line_after(&package.text, end) as u32, false)
        }
        None => (package.text.len() as u32, true),
    };
    let entry = inserts
        .entry((package.manifest.clone(), offset))
        .or_default();
    if entry.0.iter().any(|held| held == line) {
        return;
    }
    if header && entry.0.is_empty() {
        let lead = if package.text.ends_with('\n') { "\n" } else { "\n\n" };
        entry.0.push(format!("{lead}[{section}]"));
    }
    entry.0.push(line.to_string());
    entry
        .1
        .push(format!("dep {}: + {line} (for {evidence})", package.manifest));
}

/// The first dependency cycle one of the `added` edges closes, spelled for the
/// plan error; None when the graph stays acyclic.
fn dependency_cycle(
    cx: &MoveCx,
    packages: &[Package],
    added: &[(String, String, String)],
) -> Option<String> {
    if added.is_empty() {
        return None;
    }
    let manifests: Vec<Manifest> = packages
        .iter()
        .map(|package| Manifest {
            path: package.manifest.clone(),
            kind: ManifestKind::Cargo,
            text: package.text.clone(),
        })
        .collect();
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for edge in fold_package_edges(&manifests) {
        if let FlatFact::PackageEdgeRow {
            src_manifest,
            dst_manifest,
            kind,
        } = edge
        {
            if kind != "dev" {
                graph.entry(src_manifest).or_default().insert(dst_manifest);
            }
        }
    }
    for (from, to, _) in added {
        graph.entry(from.clone()).or_default().insert(to.clone());
    }
    let name_of = |manifest: &str| {
        packages
            .iter()
            .find(|package| package.manifest == manifest)
            .map(|package| package.name.clone())
            .unwrap_or_else(|| manifest.to_string())
    };
    let _ = cx;
    for (from, to, evidence) in added {
        // A path back from `to` to `from` closes the cycle.
        let mut previous: BTreeMap<String, String> = BTreeMap::new();
        let mut frontier = vec![to.clone()];
        let mut seen: BTreeSet<String> = BTreeSet::from([to.clone()]);
        while let Some(at) = frontier.pop() {
            if at == *from {
                let mut chain = vec![name_of(from)];
                let mut step = from.clone();
                while let Some(back) = previous.get(&step) {
                    chain.push(name_of(back));
                    step = back.clone();
                }
                chain.reverse();
                let mut spelled = vec![name_of(from)];
                spelled.extend(chain);
                return Some(format!(
                    "dependency cycle: {} (the batch makes {} depend on {}: {evidence})",
                    spelled.join(" -> "),
                    name_of(from),
                    name_of(to)
                ));
            }
            for next in graph.get(&at).into_iter().flatten() {
                if seen.insert(next.clone()) {
                    previous.insert(next.clone(), at.clone());
                    frontier.push(next.clone());
                }
            }
        }
    }
    None
}
