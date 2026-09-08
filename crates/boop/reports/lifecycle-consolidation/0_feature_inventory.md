# Boop feature inventory

Work in progress, based on `66cbe8e`. Worktree: `refactor/boop-lifecycle-consolidation`.
Installed help and isolated reproduction receipts live under
`/private/tmp/boop-lifecycle-consolidation-proof-01a08191`.

| Surface | Observed path | Regression / status |
| --- | --- | --- |
| Register coordinator/native | `main.rs::AgentCmd` → `cli/job.rs::run_agent` → `cli/me.rs::register_route` | Session/pane options added; merge preserves omitted metadata; 8 registry regressions pass |
| Attach existing pane | `beep lane patch` → `cli/me.rs::register_route` | Compatibility entry shares registration; `%pane` works; missing target errors; existing kind preserved |
| Delivery | `beep` → `cli/mail.rs::deliver_hail` → `boop-proc::deliver::land` | Lane routes short circuit to supervisor; incident reproduced |
| Shell wrapper | `shell-init bash` → `boop_wrap` → `boop tui` → `run_native_tui` | Codex/Claude/ccz/Kimi/OpenCode share the path inside and outside tmux; forwarding and exit regression passes |
| Interactive lifecycle | `run_native_tui` → `Door::tui_launch` → adapter observation → existing route/session store | Actual Codex start/resume/settings/clear responses observed; raw live receipts in report 2 |

Complete CLI/help/test matrix and canonical paths remain in progress.
Recursive installed help capture includes 101 pages (see `3_help-manifest.json`).

## Registration consolidation

`beep agent register` and compatibility `beep lane patch` now call
`cli/me.rs::register_route`. Explicit `--session-id` and `--tmux` are supported
by registration. Supplied fields merge under the existing store transaction;
omitted fields and route kind survive. A new pane attachment is a coordinator.
Existing lane ownership survives patching. Invalid targets return an error.

Removed `run_adopt`, `run_adopt_with`, duplicate pane discovery/fallback and
unreachable adoption hook-removal branch. Hook management remains `inbox hooks`.
Removed the CLI copy of route serialization. Callers use
`boop-store::bus::route_to_value`; legacy registry field spellings remain a
compatibility obligation. Ordinary `write_route` uses the single-row store
upsert instead of rewriting the complete registry through CAS.

Initial incident regressions: 3 failures before changes; `registry_kinds` after
changes: 8 passed, 0 failed. Registration helper unit tests: 3 passed;
coordinator compatibility tests: 4 passed.

## Native launch consolidation

The generated shell functions previously had separate tmux and direct-executable
branches. Both now use `boop tui`; default route names use pane or process identity
so simultaneous launches in one cwd do not overwrite each other. Parent linkage
is retained on process resume. All five entries, including the `ccz` executable
alias, have forwarding and exit-code coverage. Bash is the supported shell and
the user's configured variant; authenticated trials used `/opt/homebrew/bin/bash`.

Codex previously used a shared daemon and cwd/time discovery. Its adapter now
owns a private backend and observes the real TUI connection. Actual selected
thread responses bind the route; subscribed settings update model and effort.
`LiveSessions::live_session_for_route` owns endpoint interpretation, and the
delivery code delegates to it. Other adapters retain their existing deterministic
contracts. Their discovery paths and remaining competing liveness writes are
listed in report 3 for the next consolidation chunk.
