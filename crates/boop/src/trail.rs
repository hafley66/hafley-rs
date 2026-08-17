//! The per-lane on-disk trail at `~/.agent/lanes/<lane>`. A lane pane can be
//! killed at any moment and its scrollback goes with it, so every supervisor
//! event and every harness child's stderr is written here as well.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;

use anyhow::{Context, Result};
use tracing_subscriber::fmt::writer::{BoxMakeWriter, MakeWriterExt};

/// The supervisor's own tracing output.
pub const SUPERVISE_LOG: &str = "supervise.log";
/// Whatever the harness child wrote to fd 2.
pub const CHILD_STDERR: &str = "child.stderr";

/// `~/.agent/lanes`.
pub fn lanes_root() -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolve home directory")?;
    Ok(home.join(".agent").join("lanes"))
}

/// A lane name is a path segment here; a separator in it would escape the root.
fn segment(lane: &str) -> String {
    lane.replace(['/', '\\'], "_")
}

/// The trail directory for one lane under `root`.
pub fn lane_dir_in(root: &Path, lane: &str) -> PathBuf {
    root.join(segment(lane))
}

/// The trail directory for one lane under `~/.agent/lanes`.
pub fn lane_dir(lane: &str) -> Result<PathBuf> {
    Ok(lane_dir_in(&lanes_root()?, lane))
}

/// Open `<root>/<lane>/<name>` for append, creating the directory.
pub fn open_in(root: &Path, lane: &str, name: &str) -> std::io::Result<File> {
    let dir = lane_dir_in(root, lane);
    std::fs::create_dir_all(&dir)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(name))
}

/// Open one of this lane's trail files under `~/.agent/lanes`, or `None` when
/// the home directory or the file itself is unavailable. A trail that cannot
/// open never blocks the lane; the pane's stderr is still written.
pub fn open(lane: &str, name: &str) -> Option<File> {
    let root = lanes_root().ok()?;
    open_in(&root, lane, name).ok()
}

/// The stderr sink for a harness child. `Stdio::inherit` is the fallback: a
/// lane that cannot open its trail still shows the child in the pane.
pub fn child_stderr_in(root: &Path, lane: &str) -> Stdio {
    match open_in(root, lane, CHILD_STDERR) {
        Ok(file) => Stdio::from(file),
        Err(_) => Stdio::inherit(),
    }
}

/// As `child_stderr_in`, rooted at `~/.agent/lanes`. `None` for a lane name is
/// a caller with no lane (concatmap, an embedding host): it inherits.
pub fn child_stderr(lane: Option<&str>) -> Stdio {
    match lane.and_then(|lane| open(lane, CHILD_STDERR)) {
        Some(file) => Stdio::from(file),
        None => Stdio::inherit(),
    }
}

/// The writer the CLI's fmt subscriber uses. With a lane log every event is
/// written twice: once to the pane, once to the file that outlives it.
///
/// A subscriber builder lives here rather than in the binary so the tee has a
/// test that installs it; the library still installs nothing on its own.
pub fn lane_writer(lane_log: Option<File>) -> BoxMakeWriter {
    match lane_log {
        // `File` is unbuffered, so one event is one write syscall and a SIGKILL
        // between events loses nothing.
        Some(file) => BoxMakeWriter::new(std::io::stderr.and(Mutex::new(file))),
        None => BoxMakeWriter::new(std::io::stderr),
    }
}

/// Why a lane whose tmux session is gone is gone. Every variant renders to a
/// token, so a dead row is never blank.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeadReason {
    /// The supervisor reached an exit path and wrote its result row.
    Reported { rc: i32, detail: Option<String> },
    /// A trail exists and no result row does: the supervisor died before any
    /// exit path ran, and the trail files are what is left to read.
    DiedBeforeResult,
    /// Neither a result row nor a trail directory. Nothing to diagnose from.
    NoTrail,
}

impl DeadReason {
    /// The token `lane list` prints after the row.
    pub fn token(&self) -> String {
        match self {
            DeadReason::Reported {
                rc,
                detail: Some(detail),
            } => format!("rc={rc} ({detail})"),
            DeadReason::Reported { rc, detail: None } => format!("rc={rc}"),
            DeadReason::DiedBeforeResult => "died-before-result".to_owned(),
            DeadReason::NoTrail => "no-trail".to_owned(),
        }
    }
}

/// Split `lane mine done rc=1 (supervisor error: x)` into its code and reason.
pub fn parse_result_body(body: &str) -> Option<(i32, Option<String>)> {
    let rc = body
        .split_whitespace()
        .find_map(|token| token.strip_prefix("rc=")?.parse::<i32>().ok())?;
    let detail = body
        .split_once('(')
        .and_then(|(_, rest)| rest.rsplit_once(')'))
        .map(|(inner, _)| inner.to_owned());
    Some((rc, detail))
}

/// The typed reason for a dead lane: its newest result row if one exists, else
/// what the trail directory says about how far the supervisor got.
pub fn dead_reason(mail_dir: &Path, lanes_root: &Path, lane: &str) -> DeadReason {
    let mut rows = Vec::new();
    for path in crate::bus::read_boxes(mail_dir).unwrap_or_default() {
        rows.extend(crate::bus::parse_box(&path));
    }
    let reported = rows
        .iter()
        .rev()
        .find(|row| row.kind == "result" && row.from == lane)
        .and_then(|row| parse_result_body(&row.body));
    match reported {
        Some((rc, detail)) => DeadReason::Reported { rc, detail },
        None if lane_dir_in(lanes_root, lane).exists() => DeadReason::DiedBeforeResult,
        None => DeadReason::NoTrail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "boop-trail-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // FAIL-PRE-FIX: the supervisor logged only to the pane's stderr, so a lane
    // whose pane was gone left nothing to read. SABOTAGE RECEIPT: replace the
    // `Some(file)` arm of `lane_writer` with the stderr-only arm and this test
    // fails with an empty supervise.log.
    #[test]
    fn the_lane_writer_tees_every_event_into_supervise_log() {
        let root = tempdir("tee");
        let file = open_in(&root, "mine", SUPERVISE_LOG).unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(lane_writer(Some(file)))
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(marker = "trail-receipt", "lane trail armed");
        });
        let text = std::fs::read_to_string(lane_dir_in(&root, "mine").join(SUPERVISE_LOG)).unwrap();
        assert!(text.contains("trail-receipt"), "{text}");
        assert!(text.contains("lane trail armed"), "{text}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A second supervisor run appends; the first run's trail is never lost.
    #[test]
    fn a_second_open_appends_instead_of_truncating() {
        let root = tempdir("append");
        writeln!(open_in(&root, "mine", SUPERVISE_LOG).unwrap(), "first").unwrap();
        writeln!(open_in(&root, "mine", SUPERVISE_LOG).unwrap(), "second").unwrap();
        let text = std::fs::read_to_string(lane_dir_in(&root, "mine").join(SUPERVISE_LOG)).unwrap();
        assert_eq!(text, "first\nsecond\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    // FAIL-PRE-FIX: the codex/claude/kimi children were spawned with
    // `Stdio::inherit()`, so `write rpc turn/start` broke against a peer whose
    // own complaint went only to the dead pane. SABOTAGE RECEIPT: return
    // `Stdio::inherit()` unconditionally from `child_stderr_in` and this test
    // fails with a missing child.stderr.
    #[test]
    fn a_child_s_stderr_lands_in_the_lane_trail() {
        let root = tempdir("childerr");
        let status = std::process::Command::new("sh")
            .args(["-c", "echo broken pipe into the peer >&2"])
            .stderr(child_stderr_in(&root, "mine"))
            .status()
            .unwrap();
        assert!(status.success());
        let text = std::fs::read_to_string(lane_dir_in(&root, "mine").join(CHILD_STDERR)).unwrap();
        assert_eq!(text, "broken pipe into the peer\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A lane name is one path segment; a separator must not escape the root.
    #[test]
    fn a_lane_name_with_a_separator_stays_under_the_root() {
        let root = tempdir("segment");
        assert_eq!(
            lane_dir_in(&root, "feature/x"),
            root.join("feature_x"),
            "a slash would otherwise make a nested directory"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_result_body_splits_into_a_code_and_a_reason() {
        assert_eq!(
            parse_result_body("lane mine done rc=1 (supervisor error: write rpc turn/start)"),
            Some((1, Some("supervisor error: write rpc turn/start".to_owned())))
        );
        assert_eq!(parse_result_body("lane mine done rc=0"), Some((0, None)));
        assert_eq!(parse_result_body("lane mine still running"), None);
    }

    // FAIL-PRE-FIX: `lane list` printed `dead` with nothing after it, so a lane
    // that vanished and a lane that reported rc=0 looked the same.
    // SABOTAGE RECEIPT: make `dead_reason` return `DeadReason::NoTrail`
    // unconditionally and the first two assertions fail.
    #[test]
    fn a_dead_lane_always_carries_a_typed_reason() {
        let mail = tempdir("deadmail");
        let root = tempdir("deadtrail");
        let row = crate::bus::Message {
            id: "m1".into(),
            from: "reported".into(),
            to: "coordinator".into(),
            from_timestamp: "2026-08-17T00:00:00.000Z".into(),
            to_timestamp: None,
            kind: "result".into(),
            reply_to: None,
            body: "lane reported done rc=1 (supervisor error: write rpc turn/start)".into(),
            r#ref: None,
        };
        writeln!(
            std::fs::File::create(mail.join("bus.ndjson")).unwrap(),
            "{}",
            crate::bus::message_line(&row)
        )
        .unwrap();
        assert_eq!(
            dead_reason(&mail, &root, "reported").token(),
            "rc=1 (supervisor error: write rpc turn/start)"
        );
        std::fs::create_dir_all(lane_dir_in(&root, "trailed")).unwrap();
        assert_eq!(
            dead_reason(&mail, &root, "trailed"),
            DeadReason::DiedBeforeResult
        );
        assert_eq!(dead_reason(&mail, &root, "ghost"), DeadReason::NoTrail);
        assert_eq!(dead_reason(&mail, &root, "ghost").token(), "no-trail");
        let _ = std::fs::remove_dir_all(&mail);
        let _ = std::fs::remove_dir_all(&root);
    }
}
