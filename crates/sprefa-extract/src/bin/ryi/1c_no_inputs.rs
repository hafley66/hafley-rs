use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ignore::{DirEntry, WalkBuilder};

const DISCOVERY_BUDGET: Duration = Duration::from_millis(50);

struct Project {
    language: &'static str,
    label: &'static str,
    root: PathBuf,
}

pub fn suggestions() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let capabilities: BTreeSet<String> = crate::capabilities::rows()
        .into_iter()
        .filter(|row| row["source"] == true)
        .filter_map(|row| row["language"].as_str().map(str::to_owned))
        .collect();
    let projects = discover_projects(&cwd, &capabilities);
    if projects.is_empty() {
        return "ryi: no inputs; pass files, directories, globs, - or --entry\nNo supported project found within the 50 ms discovery budget.".into();
    }

    let executable = std::env::current_exe()
        .ok()
        .map(|path| shell_word(&path.to_string_lossy()))
        .unwrap_or_else(|| "ryii".into());
    projects
        .iter()
        .map(|project| render(project, &executable))
        .collect::<Vec<_>>()
        .join("\n")
}

fn discover_projects(cwd: &Path, capabilities: &BTreeSet<String>) -> Vec<Project> {
    let started = Instant::now();
    let mut projects = Vec::new();
    let mut seen = BTreeSet::new();
    let mut walker = WalkBuilder::new(cwd);
    walker
        .standard_filters(true)
        .require_git(false)
        .hidden(false)
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_some_and(|kind| kind.is_dir())
                || ![".git", "target", ".boop-worktrees"]
                    .iter()
                    .any(|name| entry.file_name() == *name)
        });
    // Inspect the workspace root before spending the budget on child directories.
    'discovery: for depth in 1..=2 {
        walker.max_depth(Some(depth));
        for entry in walker.build() {
            if started.elapsed() >= DISCOVERY_BUDGET {
                break 'discovery;
            }
            let Ok(entry) = entry else { continue };
            if entry.depth() != depth {
                continue;
            }
            if let Some((language, label, root)) = project_for(&entry, cwd, capabilities) {
                let key = (language, root.clone());
                if seen.insert(key) {
                    projects.push(Project {
                        language,
                        label,
                        root,
                    });
                }
            }
        }
    }
    projects.sort_by(|left, right| left.root.cmp(&right.root));
    projects
}

fn project_for(
    entry: &DirEntry,
    cwd: &Path,
    capabilities: &BTreeSet<String>,
) -> Option<(&'static str, &'static str, PathBuf)> {
    let name = entry.file_name().to_str()?;
    let parent = entry.path().parent()?;
    let relative = parent.strip_prefix(cwd).ok()?;
    let root = if relative.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(".").join(relative)
    };

    if entry.file_type().is_some_and(|kind| kind.is_dir())
        && Path::new(name)
            .extension()
            .is_some_and(|extension| extension == "tsp")
        && capabilities.contains("typespec")
    {
        return Some(("typespec", "TypeSpec source", entry.path().to_path_buf()));
    }

    let (language, label) = match name {
        "Cargo.toml" if capabilities.contains("rust") => ("rust", "Rust workspace"),
        "package.json" if capabilities.contains("ts") => ("typescript", "TypeScript project"),
        "go.mod" if capabilities.contains("go") => ("go", "Go module"),
        "pyproject.toml" if capabilities.contains("python") => ("python", "Python project"),
        name if name.starts_with("build.gradle") && capabilities.contains("kotlin") => {
            ("kotlin", "Gradle project")
        }
        _ => return None,
    };
    Some((language, label, root))
}

fn render(project: &Project, executable: &str) -> String {
    let root = shell_word(&project.root.to_string_lossy());
    let project_key = project
        .root
        .to_string_lossy()
        .replace('/', "_")
        .replace('\\', "_")
        .replace('.', "_");
    let database = shell_word(&format!("facts-{}-{project_key}.db", project.language));
    let mut commands = vec![format!("{executable} --sqlite {database} {root}")];
    if project.language != "typespec" {
        commands.push(format!(
            "{executable} graph --callers main --root {root} {root}"
        ));
    }
    if project.language == "rust" {
        commands.push(format!("{executable} --root {root} --package-deps {root}"));
    }
    format!(
        "{} {} -> {}",
        project.language,
        project.label,
        commands.join("  |  ")
    )
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
