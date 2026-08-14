//! Revision graph queries: resolution, parents, ancestry, merge bases,
//! ahead/behind counts, and history walks.
//!
//! Read-only guarantees: every operation here runs plumbing commands
//! (`rev-parse`, `cat-file --batch`, `rev-list`, `merge-base`) that never
//! mutate the object database, refs, index, shallow file, or worktree. Fetch,
//! unshallow, and ref updates live exclusively in `_13_fetch` behind an
//! `AcquisitionPolicy`.
//!
//! Batching: parent lookup and commit-body reads go through one persistent
//! `git cat-file --batch` process per query; each walk is one `git rev-list`
//! invocation covering every reachable commit. Pairwise relations (`merge-base`,
//! `--is-ancestor`, `--left-right --count`) have no batch form in Git and are
//! one process each, documented in the lane report.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::_0_types::{
    AheadBehind, Ancestry, CommitParents, CommitWalk, MergeBase, ObjectId, Repository, Revision,
    RevisionGraphQuery, RevisionGraphResult, RevisionResolution,
};

/// An open handle over one repository for read-only revision-graph queries.
pub struct RevisionGraph {
    repository: Repository,
}

impl RevisionGraph {
    pub fn open(repository: Repository) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &Repository {
        &self.repository
    }

    /// Execute one batched revision-graph query. Every result vector is
    /// parallel to its request list and preserves input order.
    pub fn query(&self, query: &RevisionGraphQuery) -> Result<RevisionGraphResult> {
        if query.repository != self.repository.identity {
            bail!("revision graph query belongs to another repository");
        }
        let resolutions = query
            .resolve
            .iter()
            .map(|revision| resolve(&self.repository, revision))
            .collect::<Result<Vec<_>>>()?;
        let parents = commit_parents(&self.repository, &query.parents)?;
        let ancestry = query
            .ancestry
            .iter()
            .map(|(ancestor, descendant)| is_ancestor(&self.repository, ancestor, descendant))
            .collect::<Result<Vec<_>>>()?;
        let merge_bases = query
            .merge_bases
            .iter()
            .map(|(left, right)| merge_base(&self.repository, left, right))
            .collect::<Result<Vec<_>>>()?;
        let ahead_behind = query
            .ahead_behind
            .iter()
            .map(|(left, right)| ahead_behind(&self.repository, left, right))
            .collect::<Result<Vec<_>>>()?;
        let walks = query
            .walks
            .iter()
            .map(|revision| walk(&self.repository, revision))
            .collect::<Result<Vec<_>>>()?;
        Ok(RevisionGraphResult {
            repository: self.repository.identity.clone(),
            resolutions,
            parents,
            ancestry,
            merge_bases,
            ahead_behind,
            walks,
        })
    }
}

/// Resolve one revision to a commit and classify it as present, absent, a
/// shallow boundary, or corrupt. A single `git rev-parse --verify --quiet
/// <expr>^{commit}` both peels (tags) and validates that the target is a real,
/// readable commit; only the shallow boundary needs an extra look at the
/// `.git/shallow` set, and only a corrupt object produces `corrupt`/`unable to
/// unpack` on stderr.
fn resolve(repository: &Repository, revision: &Revision) -> Result<RevisionResolution> {
    let expression = match revision {
        Revision::Worktree => "HEAD".to_string(),
        Revision::Named(name) => name.as_ref().to_string(),
        Revision::Commit(oid) => oid.0.as_ref().to_string(),
    };
    let (oid, corrupt) = rev_parse_commit(repository, &expression)?;
    let Some(oid) = oid else {
        return Ok(if corrupt {
            RevisionResolution::CorruptObject
        } else {
            RevisionResolution::Absent
        });
    };
    if shallow_set(repository)?.contains(&oid) {
        Ok(RevisionResolution::ShallowBoundary(oid))
    } else {
        Ok(RevisionResolution::Present(oid))
    }
}

fn rev_parse_commit(repository: &Repository, expression: &str) -> Result<(Option<ObjectId>, bool)> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{expression}^{{commit}}"),
        ])
        .output()
        .with_context(|| format!("resolve {expression:?}"))?;
    if output.status.success() {
        let oid = ObjectId(Arc::from(
            String::from_utf8(output.stdout)?.trim().to_string(),
        ));
        return Ok((Some(oid), false));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let corrupt = ["corrupt", "unable to unpack", "inflate"]
        .iter()
        .any(|needle| stderr.contains(needle));
    Ok((None, corrupt))
}

/// The set of commit OIDs listed in `.git/shallow`. Empty for a full clone.
fn shallow_set(repository: &Repository) -> Result<BTreeSet<ObjectId>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "shallow",
        ])
        .output()
        .context("locate shallow file")?;
    if !output.status.success() {
        bail!("git rev-parse --git-path shallow failed");
    }
    let path = std::path::PathBuf::from(String::from_utf8(output.stdout)?.trim());
    let mut set = BTreeSet::new();
    if let Ok(text) = std::fs::read_to_string(&path) {
        for line in text.lines() {
            let line = line.trim();
            if !line.is_empty() {
                set.insert(ObjectId(Arc::from(line.to_string())));
            }
        }
    }
    Ok(set)
}

/// The direct parents of every requested commit, one persistent
/// `git cat-file --batch` process reading commit bodies and extracting the
/// `parent` lines in recorded order.
fn commit_parents(repository: &Repository, oids: &[ObjectId]) -> Result<Vec<CommitParents>> {
    if oids.is_empty() {
        return Ok(Vec::new());
    }
    let mut batch = CommitBatch::open(&repository.root)?;
    oids.iter()
        .map(|oid| {
            let body = batch.commit_body(oid)?;
            let parents = body
                .split(|byte| *byte == b'\n')
                .take_while(|line| !line.is_empty())
                .filter_map(|line| line.strip_prefix(b"parent "))
                .filter_map(|oid| std::str::from_utf8(oid).ok())
                .map(|oid| ObjectId(Arc::from(oid.to_string())))
                .collect();
            Ok(CommitParents {
                commit: oid.clone(),
                parents,
            })
        })
        .collect()
}

fn is_ancestor(
    repository: &Repository,
    ancestor: &ObjectId,
    descendant: &ObjectId,
) -> Result<Ancestry> {
    let status = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["merge-base", "--is-ancestor", &ancestor.0, &descendant.0])
        .status()
        .with_context(|| format!("is-ancestor {} {}", ancestor.0, descendant.0))?;
    let is_ancestor = match status.code() {
        Some(0) => true,
        Some(1) => false,
        _ => bail!(
            "git merge-base --is-ancestor failed for {} {}",
            ancestor.0,
            descendant.0
        ),
    };
    Ok(Ancestry {
        ancestor: ancestor.clone(),
        descendant: descendant.clone(),
        is_ancestor,
    })
}

fn merge_base(repository: &Repository, left: &ObjectId, right: &ObjectId) -> Result<MergeBase> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["merge-base", "--all", &left.0, &right.0])
        .output()
        .with_context(|| format!("merge-base {} {}", left.0, right.0))?;
    if !output.status.success() {
        bail!("git merge-base failed for {} {}", left.0, right.0);
    }
    let mut bases: Vec<ObjectId> = String::from_utf8(output.stdout)?
        .lines()
        .map(|line| ObjectId(Arc::from(line.to_string())))
        .collect();
    bases.sort();
    Ok(MergeBase {
        left: left.clone(),
        right: right.clone(),
        bases,
    })
}

fn ahead_behind(repository: &Repository, left: &ObjectId, right: &ObjectId) -> Result<AheadBehind> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args([
            "rev-list",
            "--left-right",
            "--count",
            &format!("{}...{}", left.0, right.0),
        ])
        .output()
        .with_context(|| format!("ahead/behind {} {}", left.0, right.0))?;
    if !output.status.success() {
        bail!("git rev-list --left-right --count failed");
    }
    let text = String::from_utf8(output.stdout)?;
    let mut fields = text.split_whitespace();
    let ahead: u64 = fields.next().context("missing ahead count")?.parse()?;
    let behind: u64 = fields.next().context("missing behind count")?.parse()?;
    Ok(AheadBehind {
        left: left.clone(),
        right: right.clone(),
        ahead,
        behind,
    })
}

/// Walk every commit reachable from a revision, peeling lightweight and
/// annotated tags first. One `git rev-list --topo-order` invocation returns a
/// stable, newest-first traversal independent of commit timestamps.
fn walk(repository: &Repository, revision: &Revision) -> Result<CommitWalk> {
    let resolution = resolve(repository, revision)?;
    let start = match resolution {
        RevisionResolution::Present(oid) | RevisionResolution::ShallowBoundary(oid) => oid,
        RevisionResolution::Absent => bail!("cannot walk a revision that does not resolve"),
        RevisionResolution::CorruptObject => bail!("cannot walk a corrupt object"),
    };
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["rev-list", "--topo-order", &start.0])
        .output()
        .with_context(|| format!("walk {}", start.0))?;
    if !output.status.success() {
        bail!("git rev-list failed for {}", start.0);
    }
    let commits = String::from_utf8(output.stdout)?
        .lines()
        .map(|line| ObjectId(Arc::from(line.to_string())))
        .collect();
    Ok(CommitWalk { start, commits })
}

/// A persistent `git cat-file --batch` reader restricted to commit bodies.
struct CommitBatch {
    child: Child,
    input: ChildStdin,
    output: BufReader<std::process::ChildStdout>,
}

impl CommitBatch {
    fn open(root: &std::path::Path) -> Result<Self> {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("start git cat-file --batch")?;
        let input = child.stdin.take().context("open cat-file stdin")?;
        let output = BufReader::new(child.stdout.take().context("open cat-file stdout")?);
        Ok(Self {
            child,
            input,
            output,
        })
    }

    fn commit_body(&mut self, oid: &ObjectId) -> Result<Vec<u8>> {
        writeln!(self.input, "{}", oid.0)?;
        self.input.flush()?;
        let mut header = String::new();
        self.output.read_line(&mut header)?;
        let fields: Vec<&str> = header.split_whitespace().collect();
        if fields.len() != 3 || fields[1] != "commit" {
            bail!(
                "git cat-file did not return a commit for {}: {}",
                oid.0,
                header.trim()
            );
        }
        let size: usize = fields[2].parse().context("parse commit size")?;
        let mut body = vec![0; size];
        self.output.read_exact(&mut body)?;
        let mut newline = [0u8; 1];
        self.output.read_exact(&mut newline)?;
        Ok(body)
    }
}

impl Drop for CommitBatch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
