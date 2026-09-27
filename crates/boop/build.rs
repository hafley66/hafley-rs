//! One job: tell the crate which commit it was built from and when, so
//! `boop --version` and invocation rows name the binary that produced them.
//!
//! Environment stamps win. Otherwise the script asks git itself, and a build
//! outside a checkout is stamped `unknown` rather than failed.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo::rerun-if-env-changed=BOOP_BUILD_SHA");
    println!("cargo::rerun-if-env-changed=BOOP_BUILD_SHA_FULL");
    // A worktree keeps its git dir elsewhere, so the watched paths come from
    // git rather than from a guess at `../../.git`.
    for file in ["HEAD", "index"] {
        if let Some(path) = git(&["rev-parse", "--git-path", file]) {
            println!("cargo::rerun-if-changed={path}");
        }
    }
    let short = env_stamp("BOOP_BUILD_SHA").unwrap_or_else(short_stamp);
    let full = env_stamp("BOOP_BUILD_SHA_FULL").unwrap_or_else(full_stamp);
    println!("cargo::rustc-env=BOOP_BUILD_SHA={short}");
    println!("cargo::rustc-env=BOOP_BUILD_SHA_FULL={full}");
    println!("cargo::rustc-env=BOOP_BUILD_TS={}", rfc3339_now());
}

fn env_stamp(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// The short HEAD sha, with `-dirty` appended when tracked files differ from
/// it. Untracked files are not a difference from the commit's content.
fn short_stamp() -> String {
    let Some(sha) = git(&["rev-parse", "--short", "HEAD"]) else {
        return "unknown".to_owned();
    };
    dirty_suffix(sha)
}

fn full_stamp() -> String {
    let Some(sha) = git(&["rev-parse", "HEAD"]) else {
        return "unknown".to_owned();
    };
    dirty_suffix(sha)
}

fn dirty_suffix(sha: String) -> String {
    match git(&["status", "--porcelain", "--untracked-files=no"]) {
        Some(changes) if !changes.is_empty() => format!("{sha}-dirty"),
        _ => sha,
    }
}

fn rfc3339_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    let (hour, minute, second) = (rem / 3_600, (rem % 3_600) / 60, rem % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
