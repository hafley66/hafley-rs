# Lane: `boop config presets --format json` (hafley-rs)

Favor plain code. If reality deviates, STOP and write `plans/2026-09-05-presets-json.REPORT.md` (never a root REPORT.md).

## Why
instant's terminal context menu will list boop presets grouped by harness. It shells out to `boop config presets` and needs a stable machine shape instead of the fixed-width table.

## Files you own (inside $PWD)
- crates/boop/src/main.rs: the `Presets` variant (~:335) gains `#[arg(long, value_enum, default_value = "table")] format: PresetsFormat` with variants `table | json` (reuse an existing `--format` ValueEnum in the crate if one already carries `table`/`json`; `grep -rn 'ValueEnum' crates/boop/src` first).
- crates/boop/src/cli/debug.rs: split `presets_table` (:248) into `presets_rows(registry) -> Result<Vec<PresetRow>>` (struct with `name, harness, model, effort: Option<String>, variant: Option<String>, bin: Option<String>, status: String, default: bool`; `default` = the row the table marks with `*`) and two printers: the existing table and `serde_json::to_string_pretty` of the rows as a JSON array, in config-file order (same order the table prints).
- crates/boop/tests/: one test that `boop config presets --format json` parses as a JSON array and every element has the eight keys; one that `--format table` output is byte-identical to today's (snapshot the current output at the start of your work into the test).
- crates/boop/src/cli/mod.rs help text for `config presets` mentions `--format json`.

Do not touch any other file.

## Rules
1. Field names in JSON are exactly the struct field names above, snake_case.
2. One commit, subject exactly: `boop: config presets --format json`.
3. Banned identifiers: provenance, substrate, load-bearing, regime.

## Validation (paste into the plans/ report)
```
export CARGO_TARGET_DIR=$HOME/.cache/cargo-target/lane-env
cargo test -p boop 2>&1 | grep -E '^test result|FAILED$'
cargo clippy -p boop --all-targets -- -D warnings 2>&1 | grep -E '^error' | head
cargo run -q -p boop -- config presets --format json | head -20
git log --oneline -1
```
Known failures to report, not fix: `deliver_door::a_route_with_a_live_pane_takes_the_paste_rung` (env), `install_rail::the_version_string_carries_the_commit_it_was_built_from` (stale stamp), clippy `unwrap` after `is_some` at job.rs:1270 and debug.rs:186 (pre-existing).
