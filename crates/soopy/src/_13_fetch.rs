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
    AcquisitionOperation, AcquisitionPolicy, AcquisitionReceipt, AcquisitionRequest, ObjectId,
    Repository,
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
    ) -> Result<Vec<AcquisitionReceipt>> {
        if request.repository != self.repository.identity {
            bail!("acquisition request belongs to another repository");
        }
        request
            .operations
            .iter()
            .map(|operation| self.execute_one(policy, operation))
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
        // A locally resolvable ref is already present; no fetch is attempted.
        if let Some(target) = resolve_ref(&self.repository, name)? {
            return Ok(AcquisitionReceipt::AlreadyPresent { target });
        }
        if !policy.allow_fetch {
            return Ok(AcquisitionReceipt::RejectedByPolicy);
        }
        // Fetch the branch and update the remote-tracking ref so the result is
        // observable afterwards through a stable ref name.
        let refspec = format!("refs/heads/{name}:refs/remotes/{remote}/{name}");
        if let Some(reason) = git_fetch(&self.repository, remote, &[refspec.as_str()]) {
            return Ok(AcquisitionReceipt::Unavailable {
                reason: Arc::from(reason),
            });
        }
        let target = resolve_ref(&self.repository, &format!("refs/remotes/{remote}/{name}"))?
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
        if let Some(target) = resolve_ref(&self.repository, &tag_ref)? {
            return Ok(AcquisitionReceipt::AlreadyPresent { target });
        }
        if !policy.allow_tag_fetch {
            return Ok(AcquisitionReceipt::RejectedByPolicy);
        }
        let refspec = format!("{tag_ref}:{tag_ref}");
        if let Some(reason) = git_fetch(&self.repository, remote, &[refspec.as_str()]) {
            return Ok(AcquisitionReceipt::Unavailable {
                reason: Arc::from(reason),
            });
        }
        let target =
            resolve_ref(&self.repository, &tag_ref)?.context("fetched tag did not resolve")?;
        Ok(AcquisitionReceipt::FetchedTag {
            name: Arc::from(name),
            target,
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
        let flag = format!("--deepen={depth}");
        if let Some(reason) = git_fetch(&self.repository, remote, &[flag.as_str()]) {
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
        if let Some(reason) = git_fetch(&self.repository, remote, &["--unshallow"]) {
            return Ok(AcquisitionReceipt::Unavailable {
                reason: Arc::from(reason),
            });
        }
        Ok(AcquisitionReceipt::Unshallowed)
    }
}

/// Resolve a ref name to its commit OID, or `None` when it does not resolve.
fn resolve_ref(repository: &Repository, name: &str) -> Result<Option<ObjectId>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{name}^{{commit}}"),
        ])
        .output()
        .with_context(|| format!("resolve ref {name:?}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(ObjectId(Arc::from(
        String::from_utf8(output.stdout)?.trim().to_string(),
    ))))
}

/// Run `git fetch <remote> <args...>`. Returns `None` on success and the
/// captured stderr on failure, used as the `Unavailable` receipt reason.
fn git_fetch(repository: &Repository, remote: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .arg("fetch")
        .arg(remote)
        .args(args)
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
