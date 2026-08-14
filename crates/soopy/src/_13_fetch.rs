//! Acquisition: explicit, policy-gated object and ref acquisition.
//!
//! Read-only graph queries in `_12_revision_graph` never fetch, unshallow, or
//! update refs. Acquisition is the only surface that may do so, and every
//! operation is gated by an `AcquisitionPolicy` before any Git process runs.
//! A default policy rejects everything, so a caller that does not opt in gets
//! `RejectedByPolicy` for every network mutation.

use std::process::Command;
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::_0_types::{
    AcquisitionOperation, AcquisitionOutcome, AcquisitionPolicy, AcquisitionReceipt,
    AcquisitionRequest, ObjectId, Repository,
};

/// An open handle over one repository for policy-gated acquisition.
pub struct Acquisition {
    repository: Repository,
}

impl Acquisition {
    pub fn open(repository: Repository) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &Repository {
        &self.repository
    }

    /// Execute a batch of acquisition operations, returning one typed receipt
    /// per operation in request order. An operation the policy rejects is a
    /// `RejectedByPolicy` receipt, not an error: the caller distinguishes the
    /// two outcomes without catching a failure.
    pub fn execute(
        &self,
        policy: &AcquisitionPolicy,
        request: &AcquisitionRequest,
    ) -> Result<Vec<AcquisitionOutcome>> {
        if request.repository != self.repository.identity {
            bail!("acquisition request belongs to another repository");
        }
        for operation in &request.operations {
            if policy_allows(policy, operation) {
                validate_operation(&self.repository, operation)?;
            }
        }
        request
            .operations
            .iter()
            .map(|operation| {
                self.execute_one(policy, operation)
                    .map(|receipt| AcquisitionOutcome {
                        operation: operation.clone(),
                        receipt,
                    })
            })
            .collect()
    }

    fn execute_one(
        &self,
        policy: &AcquisitionPolicy,
        operation: &AcquisitionOperation,
    ) -> Result<AcquisitionReceipt> {
        match operation {
            AcquisitionOperation::FetchRef { remote, name } => self.fetch_ref(policy, remote, name),
            AcquisitionOperation::FetchTag { remote, name } => self.fetch_tag(policy, remote, name),
            AcquisitionOperation::Deepen { remote, depth } => self.deepen(policy, remote, *depth),
            AcquisitionOperation::Unshallow { remote } => self.unshallow(policy, remote),
        }
    }

    fn fetch_ref(
        &self,
        policy: &AcquisitionPolicy,
        remote: &str,
        name: &str,
    ) -> Result<AcquisitionReceipt> {
        let tracking_ref = format!("refs/remotes/{remote}/{name}");
        if let Some(direct) = resolve_object(&self.repository, &tracking_ref)? {
            return Ok(AcquisitionReceipt::AlreadyPresent {
                peeled: resolve_commit(&self.repository, &tracking_ref)?,
                direct,
            });
        }
        if !policy.allow_fetch {
            return Ok(AcquisitionReceipt::RejectedByPolicy);
        }
        // Fetch the branch and update the remote-tracking ref so the result is
        // observable afterwards through a stable ref name.
        let refspec = format!("refs/heads/{name}:refs/remotes/{remote}/{name}");
        if let Some(reason) = git_fetch(&self.repository, &[], remote, &[refspec.as_str()]) {
            return Ok(AcquisitionReceipt::Unavailable {
                reason: Arc::from(reason),
            });
        }
        let target = resolve_commit(&self.repository, &tracking_ref)?
            .context("fetched ref did not resolve")?;
        Ok(AcquisitionReceipt::FetchedRef {
            name: Arc::from(name),
            target,
        })
    }

    fn fetch_tag(
        &self,
        policy: &AcquisitionPolicy,
        remote: &str,
        name: &str,
    ) -> Result<AcquisitionReceipt> {
        let tag_ref = format!("refs/tags/{name}");
        if let Some(direct) = resolve_object(&self.repository, &tag_ref)? {
            return Ok(AcquisitionReceipt::AlreadyPresent {
                peeled: resolve_commit(&self.repository, &tag_ref)?,
                direct,
            });
        }
        if !policy.allow_tag_fetch {
            return Ok(AcquisitionReceipt::RejectedByPolicy);
        }
        let refspec = format!("{tag_ref}:{tag_ref}");
        if let Some(reason) = git_fetch(&self.repository, &[], remote, &[refspec.as_str()]) {
            return Ok(AcquisitionReceipt::Unavailable {
                reason: Arc::from(reason),
            });
        }
        let direct = resolve_object(&self.repository, &tag_ref)?
            .context("fetched tag object did not resolve")?;
        let peeled = resolve_commit(&self.repository, &tag_ref)?;
        Ok(AcquisitionReceipt::FetchedTag {
            name: Arc::from(name),
            direct,
            peeled,
        })
    }

    fn deepen(
        &self,
        policy: &AcquisitionPolicy,
        remote: &str,
        depth: u32,
    ) -> Result<AcquisitionReceipt> {
        if !policy.allow_unshallow {
            return Ok(AcquisitionReceipt::RejectedByPolicy);
        }
        if !is_shallow(&self.repository)? {
            return Ok(AcquisitionReceipt::AlreadyComplete);
        }
        let flag = format!("--deepen={depth}");
        if let Some(reason) = git_fetch(&self.repository, &[flag.as_str()], remote, &[]) {
            return Ok(AcquisitionReceipt::Unavailable {
                reason: Arc::from(reason),
            });
        }
        Ok(AcquisitionReceipt::Deepened { depth })
    }

    fn unshallow(&self, policy: &AcquisitionPolicy, remote: &str) -> Result<AcquisitionReceipt> {
        if !policy.allow_unshallow {
            return Ok(AcquisitionReceipt::RejectedByPolicy);
        }
        if !is_shallow(&self.repository)? {
            return Ok(AcquisitionReceipt::AlreadyComplete);
        }
        if let Some(reason) = git_fetch(&self.repository, &["--unshallow"], remote, &[]) {
            return Ok(AcquisitionReceipt::Unavailable {
                reason: Arc::from(reason),
            });
        }
        Ok(AcquisitionReceipt::Unshallowed)
    }
}

fn policy_allows(policy: &AcquisitionPolicy, operation: &AcquisitionOperation) -> bool {
    match operation {
        AcquisitionOperation::FetchRef { .. } => policy.allow_fetch,
        AcquisitionOperation::FetchTag { .. } => policy.allow_tag_fetch,
        AcquisitionOperation::Deepen { .. } | AcquisitionOperation::Unshallow { .. } => {
            policy.allow_unshallow
        }
    }
}

/// Resolve a ref name to its commit OID, or `None` when it does not resolve.
fn resolve_object(repository: &Repository, name: &str) -> Result<Option<ObjectId>> {
    resolve_expression(repository, name)
}

fn resolve_commit(repository: &Repository, name: &str) -> Result<Option<ObjectId>> {
    resolve_expression(repository, &format!("{name}^{{commit}}"))
}

fn resolve_expression(repository: &Repository, expression: &str) -> Result<Option<ObjectId>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["rev-parse", "--verify", "--quiet", expression])
        .output()
        .with_context(|| format!("resolve ref {expression:?}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(ObjectId(Arc::from(
        String::from_utf8(output.stdout)?.trim().to_string(),
    ))))
}

fn validate_operation(repository: &Repository, operation: &AcquisitionOperation) -> Result<()> {
    let (remote, kind, name) = match operation {
        AcquisitionOperation::FetchRef { remote, name } => {
            (remote.as_ref(), Some("heads"), Some(name.as_ref()))
        }
        AcquisitionOperation::FetchTag { remote, name } => {
            (remote.as_ref(), Some("tags"), Some(name.as_ref()))
        }
        AcquisitionOperation::Deepen { remote, depth } => {
            if *depth == 0 {
                bail!("deepen depth must be greater than zero");
            }
            (remote.as_ref(), None, None)
        }
        AcquisitionOperation::Unshallow { remote } => (remote.as_ref(), None, None),
    };
    if remote.is_empty() || remote.starts_with('-') || remote.contains(['\0', '\n', '\r', ':']) {
        bail!("invalid Git remote name {remote:?}");
    }
    let remote_status = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["remote", "get-url", "--", remote])
        .output()
        .with_context(|| format!("validate remote {remote:?}"))?;
    if !remote_status.status.success() {
        bail!("unknown Git remote {remote:?}");
    }
    if let (Some(kind), Some(name)) = (kind, name) {
        let full = format!("refs/{kind}/{name}");
        let status = Command::new("git")
            .args(["check-ref-format", &full])
            .status()
            .with_context(|| format!("validate ref name {name:?}"))?;
        if !status.success() {
            bail!("invalid Git {kind} name {name:?}");
        }
    }
    Ok(())
}

fn is_shallow(repository: &Repository) -> Result<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["rev-parse", "--is-shallow-repository"])
        .output()
        .context("detect shallow repository")?;
    if !output.status.success() {
        bail!("git rev-parse --is-shallow-repository failed");
    }
    Ok(String::from_utf8(output.stdout)?.trim() == "true")
}

/// Run `git fetch <remote> <args...>`. Returns `None` on success and the
/// captured stderr on failure, used as the `Unavailable` receipt reason.
fn git_fetch(
    repository: &Repository,
    options: &[&str],
    remote: &str,
    refspecs: &[&str],
) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .arg("fetch")
        .args(options)
        .arg("--")
        .arg(remote)
        .args(refspecs)
        .output();
    match output {
        Ok(output) if output.status.success() => None,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let reason = stderr.lines().last().unwrap_or("fetch failed");
            Some(reason.trim().to_string())
        }
        Err(error) => Some(error.to_string()),
    }
}
