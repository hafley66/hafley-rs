//! `Inputs` -> one ordered, deduped file list. Directories and globs walk through
//! soopy (gitignore-aware worktree walk in a repository, directory snapshot outside).

use std::collections::HashSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use sprefa_extract::{source_for, SourcePattern};

use crate::cli::Inputs;

/// Every file the inputs name, in input order; a directory or glob expands in
/// path order. A file named twice keeps its first spelling.
pub fn expand(inputs: &Inputs) -> Result<Vec<PathBuf>, String> {
    let mut tokens = Vec::new();
    let mut stdin_read = false;
    for token in &inputs.paths {
        if token == "-" {
            if stdin_read {
                continue;
            }
            stdin_read = true;
            for line in std::io::stdin().lock().lines() {
                let line = line.map_err(|error| format!("stdin: {error}"))?;
                let line = line.trim();
                if !line.is_empty() {
                    tokens.push(line.to_string());
                }
            }
        } else {
            tokens.push(token.clone());
        }
    }
    if tokens.is_empty() && !inputs.patterns.is_empty() {
        tokens.push(".".to_string());
    }
    let mut files = Vec::new();
    for token in &tokens {
        let path = PathBuf::from(token);
        if path.is_dir() {
            files.extend(walk(&path, &inputs.patterns)?);
        } else if path.exists() {
            files.push(path);
        } else if is_glob(token) {
            let (base, rest) = split_glob(token);
            if !base.is_dir() {
                return Err(format!("{token} matched nothing"));
            }
            let found = walk(&base, &[rest])?;
            if found.is_empty() {
                return Err(format!("{token} matched nothing"));
            }
            files.extend(found);
        } else {
            return Err(format!("{token} does not exist"));
        }
    }
    if !inputs.entry.is_empty() {
        let universe = if files.is_empty() {
            walk(&project_dir(&inputs.entry[0]), &inputs.patterns)?
        } else {
            files
        };
        let root = inputs.root.clone().unwrap_or_else(|| project_dir(&inputs.entry[0]));
        files = sprefa_extract::reach_files(&root, &universe, &inputs.entry, inputs.depth)
            .map_err(|error| error.to_string())?;
    }
    let mut seen = HashSet::new();
    files.retain(|path| seen.insert(std::fs::canonicalize(path).unwrap_or_else(|_| path.clone())));
    Ok(files)
}

/// The corpus root a whole-project verb runs over: `--root`, else the one
/// directory input, else the git root of the working directory.
pub fn root(inputs: &Inputs) -> PathBuf {
    if let Some(root) = &inputs.root {
        return root.clone();
    }
    if let [only] = inputs.paths.as_slice() {
        if Path::new(only).is_dir() {
            return PathBuf::from(only);
        }
    }
    soopy::discover(".")
        .map(|repository| repository.root)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// The default `--root` for the repository verbs: the working directory's git root.
pub fn git_root_of_cwd() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(soopy::discover(".")
        .map_err(|error| format!("--root: {error:#}"))?
        .root)
}

/// Every roster-claimed file under `dir` that matches `patterns` (relative to
/// `dir`; empty means all), spelled `dir` joined with its path below `dir`.
fn walk(dir: &Path, patterns: &[String]) -> Result<Vec<PathBuf>, String> {
    let discovered = sprefa_extract::trace::stage_span("discover").in_scope(|| soopy::discover(dir));
    let mut found: Vec<PathBuf> = match discovered {
        Ok(repository) => {
            let absolute = std::fs::canonicalize(dir)
                .map_err(|error| format!("{}: {error}", dir.display()))?;
            let below = absolute
                .strip_prefix(&repository.root)
                .map_err(|_| format!("{} is outside {}", dir.display(), repository.root.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            let prefixed = |glob: &str| {
                if below.is_empty() {
                    glob.to_string()
                } else {
                    format!("{below}/{glob}")
                }
            };
            let globs: Vec<SourcePattern> = if patterns.is_empty() {
                vec![SourcePattern(prefixed("**"))]
            } else {
                patterns.iter().map(|glob| SourcePattern(prefixed(glob))).collect()
            };
            // Expansion keeps paths only; the worktree stamp is never read, so
            // it costs no `git rev-parse` / `git status`.
            let revision = soopy::RevisionId::Worktree {
                worktree: repository.worktree.clone(),
                head: None,
                dirty: false,
            };
            let mut tree = soopy::SourceTree::open(repository);
            sprefa_extract::trace::stage_span("enumerate")
                .in_scope(|| tree.enumerate(&revision, &globs))
                .map_err(|error| format!("{}: {error:#}", dir.display()))?
                .into_iter()
                .map(|entry| {
                    let path = entry.source.path.0.to_string();
                    let rest = if below.is_empty() {
                        path.as_str()
                    } else {
                        path.strip_prefix(&below).map_or(path.as_str(), |rest| rest.trim_start_matches('/'))
                    };
                    joined(dir, rest)
                })
                .collect()
        }
        Err(_) => {
            let mut root = soopy::DirectoryRoot::open(dir)
                .map_err(|error| format!("{}: {error:#}", dir.display()))?;
            let query = soopy::FileQuery {
                patterns: patterns.iter().map(|glob| SourcePattern(glob.clone())).collect(),
            };
            root.snapshot(&query)
                .map_err(|error| format!("{}: {error:#}", dir.display()))?
                .files
                .into_iter()
                .map(|entry| joined(dir, &entry.file.path.0))
                .collect()
        }
    };
    found.retain(|path| source_for(&path.to_string_lossy()).is_some());
    found.sort();
    Ok(found)
}

/// `dir` joined with a `/`-separated relative path, with a leading `./` dropped
/// so `.` expands to `src/lib.rs`, not `./src/lib.rs`.
fn joined(dir: &Path, rest: &str) -> PathBuf {
    if dir == Path::new(".") {
        PathBuf::from(rest)
    } else {
        dir.join(rest)
    }
}

fn is_glob(token: &str) -> bool {
    token.contains(['*', '?', '[', '{'])
}

/// The literal directory prefix of a glob and the glob below it.
fn split_glob(token: &str) -> (PathBuf, String) {
    let mut base = PathBuf::new();
    let mut rest = Vec::new();
    for component in Path::new(token).components() {
        let text = component.as_os_str().to_string_lossy().to_string();
        if rest.is_empty() && !is_glob(&text) {
            base.push(component);
        } else {
            rest.push(text);
        }
    }
    if rest.is_empty() {
        // Every component was literal; the last one names the glob target.
        if let Some(last) = base.file_name().map(|name| name.to_string_lossy().to_string()) {
            base.pop();
            rest.push(last);
        }
    }
    if base.as_os_str().is_empty() {
        base = PathBuf::from(".");
    }
    (base, rest.join("/"))
}

/// The nearest ancestor of `entry` holding a Cargo.toml, package.json or
/// go.mod; else the git root; else the entry's own directory.
fn project_dir(entry: &Path) -> PathBuf {
    let start = entry.parent().unwrap_or(Path::new("."));
    let mut dir = Some(start);
    while let Some(at) = dir {
        let at_or_dot = if at.as_os_str().is_empty() { Path::new(".") } else { at };
        if ["Cargo.toml", "package.json", "go.mod"]
            .iter()
            .any(|marker| at_or_dot.join(marker).is_file())
        {
            return at_or_dot.to_path_buf();
        }
        dir = at.parent();
    }
    soopy::discover(start)
        .map(|repository| repository.root)
        .unwrap_or_else(|_| start.to_path_buf())
}
