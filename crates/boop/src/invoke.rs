//! Best-effort start and finish rows per Boop CLI invocation in the trace store.
//!
//! Storage is the already-shipped `agent_trace_event` table reached through
//! [`boop_store::Store::record_trace_event`]. No new table, crate or framework.
//! A failed store open is swallowed: analytics never changes CLI behaviour.
//!
//! Redaction is structural, not textual. The detail carries the normalized
//! command path, option *names* and counts, and the outcome. Positional bodies,
//! flag values, paths and tokens are never read, so there is nothing to scrub.

use std::path::Path;

use boop_store::{Store, TraceEvent};
use clap::parser::ValueSource;
use clap::{ArgAction, ArgMatches, Command};

/// The single trace lane CLI invocations attach to when no real route exists.
const LANE: &str = "boop-cli";
/// The trace `kind` this module writes.
pub const KIND: &str = "cli-invocation";
/// Analytics is a side observation, never a reason for a command to wait on a
/// contention-heavy writer. The bounded write gives up after this long.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// One invocation as observed at the entrypoint, before persistence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub id: String,
    pub command_path: String,
    pub options: Vec<(String, usize)>,
    pub started_ms: u64,
    pub lane: Option<String>,
    pub harness: Option<String>,
}

/// The normalized subcommand path from clap's argv walk, e.g. `beep lane send`.
pub fn command_path(matches: &ArgMatches) -> String {
    let mut path = Vec::new();
    let mut current = matches;
    while let Some((name, sub)) = current.subcommand() {
        path.push(name.to_owned());
        current = sub;
    }
    if path.is_empty() {
        "none".to_owned()
    } else {
        path.join(" ")
    }
}

/// Parse failures do not produce a trusted command path. Raw argv is opaque
/// here because a positional token can be a body, path, or credential.
pub fn command_path_from_argv(_argv: &[String]) -> String {
    "parse-error".to_owned()
}

fn option_names_in_command(
    command: &Command,
    matches: &ArgMatches,
    counts: &mut Vec<(String, usize)>,
) {
    for arg in command.get_arguments() {
        let Some(canonical) = arg
            .get_long()
            .map(|name| format!("--{name}"))
            .or_else(|| arg.get_short().map(|name| format!("-{name}")))
        else {
            continue;
        };
        let id = arg.get_id().as_str();
        // A schema default is not a user choice: only a value the command line
        // actually supplied counts as an option the caller passed.
        let count = if matches.value_source(id) == Some(ValueSource::CommandLine) {
            match arg.get_action() {
                ArgAction::Count => matches.get_count(id) as usize,
                ArgAction::SetTrue | ArgAction::SetFalse => {
                    matches.indices_of(id).map_or(0, Iterator::count)
                }
                ArgAction::Set | ArgAction::Append => {
                    matches.get_raw(id).map_or(0, Iterator::count)
                }
                _ => 0,
            }
        } else {
            0
        };
        if count > 0 {
            counts.push((canonical, count));
        }
    }
    if let Some((name, submatches)) = matches.subcommand() {
        if let Some(subcommand) = command.find_subcommand(name) {
            option_names_in_command(subcommand, submatches, counts);
        }
    }
}

/// Option names with occurrence counts, restricted to options declared by the
/// active Clap schema and present in parsed matches. Values are never read.
pub fn option_names(command: &Command, matches: &ArgMatches) -> Vec<(String, usize)> {
    let mut counts = Vec::new();
    option_names_in_command(command, matches, &mut counts);
    counts.sort();
    counts
}

/// Harness and route identity when the terminal subcommand carries them.
pub fn identity(matches: &ArgMatches) -> (Option<String>, Option<String>) {
    let mut current = matches;
    let mut harness = None;
    let mut lane = None;
    loop {
        if let Ok(value) = current.try_get_one::<String>("harness") {
            harness = value.cloned().or(harness);
        }
        if let Ok(value) = current.try_get_one::<String>("lane") {
            lane = value.cloned().or(lane);
        }
        let Some((_, sub)) = current.subcommand() else {
            break;
        };
        current = sub;
    }
    (harness, lane)
}

/// Parse-error outcome classification. Help and version are clap `ErrorKind`s
/// on the same path, so they are observed without a second parse.
pub fn outcome_for_clap_error(error: &clap::Error) -> &'static str {
    use clap::error::ErrorKind;
    match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => "help",
        ErrorKind::DisplayVersion => "version",
        _ => "parse-error",
    }
}

/// Build the invocation record from the command schema and parsed matches.
pub fn begin(
    command_path: String,
    command: &Command,
    matches: Option<&ArgMatches>,
    started_ms: u64,
) -> Invocation {
    Invocation {
        id: format!("cli-{started_ms}-{}", std::process::id()),
        command_path,
        options: matches
            .map(|matches| option_names(command, matches))
            .unwrap_or_default(),
        started_ms,
        lane: None,
        harness: None,
    }
}

/// The persisted detail: normalized facts only.
fn detail_json(invocation: &Invocation, outcome: &str, elapsed_ms: u64) -> String {
    let options: Vec<String> = invocation
        .options
        .iter()
        .map(|(name, count)| {
            if *count > 1 {
                format!("{name}x{count}")
            } else {
                name.clone()
            }
        })
        .collect();
    serde_json::json!({
        "command": invocation.command_path,
        "outcome": outcome,
        "options": options,
        "harness": invocation.harness,
        "lane": invocation.lane,
        "duration_ms": elapsed_ms,
    })
    .to_string()
}

fn persist_with(
    path: &Path,
    invocation: &Invocation,
    elapsed_ms: u64,
    outcome: &str,
    phase: &str,
) -> bool {
    let Ok(store) = Store::open_bounded_write(path.to_path_buf(), BUSY_TIMEOUT) else {
        return false;
    };
    let event = TraceEvent {
        event_key: format!("{KIND}/{}/{}", invocation.id, phase),
        lane: invocation.lane.clone().unwrap_or_else(|| LANE.to_owned()),
        trace: None,
        session: None,
        kind: KIND.to_owned(),
        from_lane: None,
        to_lane: None,
        started_ts: Some(invocation.started_ms),
        finished_ts: (phase == "finish").then(|| invocation.started_ms.saturating_add(elapsed_ms)),
        delivery_state: Some(outcome.to_owned()),
        classification: None,
        detail: detail_json(invocation, outcome, elapsed_ms),
        created_ts: invocation.started_ms.saturating_add(elapsed_ms),
    };
    store.record_trace_event(&event).is_ok()
}

/// Persist the observable start before running the CLI. A missing finish row
/// can mean an active/interrupted process or an unavailable final append.
pub fn start_with(path: &Path, invocation: &Invocation) -> bool {
    persist_with(path, invocation, 0, "started", "start")
}

/// Persist the terminal update when control returns from the CLI.
pub fn finish_with(path: &Path, invocation: &Invocation, elapsed_ms: u64, outcome: &str) -> bool {
    persist_with(path, invocation, elapsed_ms, outcome, "finish")
}

/// Persist to the default store. Returns false when unavailable.
pub fn finish(invocation: &Invocation, elapsed_ms: u64, outcome: &str) -> bool {
    let Ok(path) = Store::default_path() else {
        return false;
    };
    finish_with(&path, invocation, elapsed_ms, outcome)
}

pub fn start(invocation: &Invocation) -> bool {
    let Ok(path) = Store::default_path() else {
        return false;
    };
    start_with(&path, invocation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Cli;
    use clap::CommandFactory;
    use std::path::PathBuf;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("boop-invoke-{}-{tag}.db", std::process::id()))
    }

    #[test]
    fn identity_reads_common_command_matches_without_panicking() {
        let cases = [
            (vec!["boop", "tui", "codex"], Some("codex"), None),
            (
                vec![
                    "boop",
                    "beep",
                    "lane",
                    "create",
                    "--branch",
                    "feature-x",
                    "--harness",
                    "codex",
                    "--lane",
                    "lane-probe",
                ],
                Some("codex"),
                Some("lane-probe"),
            ),
            (
                vec![
                    "boop",
                    "beep",
                    "agent",
                    "register",
                    "agent-a",
                    "--harness",
                    "codex",
                ],
                Some("codex"),
                None,
            ),
            (vec!["boop", "db", "--format", "text"], None, None),
        ];
        for (argv, expected_harness, expected_lane) in cases {
            let matches = Cli::command()
                .try_get_matches_from(argv)
                .expect("common command parses");
            assert_eq!(
                identity(&matches),
                (
                    expected_harness.map(str::to_owned),
                    expected_lane.map(str::to_owned)
                )
            );
        }
    }

    #[test]
    fn option_names_count_and_drop_values() {
        let argv = vec![
            "boop".to_owned(),
            "beep".to_owned(),
            "--as".to_owned(),
            "me".to_owned(),
            "--as".to_owned(),
            "other".to_owned(),
            "--token=secret123".to_owned(),
        ];
        let schema = Command::new("boop").subcommand(
            Command::new("beep")
                .arg(
                    clap::Arg::new("as")
                        .long("as")
                        .num_args(1)
                        .action(ArgAction::Append),
                )
                .arg(clap::Arg::new("token").long("token").num_args(1)),
        );
        let matches = schema
            .clone()
            .try_get_matches_from(argv)
            .expect("schema test argv parses");
        assert_eq!(
            option_names(&schema, &matches),
            vec![("--as".to_owned(), 2), ("--token".to_owned(), 1)]
        );
    }

    #[test]
    fn option_names_accept_only_schema_options_and_never_values() {
        let schema = Command::new("boop")
            .arg(
                clap::Arg::new("name")
                    .short('n')
                    .long("name")
                    .num_args(1)
                    .action(ArgAction::Append),
            )
            .arg(
                clap::Arg::new("verbose")
                    .short('v')
                    .action(ArgAction::Count),
            );
        let argv = vec![
            "boop".to_owned(),
            "--name=--looks-like-an-option".to_owned(),
            "-nsecret-value".to_owned(),
            "-vv".to_owned(),
        ];
        let matches = schema
            .clone()
            .try_get_matches_from(argv)
            .expect("schema test argv parses");
        assert_eq!(
            option_names(&schema, &matches),
            vec![("--name".to_owned(), 2), ("-v".to_owned(), 2)]
        );
    }

    #[test]
    fn option_names_exclude_schema_defaults() {
        let schema = Command::new("boop")
            .arg(
                clap::Arg::new("kind")
                    .long("kind")
                    .num_args(1)
                    .default_value("request"),
            )
            .arg(
                clap::Arg::new("limit")
                    .long("limit")
                    .num_args(1)
                    .default_value("5"),
            )
            .arg(
                clap::Arg::new("verbose")
                    .long("verbose")
                    .action(ArgAction::SetTrue),
            );
        let defaults = schema
            .clone()
            .try_get_matches_from(["boop"])
            .expect("defaults-only argv parses");
        assert_eq!(
            option_names(&schema, &defaults),
            Vec::<(String, usize)>::new()
        );
        let supplied = schema
            .clone()
            .try_get_matches_from(["boop", "--kind", "hail", "--limit", "9", "--verbose"])
            .expect("supplied argv parses");
        assert_eq!(
            option_names(&schema, &supplied),
            vec![
                ("--kind".to_owned(), 1),
                ("--limit".to_owned(), 1),
                ("--verbose".to_owned(), 1),
            ]
        );
    }

    #[test]
    fn parse_errors_use_an_opaque_command_sentinel() {
        let argv = vec![
            "boop".to_owned(),
            "--bad".to_owned(),
            "credential-value".to_owned(),
        ];
        assert_eq!(command_path_from_argv(&argv), "parse-error");
    }

    #[test]
    fn detail_omits_body_and_secret_values() {
        let argv = vec!["boop".to_owned(), "--token=secret123".to_owned()];
        let schema = Command::new("boop").arg(clap::Arg::new("token").long("token").num_args(1));
        let matches = schema
            .clone()
            .try_get_matches_from(argv.clone())
            .expect("schema test argv parses");
        let invocation = begin("beep".to_owned(), &schema, Some(&matches), 1_000);
        let detail = detail_json(&invocation, "ok", 5);
        assert!(!detail.contains("secret123"), "{detail}");
        assert!(!detail.contains("secret123"), "{detail}");
        assert!(!detail.contains("/Users/"), "{detail}");
        assert!(detail.contains("--token"), "{detail}");
        assert!(detail.contains("\"command\":\"beep\""), "{detail}");
    }

    #[test]
    fn record_round_trips_into_the_trace_table() {
        let path = temp_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        Store::open(path.clone()).expect("initialise the analytics store");
        let schema = Command::new("boop");
        let matches = schema.clone().try_get_matches_from(["boop"]).unwrap();
        let invocation = begin("db".to_owned(), &schema, Some(&matches), 2_000);
        assert!(start_with(&path, &invocation));
        assert!(finish_with(&path, &invocation, 12, "ok"));

        let store = Store::open(path.clone()).unwrap();
        let rows = store.query_trace_events(None, 10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, KIND);
        assert_eq!(rows[0].lane, LANE);
        assert_eq!(rows[0].delivery_state.as_deref(), Some("started"));
        assert_eq!(rows[0].started_ts, Some(2_000));
        assert_eq!(rows[0].finished_ts, None);
        assert_eq!(rows[1].delivery_state.as_deref(), Some("ok"));
        assert_eq!(rows[1].started_ts, Some(2_000));
        assert_eq!(rows[1].finished_ts, Some(2_012));
        assert!(rows[1].detail.contains("db"));
        assert!(!rows[1].detail.contains("boop-cli/"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unavailable_store_never_breaks_the_caller() {
        let dir = std::env::temp_dir().join(format!("boop-invoke-dir-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let invocation = begin("db".to_owned(), &Command::new("boop"), None, 3_000);
        assert!(!finish_with(&dir, &invocation, 1, "ok"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
