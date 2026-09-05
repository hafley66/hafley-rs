# Lane A: harness transcript logic into boop-harness

Port of instant's harness transcript logic (`0_harness_store.rs`, `ledger.rs`) into
`boop-harness`, per `plans/2026-09-05-harness-port.BRIEF-A.md`. `chat.rs` retired.
Principal-trait law applied: every harness-specific body lives in its adapter file.

## Moved functions

| instant | boop-harness |
|---|---|
| `HarnessSession` (0_harness_store.rs:15) | `transcript::SessionMeta` |
| `AiMessage` (ledger.rs:66) | `transcript::Message` |
| `Editor` (ledger.rs:28) | `HarnessId` (already exists) |
| `refs` (0_harness_store.rs:115) | `Registry::sessions_in_cwd` |
| `sessions` (0_harness_store.rs:333) | `Registry::describe_all` |
| `session_ids` (0_harness_store.rs:339) | `Registry::session_ids_for_cwd` |
| `messages` (0_harness_store.rs:378) | `Registry::messages_by_id` |
| `claude_shape` (0_harness_store.rs:128) | `Harness::describe` (claude.rs) |
| `codex_shape` (0_harness_store.rs:171) | `Harness::describe` (codex.rs) |
| `kimi_shape` (0_harness_store.rs:258) | `Harness::describe` (kimi.rs) |
| `opencode_shape` (0_harness_store.rs:277) | `Harness::describe` (opencode.rs) |
| `resume_id` (0_harness_store.rs:106) | `Harness::resume_id` (claude overrides) |
| `claude_project_dir` / `claude_session_path` (0_harness_store.rs:354/363) | `harness::claude::{claude_project_dir, claude_session_path}` |
| `boop_mux_session` body (0_harness_store.rs:403) | `live::session_in_pane` |
| `interactive_session_id` (0_harness_store.rs:435) | `live::interactive_session_id` |
| `resolve_registered_session` (0_harness_store.rs:457) | `live::resolve_registered_session` |
| `route_session_in_pane` (0_harness_store.rs:474) | `live::route_session_in_pane` |
| `read_claude` (ledger.rs:680) | `harness::claude::read_claude` |
| `claude_text` / `classify_user_line` / `injected_tag` / `diagram_write_fence` / `content_has_tool_result` / `first_text` / `tool_result_text` (ledger.rs) | `harness::claude` private fns |
| `read_codex` (ledger.rs:118) | `harness::codex::read_codex` |
| `codex_value_text` (ledger.rs:260) | `harness::codex::codex_value_text` |
| `read_kimi` (ledger.rs:277) | `harness::kimi::read_kimi` |
| `kimi_wire_meta` / `wire_num` / `kimi_state_path` (0_harness_store.rs) | `harness::kimi` private fns |
| `read_opencode` (ledger.rs:825) | `harness::opencode::read_opencode` |
| `opencode_message_text` (ledger.rs:774) | `harness::opencode::opencode_message_text` |
| `iso_to_ms` (ledger.rs:546) | `transcript::iso_to_ms` |
| `codex_session_path` (ledger.rs:80) | `harness::codex::codex_session_path` |
| `kimi_session_path` (ledger.rs:102) | `harness::kimi::kimi_session_path` |
| kimi main-agent filter (0_harness_store.rs:124) | `Harness::lists_session` (kimi overrides) |
| `chat.rs` (boop, claude-only, dead) | deleted; `pub mod chat;` removed from boop/src/lib.rs |

`transcript.rs` holds only the harness-neutral surface: the two types, `iso_to_ms`,
`chrono_lite`, `mtime`, `created`, `json`, `head_values`, `tail_values`, `usage`, `cap`,
`preview_of`, `Extracted`.

## Tests ported

24 tests ported with identical names, plus one wire-compat test:

- 7 from `0_harness_store_tests.rs`: `an_existing_guardian_route_resolves_to_the_interactive_parent`,
  `claude_messages_resolve_one_exact_file_without_session_discovery`,
  `four_harnesses_lower_into_one_session_shape`, `opencode_tokens_take_max_not_latest`,
  `an_archived_opencode_session_is_not_a_row`, `kimi_wire_usage_sums_inputs`,
  `a_resume_id_is_the_stem_for_claude_and_the_session_id_elsewhere`.
- 17 from `ledger.rs` tests: `title_lookup_skips_leading_tool_and_meta_rows`,
  `string_content_is_a_user_row`, `tool_result_block_is_a_tool_row`,
  `meta_string_content_is_a_meta_row`, `task_notification_is_a_meta_row`,
  `compact_summary_is_a_meta_row`, `legacy_command_body_is_a_meta_row`,
  `typed_and_queued_input_stay_user_rows`, `prose_about_a_tag_stays_a_user_row`,
  `meta_text_block_is_a_meta_row`, `interrupted_message_stays_a_user_row`,
  `assistant_line_is_unaffected`, `assistant_d2_write_retains_the_complete_fenced_file`,
  `seq_is_the_line_index_across_skipped_lines`, `after_seq_skips_up_to_and_including_that_index`,
  `captured_claude_session_labels_every_row_by_its_wire_shape`,
  `captured_claude_subagent_session_parses`.
- Wire test (plan section 4): `wire_shapes_pin_the_instant_key_set`.

## Principal-trait law acceptance

`grep -n "HarnessId::" crates/boop-harness/src/transcript.rs crates/boop-harness/src/registry.rs crates/boop-harness/src/live.rs`:

- `transcript.rs`: no matches.
- `registry.rs`: line 51 (`HarnessId::parse` in pre-existing `by_name`), and lines 143-205 (pre-existing `mod tests`). None are new production code introduced by this port.
- `live.rs`: line 196 (`harness: HarnessId::Claude` in the pre-existing `mod tests` LiveSession helper). Required to construct a `LiveSession`.

`grep -rn "match session.harness" crates/boop-harness/src/` outside adapter files: none. The one test that shaped all four harnesses (`four_harnesses_lower_into_one_session_shape`) now dispatches through `Registry::get(...).describe(...)`.

## Validation

```
$ CARGO_TARGET_DIR=$HOME/.cache/cargo-target/harness-port cargo test -p boop-harness 2>&1 | tail -30
test result: ok. 165 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 5.72s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ CARGO_TARGET_DIR=$HOME/.cache/cargo-target/harness-port cargo build -p boop 2>&1 | tail -5
warning: `boop` (bin "boop") generated 1 warning   # pre-existing dead_code in cli/debug.rs:186
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.53s

$ CARGO_TARGET_DIR=$HOME/.cache/cargo-target/harness-port cargo clippy -p boop-harness --all-targets -- -D warnings 2>&1 | tail -20
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.07s
```

All three green. The `boop` build warning is pre-existing on HEAD, unrelated to chat.rs removal.

## Deviations from the brief

- **Test count**: the brief said `0_harness_store_tests.rs` has 8 tests; it actually has 7. All 7 ported.
- **Fixtures**: the two `captured_claude_*` tests depend on transcript fixture files that only exist
  in the instant repo. Their `claude/session.jsonl` and `claude/subagent.jsonl` were copied into
  `boop-harness/tests/fixtures/transcripts/claude/` (data only, instant repo untouched); the
  `fixture()` helper was pointed at that path.
- **tool_result_text placement**: the brief assigned `tool_result_text` to codex.rs, but it is used
  only by `claude_text` (ledger.rs:525), never by `read_codex`. Placed it in claude.rs with its
  sole consumer; noted so the coordinator can reassign if the intent differed.
- **`codex_value_text` cross-file**: `read_kimi` uses `codex_value_text` (ledger.rs:374/389), which
  lives in codex.rs per the brief; kimi.rs references it as `super::codex::codex_value_text`.
- **Private-fn privacy**: the readers are `pub(crate)` within their adapter module (not strictly
  private `fn`) so `transcript_tests.rs` (which per the brief holds every ported test) can call
  them. They remain module-level fns beside each `impl Harness`, not trait methods.
- **Clippy fixes** for the `-D warnings` gate: removed an unnecessary `as i64` cast in
  `chrono_lite`; reflowed an over-indented doc comment in `transcript.rs`; fixed a pre-existing
  `needless_borrow` in opencode.rs:480 that was already failing on base HEAD.
- No `Cargo.toml` change needed: `rusqlite`, `serde`, `serde_json` are already deps of
  `boop-harness`.
