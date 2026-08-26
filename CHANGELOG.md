# changelog

all notable changes to `boop` and `boop-mux` are recorded here. versions follow semantic versioning.

## unreleased

## [0.0.5] - 2026-08-25

### Added

- *(boop)* typed lane completion: `lane create --expect-path <rel>`, `--expect-commit-subject <text>`, `--expect-commits-at-least <n>`; a clean exit with an unmet assertion is rewritten to rc=4 with the failed assertions in the row's detail; `lane get` prints the `expect` field
- *(boop)* `lane list --all` shows unregistered tmux sessions and claude Agent-tool worktrees, with measured liveness for pane-less routes
- *(boop)* the session graph resolves lanes it could not before; the resolved lane shell is dropped from the graph to avoid duplication

### Changed

- *(boop)* the 49 hidden verb aliases (Dispatch, Lane, Chat, Sync, Follow and the nested variants) are deleted; the 9 documented verbs are the whole surface
- *(boop)* help doctrine: spawn examples spell `--preset`, the stale `--model` and `--harness` flags are gone, every `lane list` and `lane create` flag has help text

### Fixed

- *(boop)* `boop wait <lane>` ignores result rows older than the newest taken inbound row
- *(codex)* bookkeeping records leave no row and no WARN; reasoning summaries project as assistant rows
- *(harness)* the setup-step deadline polls `try_wait` and kills the process group with `killpg`
- *(boop)* supervise unit tests pin `BOOP_DB` and `HOME` in a tempdir; fixture lanes no longer leak into the live store

## [0.0.4] - 2026-08-25

Epic boop-one-path: one path per job. The supervisor mails the parent on every
turn end, commit and exit; one send verb; presets-only model spelling; one
sqlite mailbox; identity is `--as` then `BOOP_SESSION`; top-level verbs cut
18 -> 9.

### Added

- *(boop)* `boop beep <route> <body>` is the one send: a delivery ladder (door, turn boundary, hook inbox, pane, mailbox) and every rung leaves a transition receipt
- *(boop)* `boop wait <id|lane|--me>` is the one wait verb; `beep` blocks on its answer
- *(boop)* `boop debug <lane>` answers what happened in five sections
- *(boop)* the supervisor reports every turn end and every HEAD move to the parent
- *(boop)* `boop tui <harness>` is the one pane-register path and stamps `BOOP_SESSION`
- *(boop)* `lane delete --state dead [--dry-run]` removes each dead lane's own worktree and nothing above it
- *(store)* one sqlite mailbox (`agent_mail`, `agent_route`) replaces bus.ndjson + registry.json; legacy files are tailed, never claimed
- *(acp)* kimi lanes get a shell: the five `terminal/*` methods are served and advertised
- *(harness)* kimi transcripts keep tool and assistant bodies
- *(observability)* one Rust tracing configuration across the crates

### Changed

- *(boop)* presets are the only model spelling (`boop config presets`); `gem37` is dead
- *(boop)* codex lanes launch through codex-acp only; `codex exec` is gone
- *(boop)* `agent register` prints the `--as` instruction; a bare `wait --me` under a lane stamp shared with native children is refused, naming them
- *(boop)* claude coordinators take rows at the door; the hook inbox is a hidden fallback rung
- *(boop)* concatmap and host verbs sit behind feature `dl6`
- *(proc)* the outbound reconciler is gone

### Fixed

- *(codex)* process-level sandbox and approval config reach native subagents
- *(boop)* `wait --me` skips dispatch rows and rows a rung already took
- *(boop)* opencode and codex tool turns keep their names, inputs and outputs
- *(mux)* a failed tmux spawn is an error, never a dispatched line
- *(tui)* `BOOP_SESSION` reaches the harness process; shell-init visible again

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
