# changelog

all notable changes to `boop` and `boop-mux` are recorded here. versions follow semantic versioning.

## unreleased

## [0.0.3](https://github.com/hafley66/hafley-rs/compare/boop-v0.0.2...boop-v0.0.3) - 2026-08-24

### Added

- *(boop)* lane create and lane run take --bin
- *(boop-acp)* a channel spec names the executable its harness runs as
- *(config)* model presets carry an executable override
- *(boop)* favorite show, edit, delete
- *(boop)* a wait on a door hail ends when the recipient's turn ends
- *(boop)* print harness shell wrappers
- *(boop)* launch native harness TUIs through adapters
- *(boop)* share native Codex app-server control
- *(boop)* run coordinators through ACPX
- *(boop-acp)* claude, codex, kimi lanes ride the ACP channel ([#42](https://github.com/hafley66/hafley-rs/pull/42))
- *(boop)* boop-start detection, the spawn's status line, and the lane's preamble ([#36](https://github.com/hafley66/hafley-rs/pull/36))
- parent-death policy and typed parent failure mail ([#34](https://github.com/hafley66/hafley-rs/pull/34))
- *(boop)* tell-parent and tell-children ([#33](https://github.com/hafley66/hafley-rs/pull/33))
- *(boop)* per-session mood, mail rendered through the receiver's format ([#31](https://github.com/hafley66/hafley-rs/pull/31))
- *(boop)* claude coordinators read their mail at a turn boundary
- *(boop)* flag a lane whose commits escape its registered worktree
- *(boop)* expose agent session graph
- *(boop)* resolve lane runtime identity
- add revision-pinned source tooling

### Fixed

- *(boop)* respawn the native codex TUI after daemon transport loss
- *(boop)* a preset opts into its harness, so --preset opus spawns a claude lane without --harness
- *(boop)* codex and opencode doors land live; door-e2e.sh proves all three with no keystrokes
- *(boop)* one transcript sync pass across concurrent readers
- *(boop)* BOOP_NO_SYNC=1 skips the startup transcript sync
- *(boop)* stop native TUI poll loop burning a core per wrapper
- *(boop)* do not abort startup sync on native child completion delivery failure
- *(boop)* dedupe native Codex completion delivery
- *(boop)* inspect native Codex thread creation
- *(boop)* speak websocket to Codex control socket
- *(boop)* precreate native Codex parent thread
- *(boop)* bind native parent to live TUI
- *(boop)* a lane that ends its turn is idle, never dead
- *(boop)* use supported Codex remote commands
- *(boop)* register Codex thread before TUI launch
- *(boop)* the caller's own identity resolves from an adopted pane ([#43](https://github.com/hafley66/hafley-rs/pull/43))
- *(boop)* reclaim a dead lane's worktree and branch, close the wal-lock and spawn-flake cards ([#35](https://github.com/hafley66/hafley-rs/pull/35))
- *(boop)* sync before reads ([#28](https://github.com/hafley66/hafley-rs/pull/28))
- *(boop)* replace run with host chat ([#27](https://github.com/hafley66/hafley-rs/pull/27))
- *(boop)* install only from origin/main, and stamp the sha into --version
- *(boop)* a respawned window is re-fed the supervisor's brief, prefaced
- *(boop)* [**breaking**] give every lane an on-disk trail and every death a reason
- *(boop)* claude lanes report rc=0 on completion, not a false 30s stall
- *(boop)* remove graph edge identity fallback
- *(boop)* tighten graph module and identities
- *(boop)* correct session graph identity and discovery
- *(boop)* retain compiled model routing fallbacks
- *(boop)* resume stalled lane conversations
- release boop 0.0.2 opencode completion

### Other

- *(boop)* hail help describes doors, not keystrokes
- *(boop)* plan §8 door row is door-e2e.sh; session log
- merge fix/boop-db-convoy into refactor/harness-interface
- *(boop)* research, driving native agent TUIs from outside
- *(boop)* review 2026-08-22 against ACP, acpx, agent teams, A2A
- project native child completion events
- Route agent mail through native harness control
- *(boop)* five crates: store, harness, acp, proc, cli ([#41](https://github.com/hafley66/hafley-rs/pull/41))
- main.rs becomes a clap tree plus cli/{mod,job,mail,me,db,debug}.rs ([#40](https://github.com/hafley66/hafley-rs/pull/40))
- the ACP lane channel, on the official agent-client-protocol crate ([#39](https://github.com/hafley66/hafley-rs/pull/39))
- name the two src unit tests that write into the live ~/.agent ([#37](https://github.com/hafley66/hafley-rs/pull/37))
- registry verbs stop syncing, tests get a temp HOME, result rows carry a typed rc ([#32](https://github.com/hafley66/hafley-rs/pull/32))
- bound the spawn path's git children, close both spawn-guard cards ([#29](https://github.com/hafley66/hafley-rs/pull/29))
- anchor idle Codex families from live processes
- run resident coroutine programs ([#26](https://github.com/hafley66/hafley-rs/pull/26))
- root focused graphs at live pane coordinators
- focus native families after runtime anchoring
- bound transcript writer transactions
- retry concurrent WAL initialization
- serialize schema initialization separately from opens
- anchor adopted Claude sessions to tmux families
- wire focused family filters through agent sessions
- refactor boop identity rungs into harnesses
- re-feed briefs after first-turn flakes
- sync before local commands
- reconcile pane-backed parent receipts
- project focused session families
- project unmatched harness routes into shells
- stamp children with the resolved parent lane
- bind fresh Codex spawners as parents
- decouple foreground waits from coordinators
- `boop debug` and a warn/error banner on --help
- one stall bound per turn, and a named error for a closed rpc session
- one completion row per lane exit
- Merge pull request #21 from hafley66/fix/boop-harness-model-spec
- Merge pull request #22 from hafley66/fix/boop-main-cli
- read-only db opens, db help, ProcReader seam, doctrine version, bench-grid untracked
- Merge pull request #18 from hafley66/chore/issues-sync-20260817
- Merge pull request #17 from hafley66/audit/boop-review
- favorite caller assistant messages
- expose trace events in session graph
- persist lane tracing events
- keep pane and native routes live
- *(boop)* give the streamed-activity probe a deadline, not 20 polls
- *(boop)* pin the escape fixture branch to main
- Merge remote-tracking branch 'origin/main'
- drop a duplicated clippy allow attribute
- event-based turn lifecycle, --store, e2e receipts
- Fix fresh Codex lanes dropping their brief
- Merge remote-tracking branch 'origin/main' into fix/boop-codex-pane-identity
- 'boop me' registers a Codex pane as a coordinator route
- Merge pull request #5 from hafley66/fix/boop-tui-respawn-2
- rustfmt the tui respawn refeed lines
- tui respawn re-feeds the brief and captures the session before the first death
- flake resume re-feeds the brief when no conversation is pinned
- Batch Claude cursor metadata writes
- Scope agent summary activity reads
- Stream OpenCode batch parts
- Scope runtime trace-span batch query
- Batch runtime snapshot durable reads
- Batch OpenCode message parts
- Scope session graph activity aggregates
- Add Boop performance regressions and fold mailbox state once
- Speed up native agent session projection
- Merge feature/agent-pipe: native concatmap resident
- boop config presets: preset table with model, variant, harness, default marker
- Merge feature/agent-session-graph: expose native session graph
- *(boop)* plan native agent session graph
- *(boop)* finish tmux trait merge gates
- Merge chore/tmux-trait-closure: no test speaks raw shell to tmux
- close the Multiplexer trait, migrate test call sites off raw tmux shell
- pass --variant through to opencode lanes
- Merge branch 'fix/lane-window-zero'
- the tui channel reports store activity and respawns a dead agent window
- *(boop)* map Instant agent projection work
- Merge branch 'feature/lane-runtime-identity'
- boop 0.0.1 tracing baseline
- pin user favorites into the store as markdown
- flake tell: treat a finishless newest row as a dropped stream
- resume a lane turn when opencode run exits 0 on a dropped stream
- port the create --wait exit-contract tests from sprefa PR #228
- boop and boop-mux: extract from sprefa into a standalone workspace
- *(boop-harness)* six identity and TUI-driving methods off trait Harness
- sync 2026-08-17 boop cards, session logs, plans
- hand a pane its body as a paste, not typed keys
- new-window targets name the window id, not its index

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
