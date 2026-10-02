//! Workspace package discovery and each package's declared build layouts.
//! `TsResolver` and `TsModuleIndex` both read packages through `discover`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use oxc_allocator::Allocator;
use oxc_ast::ast as ts;
use oxc_parser::Parser;
use oxc_resolver::Resolver;
use oxc_span::SourceType;

/// A package manifest in the source workspace, independent of installed links.
pub(crate) struct WorkspacePackage {
    pub directory: PathBuf,
    pub manifest: serde_json::Value,
    layout: OnceLock<Layout>,
}

/// The packages above a run's paths, keyed by manifest `name`.
#[derive(Default)]
pub(crate) struct Packages {
    pub by_name: BTreeMap<String, WorkspacePackage>,
    /// Real package directory -> the spelling the run supplied for it.
    pub written: HashMap<PathBuf, PathBuf>,
}

/// Every `package.json` in an ancestor directory of a supplied `(real, written)`
/// path. Paths are visited sorted; the first manifest per name wins.
pub(crate) fn discover<'a>(paths: impl IntoIterator<Item = (&'a Path, &'a Path)>) -> Packages {
    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort();
    let mut packages = Packages::default();
    let mut visited = HashSet::new();
    for (real, written) in paths {
        for (directory, spelled) in real.ancestors().skip(1).zip(written.ancestors().skip(1)) {
            if !visited.insert(directory.to_path_buf()) {
                continue;
            }
            let _probe = tracing::trace_span!("ts.packages.directory").entered();
            let Some(manifest) =
                std::fs::File::open(crate::read::io_path(&directory.join("package.json")))
                    .ok()
                    .and_then(|file| serde_json::from_reader::<_, serde_json::Value>(file).ok())
            else {
                continue;
            };
            let Some(name) = manifest.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            packages
                .written
                .insert(directory.to_path_buf(), spelled.to_path_buf());
            packages
                .by_name
                .entry(name.to_string())
                .or_insert_with(|| WorkspacePackage {
                    directory: directory.to_path_buf(),
                    manifest,
                    layout: OnceLock::new(),
                });
        }
    }
    packages
}

/// The npm package name a bare specifier starts with: one segment, two when scoped.
pub(crate) fn package_name(module: &str) -> Option<&str> {
    let mut parts = module.split('/');
    let first = parts.next().filter(|first| !first.is_empty() && !first.starts_with('.'))?;
    let len = match first.starts_with('@') {
        true => first.len() + 1 + parts.next()?.len(),
        false => first.len(),
    };
    Some(&module[..len])
}

/// One package tsconfig's `rootDir`, `outDir` and `declarationDir`, absolute.
struct Build {
    root_dir: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    declaration_dir: Option<PathBuf>,
}

/// Every `tsconfig*.json` in the package directory: a build config such as
/// `tsconfig.build.json` is the one that writes the published output.
#[derive(Default)]
pub(crate) struct Layout {
    builds: Vec<Build>,
}

impl WorkspacePackage {
    /// Read once per package, each config with the configs it extends.
    pub fn layout(&self, resolver: &Resolver) -> &Layout {
        self.layout.get_or_init(|| {
            let _read = tracing::trace_span!("ts.packages.layout").entered();
            let Ok(entries) = std::fs::read_dir(crate::read::io_path(&self.directory)) else {
                return Layout::default();
            };
            let mut configs: Vec<PathBuf> = entries
                .filter_map(|entry| Some(entry.ok()?.path()))
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("tsconfig") && name.ends_with(".json"))
                })
                .collect();
            configs.sort();
            let builds = configs
                .iter()
                .filter_map(|path| resolver.resolve_tsconfig(path).ok())
                .map(|config| Build {
                    root_dir: root_dir(&config.path),
                    out_dir: config.compiler_options.out_dir.clone(),
                    declaration_dir: config.compiler_options.declaration_dir.clone(),
                })
                .collect();
            Layout { builds }
        })
    }
}

impl Build {
    fn outputs(&self) -> impl Iterator<Item = &PathBuf> {
        [&self.declaration_dir, &self.out_dir].into_iter().flatten()
    }
}

impl Layout {
    /// Whether `path` lies in a directory some config writes output to.
    pub fn emits(&self, path: &Path) -> bool {
        self.builds.iter().flat_map(Build::outputs).any(|directory| path.starts_with(directory))
    }

    /// The input path `emitted` is compiled from, spelled as an import names it:
    /// `rootDir` plus the path under the output directory, a declaration as its JS twin.
    pub fn source_of(&self, emitted: &Path) -> Option<PathBuf> {
        let (root, relative) = self.builds.iter().find_map(|build| {
            let relative = build.outputs().find_map(|directory| emitted.strip_prefix(directory).ok())?;
            Some((build.root_dir.as_ref()?, relative.to_str()?))
        })?;
        let spelled = [(".d.ts", ".js"), (".d.mts", ".mjs"), (".d.cts", ".cjs")]
            .iter()
            .find_map(|(declaration, js)| {
                relative.strip_suffix(declaration).map(|stem| format!("{stem}{js}"))
            })
            .unwrap_or_else(|| relative.to_string());
        Some(root.join(spelled))
    }
}

/// `compilerOptions.rootDir` of `config`, else of the config it extends by path.
/// oxc_resolver's tsconfig model has no `rootDir`, so the JSONC is parsed here.
fn root_dir(config: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(crate::read::io_path(config)).ok()?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &text, SourceType::default())
        .parse_expression()
        .ok()?;
    let ts::Expression::ObjectExpression(object) = &parsed else {
        return None;
    };
    let directory = config.parent()?;
    if let Some(ts::Expression::ObjectExpression(options)) = object_field(object, "compilerOptions") {
        if let Some(ts::Expression::StringLiteral(root)) = object_field(options, "rootDir") {
            return Some(directory.join(root.value.as_str()).components().collect());
        }
    }
    // A package-name `extends` lives under node_modules and is not followed.
    let ts::Expression::StringLiteral(extends) = object_field(object, "extends")? else {
        return None;
    };
    let extends = extends.value.as_str();
    if !extends.starts_with('.') {
        return None;
    }
    let mut parent = directory.join(extends).into_os_string();
    if !extends.ends_with(".json") {
        parent.push(".json");
    }
    root_dir(Path::new(&parent))
}

fn object_field<'b, 'a>(object: &'b ts::ObjectExpression<'a>, key: &str) -> Option<&'b ts::Expression<'a>> {
    object.properties.iter().find_map(|property| match property {
        ts::ObjectPropertyKind::ObjectProperty(property) => {
            (property.key.static_name()?.as_ref() == key).then_some(&property.value)
        }
        _ => None,
    })
}
