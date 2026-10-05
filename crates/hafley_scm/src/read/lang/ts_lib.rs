//! The TypeScript install both tiers use, and the global value names its lib
//! declarations (`lib.*.d.ts`) declare.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::ast as ts;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// The `typescript` package the slow tier runs: the copy bundled with sprefa-extract,
/// else the nearest `node_modules/typescript` above `root`.
pub fn typescript_package(root: Option<&Path>) -> Option<PathBuf> {
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sprefa-extract/ts7/node_modules/typescript");
    std::iter::once(bundled)
        .chain(root.into_iter().flat_map(Path::ancestors).map(|dir| dir.join("node_modules/typescript")))
        .find(|package| package.join("bin/tsc").is_file())
}

/// The installed native compiler, bypassing the Node launcher.
pub fn typescript_executable(root: Option<&Path>) -> Option<PathBuf> {
    let package = std::fs::canonicalize(typescript_package(root)?).ok()?;
    let platform = match std::env::consts::OS { "macos" => "darwin", "windows" => "win32", other => other };
    let arch = match std::env::consts::ARCH { "aarch64" => "arm64", "x86_64" => "x64", other => other };
    let executable = if platform == "win32" { "tsc.exe" } else { "tsc" };
    package.ancestors()
        .filter(|path| path.file_name().is_some_and(|name| name == "node_modules"))
        .map(|path| path.join(format!("@typescript/typescript-{platform}-{arch}/lib/{executable}")))
        .find(|path| path.is_file())
}

/// The directory holding `lib.d.ts`: the package's own `lib/`, or for the native
/// compiler the `@typescript/typescript-<platform>-<arch>` package node resolves from it.
fn lib_dir(package: &Path) -> Option<PathBuf> {
    let own = package.join("lib");
    if own.join("lib.d.ts").is_file() {
        return Some(own);
    }
    let platform = match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    let native = format!("@typescript/typescript-{platform}-{arch}/lib");
    let real = std::fs::canonicalize(crate::read::io_path(package)).ok()?;
    real.ancestors()
        .filter(|dir| dir.file_name().is_some_and(|name| name == "node_modules"))
        .map(|dir| dir.join(&native))
        .find(|dir| dir.join("lib.d.ts").is_file())
}

/// Every value name a `lib*.d.ts` beside the slow tier's compiler declares at top
/// level. `None` when no TypeScript install is found: callers abstain.
pub fn globals(root: Option<&Path>) -> Option<HashSet<String>> {
    let dir = lib_dir(&typescript_package(root)?)?;
    let _span = tracing::debug_span!("ts.lib.globals").entered();
    let mut names = HashSet::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(crate::read::io_path(&dir))
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("lib") && name.ends_with(".d.ts"))
        })
        .collect();
    files.sort();
    for file in files {
        let Ok(text) = std::fs::read_to_string(crate::read::io_path(&file)) else {
            continue;
        };
        declared_values(&text, &mut names);
    }
    Some(names)
}

fn declared_values(text: &str, names: &mut HashSet<String>) {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, text, SourceType::d_ts()).parse();
    for statement in &parsed.program.body {
        match statement {
            ts::Statement::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    if let Some(id) = declarator.id.get_binding_identifier() {
                        names.insert(id.name.to_string());
                    }
                }
            }
            ts::Statement::FunctionDeclaration(function) => {
                names.extend(function.id.as_ref().map(|id| id.name.to_string()));
            }
            ts::Statement::ClassDeclaration(class) => {
                names.extend(class.id.as_ref().map(|id| id.name.to_string()));
            }
            ts::Statement::TSNamespaceDeclaration(namespace) => {
                names.insert(namespace.id.name.to_string());
            }
            ts::Statement::TSEnumDeclaration(declaration) => {
                names.insert(declaration.id.name.to_string());
            }
            _ => {}
        }
    }
}
