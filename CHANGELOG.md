# changelog

all notable changes to `boop` and `boop-mux` are recorded here. versions follow semantic versioning.

## [0.0.1] - 2026-08-13

### added

- conventional `tracing` events across lane creation, supervision, transcript synchronization, harness channels, and tmux operations.
- one `tracing-subscriber` initialization point in the `boop` binary with `RUST_LOG` filtering and stderr output.
- structured fields for lane, harness, model, working directory, tmux target, conversation identity, turn completion, OpenCode message state, and exit code.

### known defects

- TUI lane completion uses a stable pane body as its completion signal. A stalled OpenCode turn can therefore be interrupted and reported as successful.
- TUI lanes record their tmux target as the conversation identity, preventing OpenCode transcript lookup by its `ses_*` id.

[0.0.1]: https://github.com/hafley66/hafley-rs/releases/tag/v0.0.1
