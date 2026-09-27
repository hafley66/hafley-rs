//! `Inputs` -> one ordered, deduped file list. Directories and globs walk through
//! soopy (gitignore-aware worktree walk in a repository, directory snapshot outside).

use std::collections::HashSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use sprefa_extract::{source_for, SourcePattern};

use crate::cli::Inputs;

/// Every file the inputs name, in input order; a directory or glob expands in
/// path order. A file named twice keeps its first spelling.
pub fn expand(inputs: &Inputs) -> Result<Vec<PathBuf>, String> {
    let mut tokens = Vec::new();
    let mut stdin_read = false;
    for token in &inputs.paths {
        if token.as_os_str() == "-" {
            if stdin_read {
                continue;
            }
            stdin_read = true;
            let stdin = std::io::stdin();
            let reader: Box<dyn BufRead + '_> = match crate::ops::request_input_file() {
                Some(input) => Box::new(std::io::BufReader::new(
                    input.reopen().map_err(|error| format!("stdin: {error}"))?,
                )),
                None => Box::new(stdin.lock()),
            };
            for line in reader.lines() {
                let line = line.map_err(|error| format!("stdin: {error}"))?;
                let line = line.trim();
                if !line.is_empty() {
                    tokens.push(PathBuf::from(line));
                }
            }
        } else {
            tokens.push(token.clone());
        }
    }
    if tokens.is_empty() && !inputs.patterns.is_empty() {
        tokens.push(crate::ops::request_root());
    }
    let mut files = Vec::new();
    for token in &tokens {
        let path = token;
        if sprefa_extract::io_path(path).is_dir() {
            files.extend(walk(path, &inputs.patterns)?);
        } else if sprefa_extract::io_path(path).exists() {
            files.push(path.clone());
        } else if is_glob(&token.to_string_lossy()) {
            let (base, rest) = split_glob(&token.to_string_lossy());
            if !sprefa_extract::io_path(&base).is_dir() {
                return Err(format!("{} matched nothing", token.display()));
            }
            let found = walk(&base, &[rest])?;
            if found.is_empty() {
                return Err(format!("{} matched nothing", token.display()));
            }
            files.extend(found);
        } else {
            return Err(format!("{} does not exist", token.display()));
        }
    }
    if !inputs.entry.is_empty() {
        let universe = if files.is_empty() {
            walk(&project_dir(&inputs.entry[0]), &inputs.patterns)?
        } else {
            files
        };
        let root = inputs
            .root
            .clone()
            .unwrap_or_else(|| project_dir(&inputs.entry[0]));
        files = sprefa_extract::reach_files(&root, &universe, &inputs.entry, inputs.depth)
            .map_err(|error| error.to_string())?;
    }
    let mut seen = HashSet::new();
    files.retain(|path| {
        seen.insert(
            std::fs::canonicalize(sprefa_extract::io_path(path))
                .unwrap_or_else(|_| sprefa_extract::io_path(path)),
        )
    });
    Ok(files)
}

/// The corpus root a whole-project verb runs over: `--root`, else the one
/// directory input, else the git root of the working directory.
pub fn root(inputs: &Inputs) -> PathBuf {
    if let Some(root) = &inputs.root {
        return root.clone();
    }
    if let [only] = inputs.paths.as_slice() {
        if sprefa_extract::io_path(only).is_dir() {
            return only.clone();
        }
    }
    soopy::discover(crate::ops::request_root())
        .map(|repository| repository.root)
        .unwrap_or_else(|_| crate::ops::request_root())
}

/// The default `--root` for the repository verbs: the working directory's git root.
pub fn git_root_of_cwd() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(soopy::discover(crate::ops::request_root())
        .map_err(|error| format!("--root: {error:#}"))?
        .root)
}

/// Every roster-claimed file under `dir` that matches `patterns` (relative to
/// `dir`; empty means all), spelled `dir` joined with its path below `dir`.
fn walk(dir: &Path, patterns: &[String]) -> Result<Vec<PathBuf>, String> {
    let io_dir = sprefa_extract::io_path(dir);
    let absolute =
        std::fs::canonicalize(&io_dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let mut matcher = GlobSetBuilder::new();
    if patterns.is_empty() {
        matcher.add(Glob::new("**").map_err(|error| error.to_string())?);
    } else {
        for pattern in patterns {
            matcher.add(
                Glob::new(pattern).map_err(|error| format!("invalid glob {pattern:?}: {error}"))?,
            );
        }
    }
    let matcher: Arc<GlobSet> = Arc::new(matcher.build().map_err(|error| error.to_string())?);
    let found = Arc::new(Mutex::new(Vec::new()));
    let failure = Arc::new(Mutex::new(None));
    let mut walker = WalkBuilder::new(&absolute);
    walker.hidden(false).filter_entry(|entry| {
        if entry.file_name() == ".git" {
            return false;
        }
        !(entry.depth() >= 1
            && entry.file_type().is_some_and(|kind| kind.is_dir())
            && entry.path().join(".git").exists())
    });
    walker.build_parallel().run(|| {
        let matcher = Arc::clone(&matcher);
        let found = Arc::clone(&found);
        let failure = Arc::clone(&failure);
        let absolute = absolute.clone();
        let dir = dir.to_path_buf();
        Box::new(move |result| match result {
            Ok(entry) => {
                if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                    return ignore::WalkState::Continue;
                }
                let relative = match entry.path().strip_prefix(&absolute) {
                    Ok(relative) => relative,
                    Err(error) => {
                        *failure.lock().unwrap() = Some(format!("{}: {error}", dir.display()));
                        return ignore::WalkState::Quit;
                    }
                };
                if matcher.is_match(relative) {
                    if let Some(relative) = relative.to_str() {
                        if source_for(relative).is_some() {
                            found.lock().unwrap().push(joined(&dir, relative));
                        }
                    }
                }
                ignore::WalkState::Continue
            }
            Err(error) => {
                *failure.lock().unwrap() = Some(format!("{}: {error}", dir.display()));
                ignore::WalkState::Quit
            }
        })
    });
    if let Some(error) = failure.lock().unwrap().take() {
        return Err(error);
    }
    let mut found = Arc::try_unwrap(found).unwrap().into_inner().unwrap();
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
        if let Some(last) = base
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        {
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
        let at_or_dot = if at.as_os_str().is_empty() {
            Path::new(".")
        } else {
            at
        };
        if ["Cargo.toml", "package.json", "go.mod"]
            .iter()
            .any(|marker| sprefa_extract::io_path(&at_or_dot.join(marker)).is_file())
        {
            return at_or_dot.to_path_buf();
        }
        dir = at.parent();
    }
    soopy::discover(sprefa_extract::io_path(start))
        .map(|repository| repository.root)
        .unwrap_or_else(|_| start.to_path_buf())
}
