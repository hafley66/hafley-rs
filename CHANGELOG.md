# changelog

all notable changes to `boop` and `boop-mux` are recorded here. versions follow semantic versioning.

## unreleased

## [0.0.3] - 2026-08-24

### Fixed

- *(boop)* the caller's own identity resolves from an adopted pane (#43)
- release boop 0.0.2 opencode completion

### Other

- update Cargo.toml dependencies
- *(boop-harness)* six identity and TUI-driving methods off trait Harness
- decouple foreground waits from coordinators
- Merge pull request #18 from hafley66/chore/issues-sync-20260817
- sync 2026-08-17 boop cards, session logs, plans
- hand a pane its body as a paste, not typed keys
- 'boop me' registers a Codex pane as a coordinator route
- Merge feature/agent-pipe: native concatmap resident
- new-window targets name the window id, not its index
- boop 0.0.1 tracing baseline
- boop and boop-mux: extract from sprefa into a standalone workspace

### added

- `boop --preset codex` runs a foreground coordinator on a persistent ACPX session.
- coordinator hails enter the same ACPX prompt queue and receive a mailbox acknowledgment after queue admission.

## [0.0.2] - 2026-08-13

### fixed

- OpenCode TUI lanes resolve and retain the harness `ses_*` conversation id instead of recording a tmux pane target.
- an idle OpenCode pane completes only after the newest assistant message records `finish=stop` without an error.
- finishless or errored OpenCode messages produce a retryable failed turn instead of a successful lane result.
- intermediate `finish=tool-calls` state remains active, preventing the supervisor from sending `C-c` during tool execution.

## [0.0.1] - 2026-08-13

### added

- conventional `tracing` events across lane creation, supervision, transcript synchronization, harness channels, and tmux operations.
- one `tracing-subscriber` initialization point in the `boop` binary with `RUST_LOG` filtering and stderr output.
- structured fields for lane, harness, model, working directory, tmux target, conversation identity, turn completion, OpenCode message state, and exit code.

### known defects

- TUI lane completion uses a stable pane body as its completion signal. A stalled OpenCode turn can therefore be interrupted and reported as successful.
- TUI lanes record their tmux target as the conversation identity, preventing OpenCode transcript lookup by its `ses_*` id.

[0.0.1]: https://github.com/hafley66/hafley-rs/releases/tag/v0.0.1
[0.0.2]: https://github.com/hafley66/hafley-rs/releases/tag/v0.0.2
