# boop: config presets --format json

## What
`boop config presets` now takes `--format table|json`. JSON prints the preset
rows as an array with the eight fields `name, harness, model, effort, variant,
bin, status, default`, in the same order the table prints, so instant's context
menu can group presets by harness without parsing the fixed-width table.

## Where it is now
- `crates/boop/src/main.rs`: `ConfigCmd::Presets` gains `format: PresetsFormat`
  (`table | json`, default `table`); `PresetsFormat` ValueEnum added.
- `crates/boop/src/cli/debug.rs`: `presets_table` split into
  `presets_rows(registry) -> Result<Vec<PresetRow>>` (new `PresetRow` struct,
  `default` = the row the table marks `*`), the existing table printer, and
  `presets_json` (`serde_json::to_string_pretty` of the rows).
- `crates/boop/src/cli/mod.rs`: help text for `config presets` mentions
  `--format json` in both the lane doc and the PRESETS doctrine block.
- `crates/boop/tests/presets_json.rs` (wired into `tests/main.rs`): two tests,
  JSON array shape with all eight keys, and the table output byte-identical to
  the pre-change snapshot.

## Validation
```
$ export CARGO_TARGET_DIR=$HOME/.cache/cargo-target/lane-env
$ cargo test -p boop 2>&1 | grep -E '^test result|FAILED$'
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 84 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.58s
test result: FAILED. 104 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.42s
$ cargo clippy -p boop --all-targets -- -D warnings 2>&1 | grep -E '^error' | head
error: function `run_host` is never used
error: called `unwrap` on `harness` after checking its variant with `is_some`
error: could not compile `boop` (bin "boop") due to 2 previous errors
$ cargo run -q -p boop -- config presets --format json | head -20
[
  {
    "name": "astra",
    "harness": "codex",
    "model": "gpt-6-astra",
    "effort": "high",
    "variant": null,
    "bin": null,
    "status": "ok",
    "default": false
  },
  {
    "name": "fable",
    "harness": "claude",
    "model": "claude-fable-5",
    "effort": "high",
    "variant": null,
    "bin": null,
    "status": "ok",
    "default": false
$ git log --oneline -1
86c13ca plans: presets-json lane brief
```

The new `presets_json` tests pass: `presets_json::json_format_is_a_full_array_of_preset_rows ... ok`, `presets_json::table_output_is_byte_identical_to_the_snapshot ... ok`.

## Known failures (not fixed, per brief)
- `deliver_door::a_route_with_a_live_pane_takes_the_paste_rung` (env).
- Three `tell::` failures (env; reproduced on base commit 86c13ca before these
  changes): `a_caller_with_no_parent_edge_and_no_registered_coordinator_fails_by_name`,
  `a_caller_with_no_recorded_parent_falls_back_to_the_one_registered_coordinator`,
  `beep_parent_with_no_edge_fails_by_name_instead_of_addressing_the_word`.
- Clippy `-D warnings`: `run_host` dead code at debug.rs:186 and `unwrap` after
  `is_some` at job.rs:1305 (both pre-existing; reproduced on base 86c13ca).
- `install_rail::the_version_string_carries_the_commit_it_was_built_from` did not
  run in this lane (stale stamp; reported in the brief).

## Next action
Commit as `boop: config presets --format json`.
