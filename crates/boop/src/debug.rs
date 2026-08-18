//! What just went wrong, read back off the trail: the WARN/ERROR tail of every
//! `~/.agent/lanes/<lane>/supervise.log` plus the store's `kind=error` events.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use crate::trail;

/// The window `boop debug` and the `--help` banner read by default.
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(120);

/// How much of one lane's log is read. A supervisor writes a few hundred bytes
/// per event, so the newest minutes are always inside this tail.
const TAIL_BYTES: u64 = 64 * 1024;

/// One thing that went wrong, from a lane's log or from the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub lane: String,
    pub at_ms: u64,
    pub level: String,
    pub text: String,
}

/// `30s`, `2m`, `1h`, or a bare count of seconds.
pub fn parse_window(text: &str) -> Result<Duration> {
    let text = text.trim();
    let (count, scale) = match text.chars().last() {
        Some('s') => (&text[..text.len() - 1], 1),
        Some('m') => (&text[..text.len() - 1], 60),
        Some('h') => (&text[..text.len() - 1], 3600),
        _ => (text, 1),
    };
    let count: u64 = count
        .parse()
        .map_err(|_| anyhow::anyhow!("--since `{text}` is not a count of seconds, or Ns/Nm/Nh"))?;
    Ok(Duration::from_secs(count * scale))
}

/// The last `TAIL_BYTES` of a file, whole lines only.
fn tail(path: &Path, limit: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let cut = size.saturating_sub(limit);
    file.seek(SeekFrom::Start(cut)).ok()?;
    let mut bytes = Vec::with_capacity(limit.min(size) as usize);
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    match cut > 0 {
        // The seek lands mid-line; that first fragment is not a record.
        true => text.split_once('\n').map(|(_, rest)| rest.to_owned()),
        false => Some(text),
    }
}

/// One tracing line: `<rfc3339>  WARN <span>: <target>: <message>`. Anything
/// below WARN, and anything without a timestamp, is not an alert.
fn parse_line(line: &str) -> Option<(u64, String, String)> {
    let (stamp, rest) = line.split_once(' ')?;
    let at_ms = crate::harness::claude::parse_iso_ms(stamp)?;
    let rest = rest.trim_start();
    let level = ["ERROR", "WARN"]
        .into_iter()
        .find(|level| rest.starts_with(level))?;
    let body = rest[level.len()..].trim_start();
    // The span block repeats the lane and the whole spawn; the message is after.
    let body = match body.split_once("}: ") {
        Some((_, after)) => after,
        None => body,
    };
    Some((at_ms, level.to_owned(), body.trim().to_owned()))
}

/// Every WARN/ERROR at or after `since_ms` across the lane trails under `root`.
pub fn trail_alerts(root: &Path, since_ms: u64, lane: Option<&str>) -> Vec<Alert> {
    let mut alerts = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return alerts;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if lane.is_some_and(|wanted| wanted != name) {
            continue;
        }
        let Some(text) = tail(&entry.path().join(trail::SUPERVISE_LOG), TAIL_BYTES) else {
            continue;
        };
        for line in text.lines() {
            let Some((at_ms, level, body)) = parse_line(line) else {
                continue;
            };
            if at_ms >= since_ms {
                alerts.push(Alert {
                    lane: name.clone(),
                    at_ms,
                    level,
                    text: body,
                });
            }
        }
    }
    sorted(alerts)
}

/// The store's own `kind=error` events in the same window.
pub fn store_alerts(store: &crate::Store, since_ms: u64, lane: Option<&str>) -> Result<Vec<Alert>> {
    Ok(sorted(
        store
            .error_events_since(since_ms, lane)?
            .into_iter()
            .map(|row| Alert {
                lane: row.lane,
                at_ms: row.created_ts,
                level: "ERROR".to_owned(),
                text: format!("trace event: {}", row.detail),
            })
            .collect(),
    ))
}

/// Lane first, time second. The sort is stable, so two lines sharing a
/// millisecond keep the order the file wrote them in.
fn sorted(mut alerts: Vec<Alert>) -> Vec<Alert> {
    alerts.sort_by(|left, right| {
        left.lane
            .cmp(&right.lane)
            .then(left.at_ms.cmp(&right.at_ms))
    });
    alerts
}

/// `!! N warn/error lines in the last 2m across M lanes: run boop debug`, or
/// `None` on a clean window.
pub fn banner(alerts: &[Alert], window: Duration) -> Option<String> {
    if alerts.is_empty() {
        return None;
    }
    let mut lanes: Vec<&str> = alerts.iter().map(|alert| alert.lane.as_str()).collect();
    lanes.sort_unstable();
    lanes.dedup();
    Some(format!(
        "!! {} warn/error lines in the last {} across {} lanes: run boop debug",
        alerts.len(),
        window_word(window),
        lanes.len(),
    ))
}

/// The window as the caller would have typed it.
fn window_word(window: Duration) -> String {
    let seconds = window.as_secs();
    match seconds {
        _ if seconds.is_multiple_of(3600) => format!("{}h", seconds / 3600),
        _ if seconds.is_multiple_of(60) => format!("{}m", seconds / 60),
        _ => format!("{seconds}s"),
    }
}

/// The banner for `boop --help`. Trail files only: no store open, no lock wait.
pub fn help_banner(now_ms: u64) -> Option<String> {
    let root = trail::lanes_root().ok()?;
    let since = now_ms.saturating_sub(DEFAULT_WINDOW.as_millis() as u64);
    banner(&trail_alerts(&root, since, None), DEFAULT_WINDOW)
}

/// The report, grouped by lane, newest last inside each group.
pub fn report(alerts: &[Alert], window: Duration) -> String {
    if alerts.is_empty() {
        return format!("no warn/error in the last {}", window_word(window));
    }
    let mut out = String::new();
    let mut lane = "";
    for alert in alerts {
        if alert.lane != lane {
            lane = &alert.lane;
            out.push_str(&format!("\n{lane}\n"));
        }
        out.push_str(&format!(
            "  {} {:<5} {}\n",
            clock(alert.at_ms),
            alert.level,
            alert.text
        ));
    }
    out.trim_matches('\n').to_owned()
}

/// One alert as a JSON row.
pub fn as_json(alerts: &[Alert]) -> serde_json::Value {
    serde_json::Value::Array(
        alerts
            .iter()
            .map(|alert| {
                serde_json::json!({
                    "lane": alert.lane,
                    "at_ms": alert.at_ms,
                    "level": alert.level,
                    "text": alert.text,
                })
            })
            .collect(),
    )
}

/// `HH:MM:SS` UTC. The date is the window's, so it never varies inside a report.
fn clock(at_ms: u64) -> String {
    let seconds = (at_ms / 1000) % 86_400;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tempdir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "boop-debug-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_log(root: &Path, lane: &str, lines: &[&str]) {
        let mut file = trail::open_in(root, lane, trail::SUPERVISE_LOG).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    const INSIDE: &str = "2026-08-17T16:34:24.643561Z";
    const OUTSIDE: &str = "2026-08-17T16:20:00.000000Z";

    /// 2026-08-17T16:35:00Z, so INSIDE is 36s back and OUTSIDE is 15m back.
    fn now() -> u64 {
        crate::harness::claude::parse_iso_ms("2026-08-17T16:35:00.000000Z").unwrap()
    }

    fn since(window: Duration) -> u64 {
        now() - window.as_millis() as u64
    }

    #[test]
    fn only_warn_and_error_inside_the_window_are_taken() {
        let root = tempdir("window");
        write_log(
            &root,
            "lane-one",
            &[
                &format!("{OUTSIDE}  WARN lane.supervise{{lane=\"lane-one\"}}: boop::supervise: old flake"),
                &format!("{INSIDE}  INFO lane.supervise{{lane=\"lane-one\"}}: boop::supervise: a healthy turn"),
                &format!("{INSIDE}  WARN lane.supervise{{lane=\"lane-one\"}}: boop::supervise: lane provider flake; resuming"),
                &format!("{INSIDE} ERROR lane.supervise{{lane=\"lane-one\"}}: boop::supervise: lane result row write failed"),
            ],
        );
        write_log(
            &root,
            "lane-two",
            &[&format!(
                "{INSIDE}  WARN boop::channel::tui: aborted stream"
            )],
        );
        let alerts = trail_alerts(&root, since(DEFAULT_WINDOW), None);
        assert_eq!(
            alerts
                .iter()
                .map(|alert| (
                    alert.lane.as_str(),
                    alert.level.as_str(),
                    alert.text.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "lane-one",
                    "WARN",
                    "boop::supervise: lane provider flake; resuming"
                ),
                (
                    "lane-one",
                    "ERROR",
                    "boop::supervise: lane result row write failed"
                ),
                ("lane-two", "WARN", "boop::channel::tui: aborted stream"),
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_lane_filter_reads_one_lane() {
        let root = tempdir("filter");
        write_log(&root, "lane-one", &[&format!("{INSIDE}  WARN a: first")]);
        write_log(&root, "lane-two", &[&format!("{INSIDE}  WARN a: second")]);
        let alerts = trail_alerts(&root, since(DEFAULT_WINDOW), Some("lane-two"));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].lane, "lane-two");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A lane that has run for days must not be read whole on every `--help`.
    #[test]
    fn only_the_tail_of_a_long_log_is_read() {
        let root = tempdir("tail");
        let filler = format!("{INSIDE}  WARN boop::supervise: {}", "x".repeat(400));
        let mut lines: Vec<String> = vec![format!("{INSIDE}  WARN boop::supervise: the head line")];
        lines.extend(std::iter::repeat_n(filler, 300));
        lines.push(format!("{INSIDE}  WARN boop::supervise: the tail line"));
        write_log(
            &root,
            "lane-one",
            &lines.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        let alerts = trail_alerts(&root, since(DEFAULT_WINDOW), None);
        assert!(
            !alerts
                .iter()
                .any(|alert| alert.text.ends_with("the head line")),
            "the head of a 120 KB log is past the tail cut"
        );
        assert!(alerts
            .iter()
            .any(|alert| alert.text.ends_with("the tail line")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_clean_window_has_no_banner_and_says_so() {
        let root = tempdir("clean");
        write_log(&root, "lane-one", &[&format!("{OUTSIDE}  WARN a: old")]);
        let alerts = trail_alerts(&root, since(DEFAULT_WINDOW), None);
        assert!(alerts.is_empty());
        assert_eq!(banner(&alerts, DEFAULT_WINDOW), None);
        assert_eq!(
            report(&alerts, DEFAULT_WINDOW),
            "no warn/error in the last 2m"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_banner_counts_lines_and_lanes() {
        let root = tempdir("banner");
        write_log(
            &root,
            "lane-one",
            &[
                &format!("{INSIDE}  WARN a: first"),
                &format!("{INSIDE} ERROR a: second"),
            ],
        );
        write_log(&root, "lane-two", &[&format!("{INSIDE}  WARN a: third")]);
        let alerts = trail_alerts(&root, since(DEFAULT_WINDOW), None);
        assert_eq!(
            banner(&alerts, DEFAULT_WINDOW).unwrap(),
            "!! 3 warn/error lines in the last 2m across 2 lanes: run boop debug"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_report_groups_by_lane() {
        let root = tempdir("report");
        write_log(
            &root,
            "lane-one",
            &[
                &format!("{INSIDE}  WARN boop::supervise: first"),
                &format!("{INSIDE} ERROR boop::supervise: second"),
            ],
        );
        let alerts = trail_alerts(&root, since(DEFAULT_WINDOW), None);
        assert_eq!(
            report(&alerts, DEFAULT_WINDOW),
            "lane-one\n  16:34:24 WARN  boop::supervise: first\n  16:34:24 ERROR boop::supervise: second"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_window_word_parses_and_renders() {
        assert_eq!(parse_window("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_window("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_window("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_window("45").unwrap(), Duration::from_secs(45));
        assert!(parse_window("soon").is_err());
        assert_eq!(window_word(Duration::from_secs(120)), "2m");
        assert_eq!(window_word(Duration::from_secs(90)), "90s");
    }

    #[test]
    fn a_line_without_a_timestamp_is_not_an_alert() {
        assert_eq!(parse_line("  WARN boop::supervise: no stamp"), None);
        assert_eq!(parse_line(""), None);
        assert_eq!(
            parse_line(&format!("{INSIDE}  INFO boop::supervise: healthy")),
            None
        );
    }
}
