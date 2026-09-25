//! Cross-package `extract move` for TS: a specifier that crosses a package.json
//! boundary spells the package name; the importer's manifest gains the dependency.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use super::{first_object, json_literal, node_string, quote_of, specifier_refs};
use crate::manifests::{fold_package_edges, Manifest, ManifestKind};
use crate::move_cx::{dirname, join_rel, relative_between, MoveCx};
use crate::types::{LangKind, Span};
use crate::wire::FlatFact;
use crate::edit_seams::ImportRef;
use crate::edit_seams::ImportRefKind;

/// A manifest edit: one dependency a package.json gains.
pub const PKG_DEP: ImportRefKind = ImportRefKind::Ext(LangKind {
    lang: "ts",
    tag: "pkg_dep",
});

const DEP_FIELDS: [&str; 3] = ["dependencies", "devDependencies", "peerDependencies"];

struct TsPackage {
    dir: String,
    manifest: String,
    name: String,
    text: String,
    /// `main` / `module` / `types` targets, root-relative.
    entries: Vec<String>,
    /// `exports` subpath -> root-relative target, string leaves only.
    exports: Vec<(String, String)>,
    deps: BTreeSet<String>,
}

fn packages(cx: &MoveCx) -> &'static Vec<TsPackage> {
    static CACHE: OnceLock<Mutex<BTreeMap<PathBuf, &'static Vec<TsPackage>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut held = match cache.lock() {
        Ok(held) => held,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(existing) = held.get(cx.root()) {
        return existing;
    }
    let found: Vec<TsPackage> = cx
        .files()
        .iter()
        .filter(|rel| rel.as_str() == "package.json" || rel.ends_with("/package.json"))
        .filter(|rel| !rel.split('/').any(|part| part == "node_modules" || part == "dist"))
        .filter_map(|manifest| read_package(cx, manifest))
        .collect();
    let leaked: &'static Vec<TsPackage> = Box::leak(Box::new(found));
    held.insert(cx.root().to_path_buf(), leaked);
    leaked
}

fn read_package(cx: &MoveCx, manifest: &str) -> Option<TsPackage> {
    let text = cx.text(manifest)?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let name = value.get("name")?.as_str()?.to_string();
    let dir = dirname(manifest).to_string();
    let entries = ["main", "module", "types"]
        .iter()
        .filter_map(|field| value.get(*field)?.as_str())
        .map(|target| join_rel(&dir, target))
        .collect();
    let mut exports = Vec::new();
    match value.get("exports") {
        Some(serde_json::Value::String(target)) => exports.push((".".to_string(), join_rel(&dir, target))),
        Some(serde_json::Value::Object(map)) => {
            for (key, target) in map {
                collect_exports(key, target, &dir, &mut exports);
            }
        }
        _ => {}
    }
    let deps = DEP_FIELDS
        .iter()
        .filter_map(|field| value.get(*field)?.as_object())
        .flat_map(|object| object.keys().cloned())
        .collect();
    Some(TsPackage {
        dir,
        manifest: manifest.to_string(),
        name,
        text,
        entries,
        exports,
        deps,
    })
}

fn collect_exports(key: &str, value: &serde_json::Value, dir: &str, out: &mut Vec<(String, String)>) {
    match value {
        serde_json::Value::String(target) => out.push((key.to_string(), join_rel(dir, target))),
        serde_json::Value::Object(conditions) => {
            for nested in conditions.values() {
                collect_exports(key, nested, dir, out);
            }
        }
        _ => {}
    }
}

fn package_of<'a>(packages: &'a [TsPackage], rel: &str) -> Option<&'a TsPackage> {
    packages
        .iter()
        .filter(|package| package.dir.is_empty() || rel.starts_with(&format!("{}/", package.dir)))
        .max_by_key(|package| package.dir.len())
}

fn strip_ts_extension(rel: &str) -> &str {
    for extension in [".d.ts", ".tsx", ".ts", ".mts", ".cts", ".jsx", ".js", ".mjs", ".cjs"] {
        if let Some(stem) = rel.strip_suffix(extension) {
            return stem;
        }
    }
    rel
}

/// `name`, `name/<export>`, or `name/<path in package>` for `aimed`.
fn package_spec(package: &TsPackage, aimed: &str) -> String {
    if package.entries.iter().any(|entry| entry == aimed) {
        return package.name.clone();
    }
    let bare = strip_ts_extension(aimed);
    for (key, target) in &package.exports {
        if target == aimed || strip_ts_extension(target) == bare {
            return match key.strip_prefix('.') {
                Some("") | None => package.name.clone(),
                Some(tail) => format!("{}{tail}", package.name),
            };
        }
    }
    let inside = relative_between(&package.dir, bare);
    let inside = inside.strip_suffix("/index").unwrap_or(&inside);
    format!("{}/{inside}", package.name)
}

/// `to` spelled from `from` when the two sit in different named packages.
pub fn spec_across(cx: &MoveCx, from: &str, to: &str) -> Option<String> {
    let packages = packages(cx);
    let from_package = package_of(packages, from)?;
    let to_package = package_of(packages, to)?;
    (from_package.dir != to_package.dir).then(|| package_spec(to_package, to))
}

/// The package (name, declared dependency names) holding `rel`.
pub fn package_deps(cx: &MoveCx, rel: &str) -> Option<(String, BTreeSet<String>)> {
    let package = package_of(packages(cx), rel)?;
    Some((package.name.clone(), package.deps.clone()))
}

/// The re-spelling a specifier needs when the batch changes which package its
/// importer and target sit in; None when the default relative/alias arms answer.
pub(super) fn respell(
    cx: &MoveCx,
    reference: &ImportRef,
    module: &str,
    relative_spec: bool,
    aimed: &str,
) -> Option<String> {
    let packages = packages(cx);
    let importer_after = cx.after(&reference.importer);
    let from_after = package_of(packages, importer_after)?;
    let to_after = package_of(packages, aimed)?;
    let from_before = package_of(packages, &reference.importer).map(|package| &package.dir);
    let to_before = package_of(packages, &reference.target).map(|package| &package.dir);
    let crossing_after = from_after.dir != to_after.dir;
    let crossing_before = from_before != to_before;
    let names_package = !relative_spec
        && packages
            .iter()
            .any(|package| module == package.name || module.starts_with(&format!("{}/", package.name)));
    let quote = quote_of(&reference.text);
    if crossing_after && (!crossing_before || names_package) {
        return Some(format!("{quote}{}{quote}", package_spec(to_after, aimed)));
    }
    if !crossing_after && names_package {
        let relative = relative_between(dirname(importer_after), aimed);
        return Some(super::respell(&relative, strip_ts_extension(module), quote));
    }
    None
}

// ── the dependency plan ─────────────────────────────────────────────────────

#[derive(Default)]
pub(super) struct DepPlan {
    /// (manifest, offset) -> (inserted text, receipt).
    pub(super) edits: BTreeMap<(String, u32), (String, String)>,
    pub(super) errors: Vec<String>,
}

pub(super) fn dep_plan(cx: &MoveCx) -> &'static DepPlan {
    static CACHE: OnceLock<Mutex<BTreeMap<PathBuf, &'static DepPlan>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut held = match cache.lock() {
        Ok(held) => held,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(existing) = held.get(cx.root()) {
        return existing;
    }
    let moves_ts = cx
        .moved()
        .keys()
        .any(|old| crate::move_cx::owned_by(old, &super::TsSource));
    let plan = match moves_ts {
        true => build_dep_plan(cx, &specifier_refs(cx)),
        false => DepPlan::default(),
    };
    let leaked: &'static DepPlan = Box::leak(Box::new(plan));
    held.insert(cx.root().to_path_buf(), leaked);
    leaked
}

fn build_dep_plan(cx: &MoveCx, specifier_refs: &[ImportRef]) -> DepPlan {
    let mut plan = DepPlan::default();
    let packages = packages(cx);
    // (importer manifest) -> (dependency names, evidence file)
    let mut needs: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for reference in specifier_refs.iter().filter(|r| r.kind == ImportRefKind::Import) {
        let importer_after = cx.after(&reference.importer);
        let aimed = cx.after(&reference.target);
        if cx.destination(&reference.importer).is_none() && cx.destination(&reference.target).is_none() {
            continue;
        }
        let (Some(from), Some(to)) = (package_of(packages, importer_after), package_of(packages, aimed))
        else {
            continue;
        };
        if from.dir == to.dir || from.deps.contains(&to.name) {
            continue;
        }
        needs
            .entry(from.manifest.clone())
            .or_default()
            .entry(to.name.clone())
            .or_insert_with(|| reference.importer.clone());
    }
    if needs.is_empty() {
        return plan;
    }
    let protocol = match packages.iter().any(|package| package.text.contains("\"workspace:")) {
        true => "workspace:*",
        false => "*",
    };
    let mut added: Vec<(String, String, String)> = Vec::new();
    for (manifest, names) in &needs {
        let Some(package) = packages.iter().find(|package| &package.manifest == manifest) else {
            continue;
        };
        let pairs: Vec<(String, String)> = names
            .iter()
            .map(|(name, _)| (name.clone(), protocol.to_string()))
            .collect();
        let Some((offset, text)) = dependency_insertion(&package.text, &pairs) else {
            plan.errors.push(format!("{manifest}: cannot place a dependencies entry"));
            continue;
        };
        let receipt = names
            .iter()
            .map(|(name, evidence)| format!("dep {manifest}: + \"{name}\": \"{protocol}\" (for {evidence})"))
            .collect::<Vec<_>>()
            .join("\n");
        plan.edits.insert((manifest.clone(), offset), (text, receipt));
        for (name, evidence) in names {
            if let Some(to) = packages.iter().find(|package| &package.name == name) {
                added.push((manifest.clone(), to.manifest.clone(), evidence.clone()));
            }
        }
    }
    if let Some(cycle) = dependency_cycle(packages, &added) {
        plan.errors.push(cycle);
    }
    plan
}

/// Where the new `dependencies` pairs go and the text that carries them.
fn dependency_insertion(text: &str, pairs: &[(String, String)]) -> Option<(u32, String)> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_json::LANGUAGE.into()).ok()?;
    let tree = parser.parse(text, None)?;
    let source = text.as_bytes();
    let root = first_object(tree.root_node())?;
    let mut cursor = root.walk();
    let members: Vec<tree_sitter::Node<'_>> = root
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "pair")
        .collect();
    let written = |indent: &str| -> String {
        pairs
            .iter()
            .map(|(name, version)| format!("{indent}{}: {}", json_literal(name), json_literal(version)))
            .collect::<Vec<_>>()
            .join(",\n")
    };
    let indent_of = |at: usize| -> String {
        let line = text[..at].rfind('\n').map_or(0, |found| found + 1);
        text[line..at].chars().take_while(|ch| ch.is_whitespace()).collect()
    };
    for pair in &members {
        let key = pair.child_by_field_name("key")?;
        if node_string(key, source).as_deref() != Some("dependencies") {
            continue;
        }
        let value = pair.child_by_field_name("value")?;
        let mut inner = value.walk();
        let last = value
            .named_children(&mut inner)
            .filter(|node| node.kind() == "pair")
            .last();
        return match last {
            Some(last) => {
                let indent = indent_of(last.start_byte());
                Some((last.end_byte() as u32, format!(",\n{}", written(&indent))))
            }
            None => {
                let outer = indent_of(pair.start_byte());
                Some((
                    value.start_byte() as u32 + 1,
                    format!("\n{}\n{outer}", written(&format!("{outer}  "))),
                ))
            }
        };
    }
    let last = members.last()?;
    let indent = indent_of(last.start_byte());
    Some((
        last.end_byte() as u32,
        format!(
            ",\n{indent}\"dependencies\": {{\n{}\n{indent}}}",
            written(&format!("{indent}  "))
        ),
    ))
}

fn dependency_cycle(packages: &[TsPackage], added: &[(String, String, String)]) -> Option<String> {
    if added.is_empty() {
        return None;
    }
    let manifests: Vec<Manifest> = packages
        .iter()
        .map(|package| Manifest {
            path: package.manifest.clone(),
            kind: ManifestKind::Npm,
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
    for (from, to, evidence) in added {
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
                    "dependency cycle: {} (the batch makes {} depend on {} for {evidence})",
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

/// The refs the dependency plan publishes, one per manifest insertion.
pub(super) fn dep_refs(plan: &DepPlan) -> Vec<ImportRef> {
    plan.edits
        .keys()
        .map(|(manifest, offset)| ImportRef {
            importer: manifest.clone(),
            literal: Span::anchor(*offset),
            text: String::new(),
            target: manifest.clone(),
            kind: PKG_DEP,
        })
        .collect()
}
