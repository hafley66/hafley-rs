use serde_json::{json, Value};
use std::process::Command;
use std::sync::Arc;

use sprefa_extract::trace::{SummaryLayer, SummaryState};
use sprefa_extract::{dispatch, FamilyMask};
use tracing_subscriber::{layer::SubscriberExt, Registry};

const BIN: &str = env!("CARGO_BIN_EXE_ryii");
const FIXTURE: &str = "tests/fixtures/rust/sample.rs";

/// The tracing seam: shared warn-by-default telemetry and the summary table.
/// Nine scenarios (info default, json format, in-process summary layer, the
/// `DL_TRACE_SUMMARY` flag, micros ordering, pinned per-file phase counts,
/// phases-that-ran, `--bench`, the sqlite trail) collapse into one output;
/// nondeterministic content (wall micros, pid, timestamps) stays in code
/// asserts, deterministic tables freeze.
pub fn evaluate(_case: &Value) -> Value {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let fixture = format!("{manifest}/{FIXTURE}");
    let run = |args: &[&str], envs: &[(&str, Option<&str>)]| {
        let mut command = Command::new(BIN);
        command.args(args).env_remove("RUST_LOG");
        for (key, value) in envs {
            match value {
                Some(value) => command.env(key, value),
                None => command.env_remove(key),
            };
        }
        let output = command.output().expect("run extract");
        assert!(output.status.success(), "{args:?} failed: {output:?}");
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };

    // The default is sprefa_extract=info,hafley_scm=info, so an ordinary run
    // narrates itself on stderr while the fact stream stays alone on stdout.
    let (stdout, stderr) = run(
        &["--kinds", "call", &fixture],
        &[
            ("DL_TRACE_SUMMARY", None),
            ("HAFLEY_LOG_FORMAT", None),
            ("DL_TRAIL", None),
        ],
    );
    assert!(!stdout.is_empty(), "the fact stream must still reach stdout");
    assert!(
        stderr.contains("INFO") && stderr.contains("extract_file"),
        "the info default must narrate the run on stderr, got {stderr}"
    );
    assert!(
        !stdout.contains("INFO"),
        "telemetry must never leak into the fact stream"
    );
    let info_default = json!({"stdout_nonempty": true, "stderr_has_info_narration": true});

    // The json format emits one event per line; the startup event carries the
    // service identity (pid is nondeterministic, asserted live below).
    let (stdout, stderr) = run(
        &["--kinds", "call", &fixture],
        &[
            ("RUST_LOG", Some("sprefa_extract=debug,hafley_scm=debug,hafley_observe=debug")),
            ("HAFLEY_LOG_FORMAT", Some("json")),
            ("DL_TRACE_SUMMARY", None),
        ],
    );
    let values: Vec<Value> = stderr
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON event per line"))
        .collect();
    let startup = values
        .iter()
        .find(|value| value["fields"]["message"] == "observability initialized")
        .expect("startup event")
        .clone();
    assert_eq!(startup["fields"]["service.name"], "sprefa-extract");
    assert_eq!(startup["fields"]["service.version"], env!("CARGO_PKG_VERSION"));
    assert!(startup["fields"]["process.pid"].as_u64().is_some());
    assert_eq!(startup["fields"]["log.format"], "json");
    let json_format = json!({
        "service": [
            startup["fields"]["service.name"],
            startup["fields"]["service.version"],
            startup["fields"]["log.format"],
        ],
        "event_count": values.len(),
    });

    /// The (lang, family) pairs the rendered table names, header row dropped.
    fn table_rows(table: &str) -> Vec<(String, String)> {
        table
            .lines()
            .skip_while(|line| !line.starts_with("ryi summary: wall "))
            .skip(2)
            .take_while(|line| !line.trim().is_empty())
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                Some((fields.next()?.to_string(), fields.next()?.to_string()))
            })
            .collect()
    }

    /// One (lang, phase) row of the phase table as (files, calls, rows), or
    /// None when that phase was never entered.
    fn phase_row(table: &str, lang: &str, phase: &str) -> Option<(u64, u64, u64)> {
        table
            .lines()
            .skip_while(|line| !line.starts_with("ryi phases: load "))
            .skip(2)
            .take_while(|line| !line.trim().is_empty())
            .find_map(|line| {
                let columns: Vec<&str> = line.split_whitespace().collect();
                (columns.len() == 7 && columns[0] == lang && columns[1] == phase).then(|| {
                    (
                        columns[2].parse().unwrap(),
                        columns[3].parse().unwrap(),
                        columns[4].parse().unwrap(),
                    )
                })
            })
    }

    // In-process: the summary layer renders one row per (lang, family) the
    // dispatch ran, opening with the wall line.
    let state = Arc::new(SummaryState::new());
    let subscriber = Registry::default().with(SummaryLayer::new(Arc::clone(&state)));
    let content = std::fs::read(&fixture).expect("read fixture");
    tracing::subscriber::with_default(subscriber, || {
        dispatch(&fixture, &content, FamilyMask::ALL).expect("rust source matches");
    });
    let table = state.render();
    assert!(table.starts_with("ryi summary: wall "), "table must open with the wall line");
    let mut layer_rows = table_rows(&table);
    layer_rows.sort();
    for family in ["parse", "cst", "type", "call", "df", "extract_file"] {
        assert!(
            layer_rows.iter().any(|(_, f)| f == family),
            "no {family} row in\n{table}"
        );
    }

    // The µs column is descending, so the table names the expensive leg first
    // (the micros themselves are wall-clock, never frozen).
    let state = Arc::new(SummaryState::new());
    let subscriber = Registry::default().with(SummaryLayer::new(Arc::clone(&state)));
    let lib = format!("{manifest}/tests/fixtures/rust/lib.rs");
    let content = std::fs::read(&lib).expect("read fixture");
    tracing::subscriber::with_default(subscriber, || {
        dispatch(&lib, &content, FamilyMask::ALL).expect("rust source matches");
    });
    let sorted_table = state.render();
    let micros: Vec<u128> = sorted_table
        .lines()
        .skip_while(|line| !line.starts_with("ryi summary: wall "))
        .skip(2)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().nth(2)?.parse().ok())
        .collect();
    assert!(micros.len() > 2, "too few rows in\n{sorted_table}");
    assert!(
        micros.windows(2).all(|pair| pair[0] >= pair[1]),
        "rows are not wall descending in\n{sorted_table}"
    );
    let sort_rows = table_rows(&sorted_table);
    let mut sort_rows = sort_rows;
    sort_rows.sort();

    // The flag prints both tables to stderr.
    let (_, flag_stderr) = run(
        &["--kinds", "call", &fixture],
        &[("DL_TRACE_SUMMARY", Some("1")), ("DL_TRAIL", Some("0"))],
    );
    assert!(flag_stderr.contains("ryi summary: wall "), "no summary table on stderr");
    assert!(
        table_rows(&flag_stderr).contains(&("rust".to_string(), "call".to_string())),
        "no rust/call row in {flag_stderr}"
    );

    /// One `--family cst,type,call` run over one file, phase table on stderr
    /// and the trail off, so a fixture run never touches `~/.agent`.
    fn phases_of(path: &str) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .args(["--kinds", "cst,type,call,df", path])
            .env_remove("RUST_LOG")
            .env("DL_TRACE_SUMMARY", "1")
            .env("DL_TRAIL", "0")
            .output()
            .expect("run extract");
        assert!(output.status.success(), "extract failed on {path}");
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    // Per lang: how many full content hashes and how many parses ONE file
    // costs. Every lang hashes ONCE, the extract cache key in `dispatch.rs`;
    // a door that needs the id reads it back. failure-modes 107 owns the
    // count. SABOTAGE RECEIPT: drop the `extracting_blob` read at `go.rs:80`
    // back to a bare `content_id_of`, or the `EXTRACTING` set in
    // `dispatch.rs:63` that feeds it, and `go hash` reads 2 per file against
    // the 1 below; the same at `ts.rs:1698` for ts. A second blake3 is linear
    // in file size and no wall budget on a loaded machine separates it from
    // the machine.
    let hashes_per_file = [("go", 1u64), ("ts", 1), ("rust", 1)];
    let parses_per_file = [("go", 2u64), ("ts", 2), ("rust", 1)];
    let mut pinned_rows = Vec::new();
    let corpus = [
        ("go", ["go/sample.go", "go/docs.go", "go/edges.go"]),
        ("ts", ["ts/sample.ts", "ts/docs.ts", "ts/consts.ts"]),
        ("rust", ["rust/sample.rs", "rust/docs.rs", "rust/lib.rs"]),
    ];
    for (lang, files) in corpus {
        let want_hashes = hashes_per_file
            .iter()
            .find_map(|(name, count)| (*name == lang).then_some(*count))
            .expect("every lang in the corpus is priced");
        let want_parses = parses_per_file
            .iter()
            .find_map(|(name, count)| (*name == lang).then_some(*count))
            .expect("every lang in the corpus has a parse budget");
        for file in files {
            let path = format!("{manifest}/tests/fixtures/{file}");
            let table = phases_of(&path);
            let (hash_files, hash_calls, _) = phase_row(&table, lang, "hash")
                .unwrap_or_else(|| panic!("no {lang} hash row for {file} in\n{table}"));
            assert_eq!(
                (hash_files, hash_calls),
                (want_hashes, want_hashes),
                "{lang} hashed {file} {hash_files} times, want {want_hashes}\n{table}"
            );
            let (parse_files, parse_calls, _) = phase_row(&table, lang, "parse")
                .unwrap_or_else(|| panic!("no {lang} parse row for {file} in\n{table}"));
            assert_eq!(
                (parse_files, parse_calls),
                (want_parses, want_parses),
                "{lang} parsed {file} {parse_files} times\n{table}"
            );
            let mut row = vec![
                json!(file),
                json!(format!("{lang}/hash={hash_calls}")),
                json!(format!("{lang}/parse={parse_calls}")),
            ];
            if lang == "rust" {
                let (query_files, query_calls, _) = phase_row(&table, lang, "query")
                    .unwrap_or_else(|| panic!("no rust query row for {file} in\n{table}"));
                assert_eq!(
                    (query_files, query_calls),
                    (1, 1),
                    "rust queried {file} {query_calls} times\n{table}"
                );
                row.push(json!(format!("{lang}/query={query_calls}")));
            }
            let (flatten_files, _, _) = phase_row(&table, "-", "flatten")
                .unwrap_or_else(|| panic!("no flatten row for {file} in\n{table}"));
            assert_eq!(flatten_files, 1, "{file} flattened {flatten_files} times");
            row.push(json!(format!("-/flatten={flatten_files}")));
            pinned_rows.push(json!(row));
        }
    }
    // The chain phase's site count on `go_residual/callers.go`, hand-counted
    // off the phase table and pinned so a chain walk that doubles is a FAIL.
    let table = phases_of(&format!("{manifest}/tests/fixtures/go_residual/callers.go"));
    let (_, chain_calls, _) =
        phase_row(&table, "go", "chain").unwrap_or_else(|| panic!("no go chain row in\n{table}"));
    assert_eq!(
        chain_calls, 10,
        "the go chain walk entered {chain_calls} sites\n{table}"
    );
    pinned_rows.push(json!([["go_residual/callers.go"], format!("go/chain={chain_calls}")]));

    // A rust file enters no go leg and no resolve leg; the ran-phase rows
    // freeze exactly.
    let table = phases_of(&fixture);
    let mut ran_rows = Vec::new();
    for phase in ["hash", "parse", "family", "tsi_syntax", "write"] {
        let lang = if phase == "write" { "-" } else { "rust" };
        let row = phase_row(&table, lang, phase)
            .unwrap_or_else(|| panic!("no {lang}/{phase} row in\n{table}"));
        assert!(row.0 >= 1, "no {lang}/{phase} row in\n{table}");
        ran_rows.push(json!([lang, phase, row.0, row.1, row.2]));
    }
    for (lang, phase) in [("go", "bind_plan"), ("rust", "resolve_leg")] {
        assert!(
            phase_row(&table, lang, phase).is_none(),
            "{lang}/{phase} ran on a plain rust extract\n{table}"
        );
    }

    // FAIL-FIRST RECEIPT: `--bench` printed one `eprintln!` line per file
    // whose shape no test read, so the numbers were never comparable across
    // runs. `--bench` reports through the summary table instead.
    let (stdout, bench_stderr) = run(
        &["--bench", "--kinds", "call", &fixture],
        &[("DL_TRACE_SUMMARY", None), ("DL_TRAIL", Some("0"))],
    );
    assert!(
        bench_stderr.contains("ryi summary: wall ") && bench_stderr.contains("ryi phases: load "),
        "--bench printed no summary, got {bench_stderr}"
    );
    assert!(
        !bench_stderr.contains(" serial "),
        "the old per-file bench line survives: {bench_stderr}"
    );
    let bench = json!({
        "stdout_lines": stdout.lines().count(),
        "summary_table": true,
        "no_serial_line": true,
    });

    // Both a default run and an early-exit run enter the sqlite trail; the
    // failed cleave still writes its row (wall_ms is wall-clock, >= 0 live).
    let home = tempfile::tempdir().expect("isolated home");
    for args in [&["schema"][..], &["cleave"][..]] {
        let output = Command::new(BIN)
            .args(args)
            .env("RUST_LOG", "error")
            .env_remove("DL_TRACE_SUMMARY")
            .env_remove("DL_TRAIL")
            .env("HOME", home.path())
            .output()
            .expect("run ryi");
        assert_eq!(output.status.success(), args[0] == "schema");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("ryi summary:"));
    }
    let conn = rusqlite::Connection::open(home.path().join(".agent/dl6.db"))
        .expect("default trail database");
    let mut statement = conn
        .prepare("SELECT argv, wall_ms FROM extract_run ORDER BY __id")
        .expect("run rows");
    let rows: Vec<(String, i64)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query runs")
        .collect::<Result<_, _>>()
        .expect("read runs");
    assert_eq!(rows.len(), 2);
    assert!(rows[0].0.ends_with(" schema"));
    assert!(rows[1].0.ends_with(" cleave"));
    assert!(rows.iter().all(|(_, wall_ms)| *wall_ms >= 0));
    let trail = json!({
        "runs": [rows[0].0, rows[1].0],
        "count": rows.len(),
    });

    json!({
        "info_default": info_default,
        "json_format": json_format,
        "summary_layer_rows": layer_rows,
        "sorted_summary_rows": sort_rows,
        "pinned_phase_rows": pinned_rows,
        "ran_phase_rows": ran_rows,
        "bench": bench,
        "trail": trail,
    })
}
