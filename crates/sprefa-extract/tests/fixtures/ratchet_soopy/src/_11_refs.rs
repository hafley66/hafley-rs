//! Ref and tag mechanics. Named refs are enumerated through one
//! `git for-each-ref` invocation whose format uses NUL (`%00`) field separators
//! and newline record separators, so a full ref name is never split by line
//! parsing. Annotated-tag messages are the only multi-line field and are read
//! through a size-prefixed `git cat-file --batch` pass. HEAD is resolved
//! per-worktree from the open repository, while named refs come from the shared
//! ref store, so linked worktrees agree on refs but keep their own HEAD.

use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use globset::Glob;

use crate::_0_types::{
    Head, ObjectId, ObjectKind, RefDelta, RefObservation, RefQuery, RefSnapshot, Repository,
    TagMetadata, Tagger,
};

/// Nine NUL-separated fields per ref, newline-terminated per record. The
/// `%(*objectname)`/`%(*objecttype)` forms yield the peeled tag target and its
/// kind; `%00` keeps arbitrary valid ref names intact.
const FORMAT: &str = concat!(
    "%(refname)%00",
    "%(objectname)%00",
    "%(*objectname)%00",
    "%(objecttype)%00",
    "%(*objecttype)%00",
    "%(symref)%00",
    "%(taggername)%00",
    "%(taggeremail)%00",
    "%(taggerdate:unix)",
);

const FIELD_COUNT: usize = 9;
const REFNAME: usize = 0;
const OBJECTNAME: usize = 1;
const PEELED: usize = 2;
const OBJECTTYPE: usize = 3;
const PEELED_TYPE: usize = 4;
const SYMREF: usize = 5;
const TAGGER_NAME: usize = 6;
const TAGGER_EMAIL: usize = 7;
const TAGGER_WHEN: usize = 8;

/// An open handle over one repository checkout, mirroring `SourceTree`. It owns
/// the worktree used to resolve `HEAD`; named refs stay repository-scoped.
pub struct Refs {
    repository: Repository,
}

impl Refs {
    pub fn open(repository: Repository) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &Repository {
        &self.repository
    }

    /// Enumerate one ref query into a deterministic snapshot. `HEAD` resolves
    /// from this handle's checkout; named refs come from the shared ref store.
    pub fn snapshot(&self, query: &RefQuery) -> Result<RefSnapshot> {
        if query.repository != self.repository.identity {
            bail!("ref query belongs to another repository");
        }
        let head = resolve_head(&self.repository)?;
        let head_target = head_commit(&self.repository);
        let mut refs = enumerate(&self.repository, query)?;
        refs.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(RefSnapshot {
            repository: self.repository.identity.clone(),
            head,
            head_target,
            refs,
        })
    }
}

/// Classify HEAD and named-ref transitions between two snapshots of one
/// repository. A changed HEAD comes first; named refs then follow full ref
/// name order.
pub fn diff_refs(before: &RefSnapshot, after: &RefSnapshot) -> Vec<RefDelta> {
    let before_map: BTreeMap<_, _> = before
        .refs
        .iter()
        .map(|row| (row.name.clone(), row))
        .collect();
    let after_map: BTreeMap<_, _> = after
        .refs
        .iter()
        .map(|row| (row.name.clone(), row))
        .collect();
    let mut names: Vec<_> = before_map.keys().chain(after_map.keys()).cloned().collect();
    names.sort();
    names.dedup();
    let mut deltas = Vec::new();
    if before.head != after.head || before.head_target != after.head_target {
        deltas.push(RefDelta::HeadChanged {
            before: crate::_0_types::HeadObservation {
                state: before.head.clone(),
                target: before.head_target.clone(),
            },
            after: crate::_0_types::HeadObservation {
                state: after.head.clone(),
                target: after.head_target.clone(),
            },
        });
    }
    for name in names {
        match (before_map.get(&name), after_map.get(&name)) {
            (None, Some(after)) => deltas.push(RefDelta::Added((*after).clone())),
            (Some(before), None) => deltas.push(RefDelta::Removed((*before).clone())),
            (Some(before), Some(after)) if before != after => {
                deltas.push(RefDelta::Changed {
                    before: (*before).clone(),
                    after: (*after).clone(),
                });
            }
            _ => {}
        }
    }
    deltas
}

fn enumerate(repository: &Repository, query: &RefQuery) -> Result<Vec<RefObservation>> {
    let matcher = match query.pattern.as_ref() {
        Some(pattern) => Some(
            Glob::new(pattern)
                .with_context(|| format!("invalid ref glob pattern {pattern:?}"))?
                .compile_matcher(),
        ),
        None => None,
    };
    let exact = query.name.as_ref().map(|name| name.as_ref());

    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(&repository.root)
        .args(["for-each-ref", &format!("--format={FORMAT}")]);
    if !query.namespace.is_empty() {
        command.arg(query.namespace.as_ref());
    }
    let output = command.output().context("run git for-each-ref")?;
    if !output.status.success() {
        bail!("git for-each-ref failed in {}", repository.root.display());
    }
    let stdout = String::from_utf8(output.stdout).context("decode git for-each-ref output")?;

    let mut rows = Vec::new();
    for record in stdout.split('\n').filter(|record| !record.is_empty()) {
        let fields: Vec<&str> = record.split('\0').collect();
        if fields.len() != FIELD_COUNT {
            bail!("malformed git for-each-ref record");
        }
        let name = fields[REFNAME];
        if let Some(exact) = exact {
            if name != exact {
                continue;
            }
        } else if let Some(matcher) = &matcher {
            if !matcher.is_match(name) {
                continue;
            }
        }
        let kind = object_kind(fields[OBJECTTYPE])?;
        let tag = if kind == ObjectKind::Tag {
            Some(TagMetadata {
                target_kind: object_kind(fields[PEELED_TYPE])?,
                tagger: tagger(&fields)?,
                message: Arc::from(""),
            })
        } else {
            None
        };
        rows.push(RefObservation {
            repository: repository.identity.clone(),
            name: Arc::from(name),
            symbolic: optional_str(fields[SYMREF]),
            direct: ObjectId(Arc::from(fields[OBJECTNAME])),
            peeled: optional_oid(fields[PEELED]),
            kind,
            tag,
        });
    }
    fill_tag_messages(repository, &mut rows)?;
    Ok(rows)
}

fn resolve_head(repository: &Repository) -> Result<Head> {
    match symbolic_ref(repository) {
        Some(target) => {
            if head_commit(repository).is_some() {
                Ok(Head::Symbolic { target })
            } else {
                Ok(Head::Unborn { target })
            }
        }
        None => Ok(Head::Detached(
            head_commit(repository).context("resolve detached HEAD")?,
        )),
    }
}

fn symbolic_ref(repository: &Repository) -> Option<Arc<str>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["symbolic-ref", "-q", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(Arc::from(String::from_utf8(output.stdout).ok()?.trim()))
}

fn head_commit(repository: &Repository) -> Option<ObjectId> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["rev-parse", "-q", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(ObjectId(Arc::from(
        String::from_utf8(output.stdout).ok()?.trim(),
    )))
}

fn object_kind(value: &str) -> Result<ObjectKind> {
    match value {
        "blob" => Ok(ObjectKind::Blob),
        "tree" => Ok(ObjectKind::Tree),
        "commit" => Ok(ObjectKind::Commit),
        "tag" => Ok(ObjectKind::Tag),
        other => bail!("unknown Git object type {other:?}"),
    }
}

fn optional_oid(value: &str) -> Option<ObjectId> {
    (!value.is_empty()).then(|| ObjectId(Arc::from(value)))
}

fn optional_str(value: &str) -> Option<Arc<str>> {
    (!value.is_empty()).then(|| Arc::from(value))
}

fn tagger(fields: &[&str]) -> Result<Tagger> {
    let email = fields[TAGGER_EMAIL]
        .trim_start_matches('<')
        .trim_end_matches('>');
    let when: i64 = fields[TAGGER_WHEN]
        .parse()
        .context("parse tagger timestamp")?;
    Ok(Tagger {
        name: Arc::from(fields[TAGGER_NAME]),
        email: Arc::from(email),
        when,
    })
}

/// Fill the message field of every annotated-tag observation through one
/// size-prefixed `git cat-file --batch` pass. The message is the only field
/// that may span newlines, so it is deliberately kept out of the line-framed
/// `for-each-ref` output.
fn fill_tag_messages(repository: &Repository, rows: &mut [RefObservation]) -> Result<()> {
    let tag_oids: Vec<ObjectId> = rows
        .iter()
        .filter(|row| row.tag.is_some())
        .map(|row| row.direct.clone())
        .collect();
    if tag_oids.is_empty() {
        return Ok(());
    }
    let messages = tag_messages(repository, &tag_oids)?;
    for row in rows.iter_mut() {
        if let Some(message) = messages.get(&row.direct) {
            if let Some(tag) = &mut row.tag {
                tag.message = message.clone();
            }
        }
    }
    Ok(())
}

fn tag_messages(
    repository: &Repository,
    oids: &[ObjectId],
) -> Result<BTreeMap<ObjectId, Arc<str>>> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("start git cat-file --batch")?;
    {
        let input = child.stdin.as_mut().context("open cat-file stdin")?;
        for oid in oids {
            writeln!(input, "{}", oid.0).context("write cat-file object")?;
        }
    }
    let output = child
        .wait_with_output()
        .context("wait for git cat-file --batch")?;
    if !output.status.success() {
        bail!("git cat-file failed in {}", repository.root.display());
    }
    let mut rest: &[u8] = &output.stdout;
    let mut messages = BTreeMap::new();
    for oid in oids {
        let Some(header_end) = rest.iter().position(|byte| *byte == b'\n') else {
            bail!("git cat-file header missing for {}", oid.0);
        };
        let header =
            std::str::from_utf8(&rest[..header_end]).context("cat-file header not UTF-8")?;
        rest = &rest[header_end + 1..];
        let fields: Vec<&str> = header.split_whitespace().collect();
        if fields.len() != 3 {
            bail!("unexpected cat-file header for {}: {header:?}", oid.0);
        }
        let size: usize = fields[2].parse().context("parse tag object size")?;
        let body = &rest[..size];
        rest = &rest[size + 1..];
        messages.insert(oid.clone(), Arc::from(extract_message(body)));
    }
    Ok(messages)
}

/// Strip the fixed tag-object header (`object`, `type`, `tag`, `tagger`, then a
/// blank line) to recover the message. A signed tag's signature block remains
/// part of the returned text; signature stripping belongs to `ref-tag-snapshots`.
fn extract_message(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    match text.find("\n\n") {
        Some(separator) => text[separator + 2..].trim_end_matches('\n').to_string(),
        None => String::new(),
    }
}
