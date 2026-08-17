//! One job: tell the crate which commit it was built from, so `boop --version`
//! names a binary and a lane death can be tied to one.
//!
//! `BOOP_BUILD_SHA` from the environment wins. The install recipe reads the sha
//! with git and passes it in, which is the only spelling that cannot go stale
//! behind a cached build script. Absent that the script asks git itself, and a
//! build outside a checkout is stamped `unknown` rather than failed.

use std::process::Command;

fn main() {
    println!("cargo::rerun-if-env-changed=BOOP_BUILD_SHA");
    // A worktree keeps its git dir elsewhere, so the watched paths come from
    // git rather than from a guess at `../../.git`.
    for file in ["HEAD", "index"] {
        if let Some(path) = git(&["rev-parse", "--git-path", file]) {
            println!("cargo::rerun-if-changed={path}");
        }
    }
    let stamp = std::env::var("BOOP_BUILD_SHA")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(head_stamp);
    println!("cargo::rustc-env=BOOP_BUILD_SHA={stamp}");
}

/// The short HEAD sha, with `-dirty` appended when tracked files differ from
/// it. Untracked files are not a difference from the commit's content.
fn head_stamp() -> String {
    let Some(sha) = git(&["rev-parse", "--short", "HEAD"]) else {
        return "unknown".to_owned();
    };
    match git(&["status", "--porcelain", "--untracked-files=no"]) {
        Some(changes) if !changes.is_empty() => format!("{sha}-dirty"),
        _ => sha,
    }
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
