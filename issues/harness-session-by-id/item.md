---
created: 2026-08-22
updated: 2026-08-22
type: improvement
status: open
priority: normal
epic: harness-interface
related: ['@instant-harness-store-dedupe']
labels: [domain-boop]
---

# Harness::session_by_id and sessions_for_cwd, one session without a full root walk

## Description

`Harness::sessions()` walks every transcript under the root (~1300 claude files, a first-line read each). instant's turn watcher asks for one session's messages per poll, so `claude_project_dir` + `claude_session_path` survive in `instant/src-tauri/src/0_harness_store.rs:354-380` as a boop-harness gap. Add `fn session_by_id(&self, session_id: &str) -> Result<Option<SessionRef>>` and `fn sessions_for_cwd(&self, cwd: &str) -> Result<Vec<SessionRef>>` on `Harness` (claude: one project dir; codex: `state_5.sqlite` threads by cwd; opencode: its session index). Acceptance: instant deletes those two helpers; a budget test bounds `session_by_id` to one directory read.
