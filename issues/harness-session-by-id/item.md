---
created: 2026-08-22
updated: 2026-09-27
type: improvement
status: done
priority: normal
epic: harness-interface
related: ['@instant-harness-store-dedupe']
labels: [domain-boop]
closed: 2026-09-27
closed_by: codex
---

# Harness::session_by_id and sessions_for_cwd, one session without a full root walk

## Description

`Harness::sessions()` walks every transcript under the root (~1300 claude files, a first-line read each). instant's turn watcher asks for one session's messages per poll, so `claude_project_dir` + `claude_session_path` survive in `instant/src-tauri/src/0_harness_store.rs:354-380` as a boop-harness gap. Add `fn session_by_id(&self, session_id: &str) -> Result<Option<SessionRef>>` and `fn sessions_for_cwd(&self, cwd: &str) -> Result<Vec<SessionRef>>` on `Harness` (claude: one project dir; codex: `state_5.sqlite` threads by cwd; opencode: its session index).

## Acceptance Criteria

- [x] `Harness` provides fallible `session_by_id` and cwd-indexed `sessions_for_cwd` lookup methods; `Registry` uses cwd indexes.
- [x] Claude lookup resolves a direct or subagent transcript under only the cwd-encoded project; the budget test counts one project-directory read for a subagent lookup.
- [x] Codex lookup reads the exact thread from `state_5.sqlite`; cwd filtering queries the thread index.
- [x] OpenCode uses its session store query for exact id and cwd; other adapters preserve fallible lookup semantics.
- [x] The two Instant helpers named in the original report are absent from `instant/src-tauri/src` (verified read-only on 2026-09-27).

## Tests Run

- [x] `CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo nextest run -p boop-harness -j 2 -E 'test(/(session_lookup_uses_the_codex_thread_index|cwd_session_lookup_uses_the_adapter_index|session_by_id_checks_only_its_encoded_project)/)'` (3 passed).
- [x] `CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo nextest run -p boop -j 2 -E 'test(/__harness_api_compile_probe_no_such_test__/)'` (all 4 Boop test binaries compiled; 0 tests selected).

## Implementation Notes

`Harness::session_by_id` now returns `Result<Option<SessionRef>>`; adapters propagate lookup errors. Claude performs an exact direct-file check, then at most one project-directory read for subagents. Codex uses `state_5.sqlite` thread rows instead of recursively searching transcript paths. OpenCode exact-id and cwd queries use its SQLite session index. `Registry::sessions_in_cwd` dispatches through adapter indexes. CLI and integration-test call sites now handle the fallible optional result.

## Resolution

### 2026-09-27T04:23:18Z · @codex

Targeted adapter tests pass; Boop package test binaries compile against the fallible lookup API. Instant helper absence confirmed by read-only source search.
