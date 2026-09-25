//! The tracked-file surface used by source-language hosts. It deliberately
//! keeps Git pathspec semantics separate from `SourceQuery`'s filesystem glob
//! semantics: this is `git ls-files` / `--with-tree`, including its index
//! behavior, with no local path matcher substituted in between.

use std::io::Write;
use std::path::{Component, Path};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::_0_types::{
    ContentId, GitFilesQuery, ObjectId, RepoPath, Repository, RevisionId, SourceEntry, SourceRef,
};

pub fn enumerate(repository: &Repository, query: &GitFilesQuery) -> Result<Vec<SourceEntry>> {
    enumerate_from(repository, query, &repository.root)
}

/// The Git CLI interprets ordinary pathspecs relative to its current working
/// directory. Hosts preserve that contract while Soopy opens repositories by
/// their root, so translate only ordinary relative pathspecs before `-C root`.
pub fn enumerate_from(
    repository: &Repository,
    query: &GitFilesQuery,
    cwd: &Path,
) -> Result<Vec<SourceEntry>> {
    let revision = crate::_3_revision::resolve(repository, query.revision.clone())?;
    let pathspecs: Vec<String> = query
        .pathspecs
        .iter()
        .map(|pathspec| pathspec_at(repository, cwd, pathspec))
        .collect();
    let paths = listed_paths(repository, &revision, &pathspecs)?;
    let rows = match &revision {
        RevisionId::Worktree { .. } => worktree_rows(repository, &revision, &paths)?,
        RevisionId::Commit(commit) => commit_rows(repository, &revision, commit, &paths)?,
    };
    Ok(rows)
}

fn pathspec_at(repository: &Repository, cwd: &Path, pathspec: &str) -> String {
    if pathspec.starts_with(':') || Path::new(pathspec).is_absolute() {
        return pathspec.to_string();
    }
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let Ok(prefix) = cwd.strip_prefix(&repository.root) else {
        return pathspec.to_string();
    };
    let mut components = Vec::new();
    for component in prefix.components().chain(Path::new(pathspec).components()) {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                components.pop();
            }
            Component::Normal(part) => components.push(part.to_os_string()),
            Component::RootDir | Component::Prefix(_) => return pathspec.to_string(),
        }
    }
    components
        .iter()
        .collect::<std::path::PathBuf>()
        .to_string_lossy()
        .replace('\\', "/")
}

fn listed_paths(
    repository: &Repository,
    revision: &RevisionId,
    pathspecs: &[String],
) -> Result<Vec<String>> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(&repository.root)
        .args(["ls-files", "-z"]);
    if let RevisionId::Commit(commit) = revision {
        command.arg(format!("--with-tree={}", commit.0));
    }
    command.arg("--").args(pathspecs);
    let output = command.output().context("run git ls-files")?;
    if !output.status.success() {
        bail!("git ls-files failed in {}", repository.root.display());
    }
    let mut paths = Vec::new();
    for record in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(record).with_context(|| {
            format!(
                "non-UTF-8 tracked path is not supported: {:?}",
                String::from_utf8_lossy(record)
            )
        })?;
        crate::_1a_path::ensure_line_safe(path)?;
        paths.push(path.to_string());
    }
    Ok(paths)
}

fn worktree_rows(
    repository: &Repository,
    revision: &RevisionId,
    paths: &[String],
) -> Result<Vec<SourceEntry>> {
    let child = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["hash-object", "--stdin-paths"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("start git hash-object --stdin-paths")?;
    let mut input = Vec::new();
    for path in paths {
        writeln!(input, "{path}").context("buffer git hash-object path")?;
    }
    let output = communicate(child, input, "git hash-object --stdin-paths")?;
    if !output.status.success() {
        bail!("git hash-object failed in {}", repository.root.display());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let oids: Vec<&str> = stdout.lines().filter(|line| !line.is_empty()).collect();
    if oids.len() != paths.len() {
        bail!(
            "git hash-object answered {} digests for {} tracked paths",
            oids.len(),
            paths.len()
        );
    }
    Ok(paths
        .iter()
        .zip(oids)
        .filter_map(|(path, oid)| entry(repository, revision, path, oid, false))
        .collect())
}

fn commit_rows(
    repository: &Repository,
    revision: &RevisionId,
    commit: &ObjectId,
    paths: &[String],
) -> Result<Vec<SourceEntry>> {
    let child = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args([
            "cat-file",
            "--batch-check=%(objectname) %(objecttype) %(objectsize)",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("start git cat-file --batch-check")?;
    let mut input = Vec::new();
    for path in paths {
        writeln!(input, "{}:{path}", commit.0).context("buffer git cat-file object")?;
    }
    let output = communicate(child, input, "git cat-file --batch-check")?;
    if !output.status.success() {
        bail!("git cat-file failed in {}", repository.root.display());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let answers: Vec<&str> = stdout.lines().collect();
    if answers.len() != paths.len() {
        bail!(
            "git cat-file answered {} rows for {} tracked paths",
            answers.len(),
            paths.len()
        );
    }
    Ok(paths
        .iter()
        .zip(answers)
        .filter_map(|(path, answer)| {
            let fields: Vec<&str> = answer.split_whitespace().collect();
            match fields.as_slice() {
                [oid, "blob", size] => {
                    entry_with_size(repository, revision, path, oid, size.parse().ok())
                }
                _ => None,
            }
        })
        .collect())
}

/// Feed a Git batch process while its stdout is drained by `wait_with_output`.
/// Writing the complete request stream first deadlocks once both pipe buffers
/// fill on repository-sized batches.
fn communicate(mut child: Child, input: Vec<u8>, operation: &'static str) -> Result<Output> {
    let mut stdin = child.stdin.take().context("open Git batch stdin")?;
    let writer = std::thread::spawn(move || -> std::io::Result<()> { stdin.write_all(&input) });
    let output = child
        .wait_with_output()
        .with_context(|| format!("wait for {operation}"))?;
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("{operation} input writer panicked"))?
        .with_context(|| format!("write {operation} input"))?;
    Ok(output)
}

fn entry(
    repository: &Repository,
    revision: &RevisionId,
    path: &str,
    oid: &str,
    immutable_size: bool,
) -> Option<SourceEntry> {
    let size = if immutable_size {
        None
    } else {
        std::fs::metadata(repository.root.join(path))
            .ok()
            .map(|metadata| metadata.len())
    };
    entry_with_size(repository, revision, path, oid, size)
}

/// Hash arbitrary bytes as a Git blob and return its object ID, matching the
/// `git hash-object` OID that the tracked-file surface emits for worktree
/// content. Used by `read_many` to round-trip a `GitBlob` identity.
pub fn hash_object(repository: &Repository, bytes: &[u8]) -> Result<ObjectId> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("start git hash-object --stdin")?;
    {
        let input = child.stdin.as_mut().context("open git hash-object stdin")?;
        input
            .write_all(bytes)
            .context("write git hash-object bytes")?;
    }
    let output = child
        .wait_with_output()
        .context("wait for git hash-object --stdin")?;
    if !output.status.success() {
        bail!("git hash-object failed in {}", repository.root.display());
    }
    Ok(ObjectId(Arc::from(
        String::from_utf8(output.stdout)?.trim(),
    )))
}

fn entry_with_size(
    repository: &Repository,
    revision: &RevisionId,
    path: &str,
    oid: &str,
    size: Option<u64>,
) -> Option<SourceEntry> {
    (!oid.is_empty()).then(|| SourceEntry {
        source: SourceRef {
            repository: repository.identity.clone(),
            revision: revision.clone(),
            path: RepoPath(Arc::from(path)),
        },
        // Worktree `hash-object` and committed tree blobs use Git's OID text;
        // consumers can compare both against the existing host digest column.
        content: ContentId::GitBlob(ObjectId(Arc::from(oid))),
        size: size.unwrap_or(0),
    })
}
